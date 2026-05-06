// SPDX-License-Identifier: GPL-2.0-or-later OR Apache-2.0
// Copyright (c) Viacheslav Bocharov <v@baodeep.com> and JetHome (r)

//! Shared types for Ethernet PHY drivers.

/// Ethernet link speed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Speed {
    /// 10 Mbps
    Mbps10,
    /// 100 Mbps
    Mbps100,
}

/// Ethernet duplex mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Duplex {
    /// Half duplex
    Half,
    /// Full duplex
    Full,
}

/// Negotiated or configured link parameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct LinkStatus {
    /// Link speed
    pub speed: Speed,
    /// Duplex mode
    pub duplex: Duplex,
}

impl LinkStatus {
    /// Create a new link status.
    pub const fn new(speed: Speed, duplex: Duplex) -> Self {
        Self { speed, duplex }
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
    fn link_status_new() {
        let ls = LinkStatus::new(Speed::Mbps100, Duplex::Full);
        assert_eq!(ls.speed, Speed::Mbps100);
        assert_eq!(ls.duplex, Duplex::Full);
    }

    #[test]
    fn link_status_equality() {
        let a = LinkStatus::new(Speed::Mbps10, Duplex::Half);
        let b = LinkStatus::new(Speed::Mbps10, Duplex::Half);
        let c = LinkStatus::new(Speed::Mbps100, Duplex::Full);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn link_status_all_combinations() {
        let combos = [
            LinkStatus::new(Speed::Mbps10, Duplex::Half),
            LinkStatus::new(Speed::Mbps10, Duplex::Full),
            LinkStatus::new(Speed::Mbps100, Duplex::Half),
            LinkStatus::new(Speed::Mbps100, Duplex::Full),
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
        let s = Speed::Mbps100;
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
    fn link_status_debug() {
        let ls = LinkStatus::new(Speed::Mbps100, Duplex::Full);
        let dbg = format!("{:?}", ls);
        assert!(dbg.contains("Mbps100"), "debug missing Speed: {dbg}");
        assert!(dbg.contains("Full"), "debug missing Duplex: {dbg}");
    }
}
