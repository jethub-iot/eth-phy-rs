// SPDX-License-Identifier: GPL-2.0-or-later OR Apache-2.0
// Copyright (c) Viacheslav Bocharov <v@baodeep.com> and JetHome (r)

//! IEEE 802.3 Clause 22 standard register constants and helper functions.
//!
//! This module provides:
//! - Register address constants (`regs`)
//! - Bit-field constants for BMCR, BMSR, ANAR, ANLPAR registers
//! - Helper functions for common PHY operations (reset, link check,
//!   auto-negotiation, ID read, capability query, forced link)

use crate::mdio::MdioBus;
use crate::types::{Duplex, LinkStatus, PhyCapabilities, Speed};

/// IEEE 802.3 Clause 22 standard register addresses (0-6).
pub mod regs {
    /// Basic Mode Control Register.
    pub const BMCR: u8 = 0;
    /// Basic Mode Status Register.
    pub const BMSR: u8 = 1;
    /// PHY Identifier Register 1 (OUI bits 3-18).
    pub const PHYIDR1: u8 = 2;
    /// PHY Identifier Register 2 (OUI bits 19-24, model, revision).
    pub const PHYIDR2: u8 = 3;
    /// Auto-Negotiation Advertisement Register.
    pub const ANAR: u8 = 4;
    /// Auto-Negotiation Link Partner Ability Register.
    pub const ANLPAR: u8 = 5;
    /// Auto-Negotiation Expansion Register.
    pub const ANER: u8 = 6;
}

/// BMCR (Basic Mode Control Register) bit definitions.
pub mod bmcr {
    /// Software reset. Self-clearing.
    pub const RESET: u16 = 1 << 15;
    /// Loopback mode.
    pub const LOOPBACK: u16 = 1 << 14;
    /// Speed select: 1 = 100 Mbps, 0 = 10 Mbps.
    pub const SPEED_100: u16 = 1 << 13;
    /// Auto-negotiation enable.
    pub const AN_ENABLE: u16 = 1 << 12;
    /// Power down.
    pub const POWER_DOWN: u16 = 1 << 11;
    /// Electrically isolate PHY from MII/RMII.
    pub const ISOLATE: u16 = 1 << 10;
    /// Restart auto-negotiation. Self-clearing.
    pub const AN_RESTART: u16 = 1 << 9;
    /// Duplex mode: 1 = full, 0 = half.
    pub const DUPLEX_FULL: u16 = 1 << 8;
}

/// BMSR (Basic Mode Status Register) bit definitions.
pub mod bmsr {
    /// 100BASE-T4 capable.
    pub const T4_CAPABLE: u16 = 1 << 15;
    /// 100BASE-TX full duplex capable.
    pub const TX_FD_CAPABLE: u16 = 1 << 14;
    /// 100BASE-TX half duplex capable.
    pub const TX_HD_CAPABLE: u16 = 1 << 13;
    /// 10BASE-T full duplex capable.
    pub const T10_FD_CAPABLE: u16 = 1 << 12;
    /// 10BASE-T half duplex capable.
    pub const T10_HD_CAPABLE: u16 = 1 << 11;
    /// Auto-negotiation complete.
    pub const AN_COMPLETE: u16 = 1 << 5;
    /// Remote fault detected.
    pub const REMOTE_FAULT: u16 = 1 << 4;
    /// Auto-negotiation ability.
    pub const AN_ABILITY: u16 = 1 << 3;
    /// Link status (1 = link up).
    pub const LINK_STATUS: u16 = 1 << 2;
    /// Jabber detect (10BASE-T only).
    pub const JABBER_DETECT: u16 = 1 << 1;
    /// Extended register capability.
    pub const EXT_CAPABLE: u16 = 1 << 0;
}

