// SPDX-License-Identifier: GPL-2.0-or-later OR Apache-2.0
// Copyright (c) Viacheslav Bocharov <v@baodeep.com> and JetHome (r)

//! Shared types for Ethernet PHY drivers.
//!
//! The names ([`Speed`], [`Duplex`], [`LinkState`]) and variant
//! identifiers (`Speed::_10M`, `Speed::_100M`) intentionally mirror
//! the upstream `esp_hal::ethernet::mac` shape so the bridge between
//! a [`PhyDriver`](crate::PhyDriver) implementation and an
//! `esp_hal::ethernet::phy::Phy` adapter is a near-identity
//! transform — no field-by-field rebuild on the hot path.

/// Ethernet link speed.
///
/// Marked `#[non_exhaustive]` so future variants (e.g. `_1000M` for
/// gigabit-capable PHYs) can land as a non-breaking minor release —
/// downstream `match` expressions are required to keep a wildcard arm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[non_exhaustive]
pub enum Speed {
    /// 10 Mbit/s
    _10M,
    /// 100 Mbit/s
    _100M,
}

/// Ethernet duplex mode.
///
/// Marked `#[non_exhaustive]` for symmetry with [`Speed`] — downstream
/// `match` arms must include a wildcard so a future variant can land
/// in a minor release.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[non_exhaustive]
pub enum Duplex {
    /// Half duplex
    Half,
    /// Full duplex
    Full,
}

/// Negotiated link state reported by the PHY.
///
/// Shape matches `esp_hal::ethernet::mac::LinkState`; the `up` flag
/// signals whether a carrier is present. When `up == false` the
/// `speed` and `duplex` fields are unspecified — they may hold the
/// last known good values or any default the driver chose. Callers
/// must check `up` before acting on the negotiated parameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct LinkState {
    /// Whether the link is currently established (carrier present).
    pub up: bool,
    /// Negotiated link speed. Valid only when `up == true`.
    pub speed: Speed,
    /// Negotiated duplex mode. Valid only when `up == true`.
    pub duplex: Duplex,
}

impl LinkState {
    /// Construct a `LinkState` reporting the link as down. The
    /// `speed` / `duplex` fields are set to a deterministic default
    /// (`Speed::_100M`, `Duplex::Full`) so the value compares equal
    /// across calls; callers must not act on those fields while
    /// `up == false`.
    pub const fn down() -> Self {
        Self {
            up: false,
            speed: Speed::_100M,
            duplex: Duplex::Full,
        }
    }

    /// Construct an "up" `LinkState` with the given parameters.
    pub const fn up(speed: Speed, duplex: Duplex) -> Self {
        Self {
            up: true,
            speed,
            duplex,
        }
    }
}

/// PHY hardware capabilities.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct PhyCapabilities {
    /// 100BASE-TX full duplex
    pub speed_100_fd: bool,
    /// 100BASE-TX half duplex
    pub speed_100_hd: bool,
    /// 10BASE-T full duplex
    pub speed_10_fd: bool,
    /// 10BASE-T half duplex
    pub speed_10_hd: bool,
    /// Auto-negotiation supported
    pub auto_negotiation: bool,
    /// PAUSE flow control supported
    pub pause: bool,
}

#[cfg(test)]
mod tests {
    extern crate alloc;

    use super::*;
    use alloc::format;

    #[test]
    fn link_state_up_constructor() {
        let ls = LinkState::up(Speed::_100M, Duplex::Full);
        assert!(ls.up);
        assert_eq!(ls.speed, Speed::_100M);
        assert_eq!(ls.duplex, Duplex::Full);
    }

    #[test]
    fn link_state_down_constructor() {
        let ls = LinkState::down();
        assert!(!ls.up);
    }

    #[test]
    fn link_state_equality() {
        let a = LinkState::up(Speed::_10M, Duplex::Half);
        let b = LinkState::up(Speed::_10M, Duplex::Half);
        let c = LinkState::up(Speed::_100M, Duplex::Full);
        assert_eq!(a, b);
        assert_ne!(a, c);
        // `down()` returns a deterministic value
        assert_eq!(LinkState::down(), LinkState::down());
    }

    #[test]
    fn link_state_all_up_combinations() {
        let combos = [
            LinkState::up(Speed::_10M, Duplex::Half),
            LinkState::up(Speed::_10M, Duplex::Full),
            LinkState::up(Speed::_100M, Duplex::Half),
            LinkState::up(Speed::_100M, Duplex::Full),
        ];
        for i in 0..combos.len() {
            for j in (i + 1)..combos.len() {
                assert_ne!(combos[i], combos[j], "combo {i} == combo {j}");
            }
        }
    }

    #[test]
    fn phy_capabilities_default_all_false() {
        let caps = PhyCapabilities::default();
        assert!(!caps.speed_100_fd);
        assert!(!caps.speed_100_hd);
        assert!(!caps.speed_10_fd);
        assert!(!caps.speed_10_hd);
        assert!(!caps.auto_negotiation);
        assert!(!caps.pause);
    }

    #[test]
    fn phy_capabilities_equality() {
        let a = PhyCapabilities {
            speed_100_fd: true,
            auto_negotiation: true,
            ..Default::default()
        };
        let b = PhyCapabilities {
            speed_100_fd: true,
            auto_negotiation: true,
            ..Default::default()
        };
        let c = PhyCapabilities::default();
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn speed_clone_copy() {
        let s = Speed::_100M;
        let s2 = s;
        let s3 = Clone::clone(&s);
        assert_eq!(s, s2);
        assert_eq!(s, s3);
    }

    #[test]
    fn duplex_clone_copy() {
        let d = Duplex::Full;
        let d2 = d;
        let d3 = Clone::clone(&d);
        assert_eq!(d, d2);
        assert_eq!(d, d3);
    }

    #[test]
    fn link_state_debug() {
        let ls = LinkState::up(Speed::_100M, Duplex::Full);
        let dbg = format!("{:?}", ls);
        assert!(dbg.contains("_100M"), "debug missing Speed: {dbg}");
        assert!(dbg.contains("Full"), "debug missing Duplex: {dbg}");
        assert!(dbg.contains("up: true"), "debug missing up flag: {dbg}");
    }
}