/// ANAR (Auto-Negotiation Advertisement Register) bit definitions.
pub mod anar {
    /// Advertise 100BASE-TX full duplex.
    pub const TX_FD: u16 = 1 << 8;
    /// Advertise 100BASE-TX half duplex.
    pub const TX_HD: u16 = 1 << 7;
    /// Advertise 10BASE-T full duplex.
    pub const T10_FD: u16 = 1 << 6;
    /// Advertise 10BASE-T half duplex.
    pub const T10_HD: u16 = 1 << 5;
    /// IEEE 802.3 selector field.
    pub const SELECTOR_IEEE802_3: u16 = 0x0001;
}

/// ANLPAR (Auto-Negotiation Link Partner Ability Register) bit definitions.
pub mod anlpar {
    /// Link partner can 100BASE-TX full duplex.
    pub const CAN_100_FD: u16 = 1 << 8;
    /// Link partner can 100BASE-TX half duplex.
    pub const CAN_100_HD: u16 = 1 << 7;
    /// Link partner can 10BASE-T full duplex.
    pub const CAN_10_FD: u16 = 1 << 6;
    /// Link partner can 10BASE-T half duplex.
    pub const CAN_10_HD: u16 = 1 << 5;
    /// Link partner supports PAUSE flow control.
    pub const PAUSE: u16 = 1 << 10;
}

/// Perform a software reset on the PHY.
///
/// Writes BMCR with `RESET` (`0x8000`) and polls up to `max_attempts`
/// times until the bit self-clears. The write is intentionally raw —
/// per IEEE 802.3 Clause 22.1.2.5 a soft reset reloads the PHY's
/// internal default register state, so other BMCR bits (`SPEED_100`,
/// `DUPLEX_FULL`, `AN_ENABLE`, `ISOLATE`, ...) do not need to be
/// preserved across the call. Whatever the caller programmed before
/// this function runs is gone afterwards.
///
/// Returns `Ok(true)` if reset completed (bit cleared),
/// `Ok(false)` if `max_attempts` exhausted (bit still set).
/// The caller decides whether to treat a timeout as an error
/// (e.g., return `PhyError::ResetTimeout`).
pub fn soft_reset<M: MdioBus>(
    mdio: &mut M,
    phy_addr: u8,
    max_attempts: u32,
) -> Result<bool, M::Error> {
    mdio.write(phy_addr, regs::BMCR, bmcr::RESET)?;
    for _ in 0..max_attempts {
        let val = mdio.read(phy_addr, regs::BMCR)?;
        if val & bmcr::RESET == 0 {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Check whether the PHY link is up.
///
/// Reads the BMSR register and returns `true` if the LINK_STATUS bit is set.
pub fn is_link_up<M: MdioBus>(mdio: &mut M, phy_addr: u8) -> Result<bool, M::Error> {
    let val = mdio.read(phy_addr, regs::BMSR)?;
    Ok(val & bmsr::LINK_STATUS != 0)
}

/// Enable auto-negotiation and restart it.
///
/// Reads BMCR, sets AN_ENABLE and AN_RESTART, clears ISOLATE, then writes
/// the updated value back.
pub fn enable_auto_negotiation<M: MdioBus>(mdio: &mut M, phy_addr: u8) -> Result<(), M::Error> {
    let mut val = mdio.read(phy_addr, regs::BMCR)?;
    val |= bmcr::AN_ENABLE | bmcr::AN_RESTART;
    val &= !bmcr::ISOLATE;
    mdio.write(phy_addr, regs::BMCR, val)
}

/// Read the 32-bit PHY identifier.
///
/// Reads PHYIDR1 and PHYIDR2 and returns `(id1 << 16) | id2`.
pub fn read_phy_id<M: MdioBus>(mdio: &mut M, phy_addr: u8) -> Result<u32, M::Error> {
    let id1 = mdio.read(phy_addr, regs::PHYIDR1)? as u32;
    let id2 = mdio.read(phy_addr, regs::PHYIDR2)? as u32;
    Ok((id1 << 16) | id2)
}

/// Read PHY hardware capabilities from BMSR.
///
/// Parses the capability bits in the Basic Mode Status Register and
/// returns a [`PhyCapabilities`] struct.
///
/// # `pause` always returns `false`
///
/// The IEEE 802.3 Clause 22.2.4.2 BMSR layout has no PAUSE-capability
/// bit. PAUSE flow-control support is advertised through the
/// Auto-Negotiation Advertisement Register (`ANAR`, addr 0x04) — see
/// [`anar`]. Callers that need an authoritative PAUSE capability
/// must read `ANAR` directly (or wait for the link partner ability
/// in `ANLPAR`), and merge it with the BMSR-derived speed/duplex
/// fields manually. Returning `pause = false` here is therefore a
/// conservative default rather than a measurement.
pub fn read_capabilities<M: MdioBus>(
    mdio: &mut M,
    phy_addr: u8,
) -> Result<PhyCapabilities, M::Error> {
    let val = mdio.read(phy_addr, regs::BMSR)?;
    Ok(PhyCapabilities {
        speed_100_fd: val & bmsr::TX_FD_CAPABLE != 0,
        speed_100_hd: val & bmsr::TX_HD_CAPABLE != 0,
        speed_10_fd: val & bmsr::T10_FD_CAPABLE != 0,
        speed_10_hd: val & bmsr::T10_HD_CAPABLE != 0,
        auto_negotiation: val & bmsr::AN_ABILITY != 0,
        pause: false, // see rustdoc — PAUSE lives in ANAR/ANLPAR
    })
}

/// Force link speed and duplex (disable auto-negotiation).
///
/// Reads BMCR, clears `AN_ENABLE`, `AN_RESTART`, and `ISOLATE`, sets
/// `SPEED_100` / `DUPLEX_FULL` according to the requested [`LinkStatus`],
/// then writes back.
///
/// Why `AN_RESTART` is cleared explicitly: per IEEE 802.3 Clause
/// 22.2.4.1.5, `AN_RESTART` is self-clearing on hardware that
/// initiates the negotiation, but a chip with `AN_ENABLE = 0` is not
/// running negotiation and so never observes the bit; if the caller
/// has previously invoked [`enable_auto_negotiation`] (which sets
/// the bit) and then switches to forced-link, a subsequent
/// `enable_auto_negotiation` would read back `AN_RESTART = 1` from
/// BMCR and re-write it as part of the RMW, accidentally short-cutting
/// its restart cycle. Clearing here keeps the behaviour deterministic
/// across the two helpers regardless of call order.
pub fn force_link<M: MdioBus>(
    mdio: &mut M,
    phy_addr: u8,
    status: LinkStatus,
) -> Result<(), M::Error> {
    let mut val = mdio.read(phy_addr, regs::BMCR)?;
    val &=
        !(bmcr::AN_ENABLE | bmcr::AN_RESTART | bmcr::ISOLATE | bmcr::SPEED_100 | bmcr::DUPLEX_FULL);
    match status.speed {
        Speed::Mbps100 => val |= bmcr::SPEED_100,
        Speed::Mbps10 => {}
    }
    match status.duplex {
        Duplex::Full => val |= bmcr::DUPLEX_FULL,
        Duplex::Half => {}
    }
    mdio.write(phy_addr, regs::BMCR, val)
}

#[cfg(test)]
mod tests {
    extern crate alloc;

    use super::*;
    use alloc::vec;
    use alloc::vec::Vec;

    // ── Mock MDIO bus ──────────────────────────────────────────────────

    #[derive(Debug, PartialEq)]
    struct MockError;

    struct MockMdio {
        reads: Vec<u16>,
        read_idx: usize,
        writes: Vec<(u8, u8, u16)>,
        fail_at: Option<usize>,
        call_count: usize,
    }

    impl MockMdio {
        fn new(reads: Vec<u16>) -> Self {
            Self {
                reads,
                read_idx: 0,
                writes: Vec::new(),
                fail_at: None,
                call_count: 0,
            }
        }

        fn with_failure(reads: Vec<u16>, fail_at: usize) -> Self {
            Self {
                reads,
                read_idx: 0,
                writes: Vec::new(),
                fail_at: Some(fail_at),
                call_count: 0,
            }
        }
    }

    impl MdioBus for MockMdio {
        type Error = MockError;

        fn read(&mut self, _phy_addr: u8, _reg_addr: u8) -> Result<u16, Self::Error> {
            if self.fail_at == Some(self.call_count) {
                self.call_count += 1;
                return Err(MockError);
            }
            self.call_count += 1;
            let val = self.reads[self.read_idx];
            self.read_idx += 1;
            Ok(val)
        }

        fn write(&mut self, phy_addr: u8, reg_addr: u8, value: u16) -> Result<(), Self::Error> {
            if self.fail_at == Some(self.call_count) {
                self.call_count += 1;
                return Err(MockError);
            }
            self.call_count += 1;
            self.writes.push((phy_addr, reg_addr, value));
            Ok(())
        }
    }

    // ── Register constant tests ────────────────────────────────────────

    #[test]
    fn bmcr_bit_positions() {
        assert_eq!(bmcr::RESET, 0x8000);
        assert_eq!(bmcr::LOOPBACK, 0x4000);
        assert_eq!(bmcr::SPEED_100, 0x2000);
        assert_eq!(bmcr::AN_ENABLE, 0x1000);
        assert_eq!(bmcr::POWER_DOWN, 0x0800);
        assert_eq!(bmcr::ISOLATE, 0x0400);
        assert_eq!(bmcr::AN_RESTART, 0x0200);
        assert_eq!(bmcr::DUPLEX_FULL, 0x0100);
    }

    #[test]
    fn bmcr_bits_are_distinct() {
        let all = bmcr::RESET
            | bmcr::LOOPBACK
            | bmcr::SPEED_100
            | bmcr::AN_ENABLE
            | bmcr::POWER_DOWN
            | bmcr::ISOLATE
            | bmcr::AN_RESTART
            | bmcr::DUPLEX_FULL;
        assert_eq!(all.count_ones(), 8);
    }

    #[test]
    fn bmsr_bit_positions() {
        assert_eq!(bmsr::T4_CAPABLE, 0x8000);
        assert_eq!(bmsr::TX_FD_CAPABLE, 0x4000);
        assert_eq!(bmsr::TX_HD_CAPABLE, 0x2000);
        assert_eq!(bmsr::T10_FD_CAPABLE, 0x1000);
        assert_eq!(bmsr::T10_HD_CAPABLE, 0x0800);
        assert_eq!(bmsr::AN_COMPLETE, 0x0020);
        assert_eq!(bmsr::REMOTE_FAULT, 0x0010);
        assert_eq!(bmsr::AN_ABILITY, 0x0008);
        assert_eq!(bmsr::LINK_STATUS, 0x0004);
        assert_eq!(bmsr::JABBER_DETECT, 0x0002);
        assert_eq!(bmsr::EXT_CAPABLE, 0x0001);
    }

    #[test]
    fn reg_addresses() {
        assert_eq!(regs::BMCR, 0);
        assert_eq!(regs::BMSR, 1);
        assert_eq!(regs::PHYIDR1, 2);
        assert_eq!(regs::PHYIDR2, 3);
        assert_eq!(regs::ANAR, 4);
        assert_eq!(regs::ANLPAR, 5);
        assert_eq!(regs::ANER, 6);
    }

    // ── soft_reset tests ───────────────────────────────────────────────

    #[test]
    fn soft_reset_clears_immediately() {
        // First read after write returns 0 (RESET already cleared)
        let mut mdio = MockMdio::new(vec![0x0000]);
        let result = soft_reset(&mut mdio, 1, 10);
        assert!(result.unwrap());
        // One write (RESET) + one read (poll)
        assert_eq!(mdio.writes.len(), 1);
        assert_eq!(mdio.writes[0], (1, regs::BMCR, bmcr::RESET));
        assert_eq!(mdio.read_idx, 1);
    }

    #[test]
    fn soft_reset_polls_until_cleared() {
        // RESET bit set for 3 reads, then cleared on 4th
        let mut mdio = MockMdio::new(vec![bmcr::RESET, bmcr::RESET, bmcr::RESET, 0x0000]);
        let result = soft_reset(&mut mdio, 1, 10);
        assert!(result.unwrap());
        assert_eq!(mdio.read_idx, 4);
    }

    #[test]
    fn soft_reset_exhausts_attempts() {
        // RESET bit never clears — 3 attempts, returns false (timeout)
        let mut mdio = MockMdio::new(vec![bmcr::RESET, bmcr::RESET, bmcr::RESET]);
        let result = soft_reset(&mut mdio, 1, 3);
        assert!(!result.unwrap()); // timeout indicated, not error
        assert_eq!(mdio.read_idx, 3);
    }

    #[test]
    fn soft_reset_write_error() {
        // Fail on the very first call (the write)
        let mut mdio = MockMdio::with_failure(vec![], 0);
        let result = soft_reset(&mut mdio, 1, 10);
        assert!(result.is_err());
    }

    #[test]
    fn soft_reset_read_error_during_poll() {
        // Write succeeds, first poll read fails
        let mut mdio = MockMdio::with_failure(vec![], 1);
        assert!(soft_reset(&mut mdio, 1, 10).is_err());
    }

    // ── is_link_up tests ───────────────────────────────────────────────

    #[test]
    fn is_link_up_true() {
        let mut mdio = MockMdio::new(vec![bmsr::LINK_STATUS]);
        assert!(is_link_up(&mut mdio, 1).unwrap());
    }

    #[test]
    fn is_link_up_false() {
        let mut mdio = MockMdio::new(vec![0x0000]);
        assert!(!is_link_up(&mut mdio, 1).unwrap());
    }

    #[test]
    fn is_link_up_error() {
        let mut mdio = MockMdio::with_failure(vec![], 0);
        assert!(is_link_up(&mut mdio, 1).is_err());
    }

    // ── enable_auto_negotiation tests ──────────────────────────────────

    #[test]
    fn enable_auto_negotiation_sets_bits() {
        // BMCR starts at 0
        let mut mdio = MockMdio::new(vec![0x0000]);
        enable_auto_negotiation(&mut mdio, 1).unwrap();
        assert_eq!(mdio.writes.len(), 1);
        let written = mdio.writes[0].2;
        assert_ne!(written & bmcr::AN_ENABLE, 0);
        assert_ne!(written & bmcr::AN_RESTART, 0);
    }

    #[test]
    fn enable_auto_negotiation_clears_isolate() {
        // BMCR starts with ISOLATE set
        let mut mdio = MockMdio::new(vec![bmcr::ISOLATE]);
        enable_auto_negotiation(&mut mdio, 1).unwrap();
        let written = mdio.writes[0].2;
        assert_eq!(written & bmcr::ISOLATE, 0, "ISOLATE should be cleared");
        assert_ne!(written & bmcr::AN_ENABLE, 0);
    }

    // ── read_phy_id tests ──────────────────────────────────────────────

    #[test]
    fn read_phy_id_combines_registers() {
        // LAN8720A: PHYIDR1=0x0007, PHYIDR2=0xC0F1
        let mut mdio = MockMdio::new(vec![0x0007, 0xC0F1]);
        let id = read_phy_id(&mut mdio, 1).unwrap();
        assert_eq!(id, 0x0007_C0F1);
    }

    #[test]
    fn read_phy_id_error_on_first_read() {
        let mut mdio = MockMdio::with_failure(vec![], 0);
        assert!(read_phy_id(&mut mdio, 1).is_err());
    }

    #[test]
    fn read_phy_id_error_on_second_read() {
        let mut mdio = MockMdio::with_failure(vec![0x0007], 1);
        assert!(read_phy_id(&mut mdio, 1).is_err());
    }

    // ── read_capabilities tests ────────────────────────────────────────

    #[test]
    fn read_capabilities_full() {
        // All speed bits + AN ability set
        let bmsr_val = bmsr::TX_FD_CAPABLE
            | bmsr::TX_HD_CAPABLE
            | bmsr::T10_FD_CAPABLE
            | bmsr::T10_HD_CAPABLE
            | bmsr::AN_ABILITY;
        let mut mdio = MockMdio::new(vec![bmsr_val]);
        let caps = read_capabilities(&mut mdio, 1).unwrap();
        assert!(caps.speed_100_fd);
        assert!(caps.speed_100_hd);
        assert!(caps.speed_10_fd);
        assert!(caps.speed_10_hd);
        assert!(caps.auto_negotiation);
    }

    #[test]
    fn read_capabilities_10_only() {
        let bmsr_val = bmsr::T10_FD_CAPABLE | bmsr::T10_HD_CAPABLE;
        let mut mdio = MockMdio::new(vec![bmsr_val]);
        let caps = read_capabilities(&mut mdio, 1).unwrap();
        assert!(!caps.speed_100_fd);
        assert!(!caps.speed_100_hd);
        assert!(caps.speed_10_fd);
        assert!(caps.speed_10_hd);
        assert!(!caps.auto_negotiation);
    }

    // ── force_link tests ───────────────────────────────────────────────

    #[test]
    fn force_link_100_full() {
        let mut mdio = MockMdio::new(vec![0x0000]);
        let status = LinkStatus::new(Speed::Mbps100, Duplex::Full);
        force_link(&mut mdio, 1, status).unwrap();
        let written = mdio.writes[0].2;
        assert_ne!(written & bmcr::SPEED_100, 0);
        assert_ne!(written & bmcr::DUPLEX_FULL, 0);
        assert_eq!(written & bmcr::AN_ENABLE, 0, "AN must be disabled");
    }

    #[test]
    fn force_link_10_half() {
        let mut mdio = MockMdio::new(vec![0x0000]);
        let status = LinkStatus::new(Speed::Mbps10, Duplex::Half);
        force_link(&mut mdio, 1, status).unwrap();
        let written = mdio.writes[0].2;
        assert_eq!(written & bmcr::SPEED_100, 0);
        assert_eq!(written & bmcr::DUPLEX_FULL, 0);
        assert_eq!(written & bmcr::AN_ENABLE, 0);
    }

    #[test]
    fn force_link_preserves_other_bits() {
        // Start with LOOPBACK set; it should survive force_link
        let mut mdio = MockMdio::new(vec![bmcr::LOOPBACK]);
        let status = LinkStatus::new(Speed::Mbps100, Duplex::Full);
        force_link(&mut mdio, 1, status).unwrap();
        let written = mdio.writes[0].2;
        assert_ne!(written & bmcr::LOOPBACK, 0, "LOOPBACK should be preserved");
        assert_ne!(written & bmcr::SPEED_100, 0);
        assert_ne!(written & bmcr::DUPLEX_FULL, 0);
    }

    #[test]
    fn force_link_clears_an_restart() {
        // BMCR comes back with AN_ENABLE | AN_RESTART set — typical after
        // a previous enable_auto_negotiation. force_link must drop both
        // so a subsequent enable_auto_negotiation RMW doesn't re-write
        // AN_RESTART = 1 and accidentally short-cut its restart cycle.
        let mut mdio = MockMdio::new(vec![bmcr::AN_ENABLE | bmcr::AN_RESTART]);
        let status = LinkStatus::new(Speed::Mbps100, Duplex::Full);
        force_link(&mut mdio, 1, status).unwrap();
        let written = mdio.writes[0].2;
        assert_eq!(written & bmcr::AN_ENABLE, 0, "AN_ENABLE must be cleared");
        assert_eq!(written & bmcr::AN_RESTART, 0, "AN_RESTART must be cleared");
        assert_ne!(written & bmcr::SPEED_100, 0);
        assert_ne!(written & bmcr::DUPLEX_FULL, 0);
    }
}
