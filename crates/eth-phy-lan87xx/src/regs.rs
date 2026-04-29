// SPDX-License-Identifier: GPL-2.0-or-later OR Apache-2.0
// Copyright (c) Viacheslav Bocharov <v@baodeep.com> and JetHome (r)

//! LAN87xx vendor-specific register definitions.
//!
//! Covers LAN8710A, LAN8720A, LAN8740A, LAN8741A, LAN8742A.
//! All documented vendor registers are defined here; some may not be
//! referenced by the current driver code but are provided for completeness.

/// PHY OUI (Organizationally Unique Identifier) for the LAN87xx family.
///
/// The 32-bit PHY ID read from PHYIDR1/PHYIDR2 encodes the OUI in the
/// upper 22 bits.  Mask with [`PHY_OUI_MASK`] before comparing.
pub const PHY_OUI: u32 = 0x0007_C000;

/// Mask to extract the OUI portion from a 32-bit PHY ID.
pub const PHY_OUI_MASK: u32 = 0xFFFF_FC00;

/// Mode Control/Status Register (vendor register 17).
pub mod mcsr {
    /// Register address.
    pub const ADDR: u8 = 17;
    /// Energy Detect Power-Down enable.
    pub const EDPD_EN: u16 = 1 << 14;
    /// Energy-on detected (read-only status bit).
    /// Used in tests to verify bit preservation during EDPD disable.
    #[cfg(test)]
    pub const ENERGYON: u16 = 1 << 1;
}

/// PHY Special Control/Status Register (vendor register 31).
pub mod pscsr {
    /// Register address.
    pub const ADDR: u8 = 31;
    /// Auto-negotiation done indicator. The speed / duplex indication
    /// field (bits [4:2]) is only valid once this bit is set; reading
    /// PSCSR before AUTODONE can return indeterminate values during
    /// the parallel-detection convergence window.
    pub const AUTODONE: u16 = 1 << 12;
    /// Mask for the speed/duplex indication field (bits [4:2]).
    pub const SPEED_DUPLEX_MASK: u16 = 0b111 << 2;
    /// 10 Mbps half duplex.
    pub const SPEED_10_HD: u16 = 0b001 << 2;
    /// 10 Mbps full duplex.
    pub const SPEED_10_FD: u16 = 0b101 << 2;
    /// 100 Mbps half duplex.
    pub const SPEED_100_HD: u16 = 0b010 << 2;
    /// 100 Mbps full duplex.
    pub const SPEED_100_FD: u16 = 0b110 << 2;
}

/// Interrupt Source Register (vendor register 29).
/// Not used by the current driver; provided for completeness.
#[allow(dead_code)]
pub mod isr {
    /// Register address.
    pub const ADDR: u8 = 29;
    /// Energy-on generated interrupt.
    pub const ENERGYON: u16 = 1 << 7;
    /// Auto-negotiation complete.
    pub const AN_COMPLETE: u16 = 1 << 6;
    /// Remote fault detected.
    pub const REMOTE_FAULT: u16 = 1 << 5;
    /// Link down (link status changed to down).
    pub const LINK_DOWN: u16 = 1 << 4;
    /// Auto-negotiation LP acknowledge.
    pub const AN_LP_ACK: u16 = 1 << 3;
    /// Parallel detection fault.
    pub const PD_FAULT: u16 = 1 << 2;
    /// Auto-negotiation page received.
    pub const AN_PAGE_RX: u16 = 1 << 1;
}

/// Interrupt Mask Register (vendor register 30).
///
/// Bit layout mirrors [`isr`]; writing a 1 enables the corresponding interrupt.
/// Not used by the current driver; provided for completeness.
#[allow(dead_code)]
pub mod imr {
    /// Register address.
    pub const ADDR: u8 = 30;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phy_oui_matches_lan8720a() {
        let id: u32 = 0x0007_C0F0;
        assert_eq!(id & PHY_OUI_MASK, PHY_OUI);
    }

    #[test]
    fn phy_oui_matches_lan8742a() {
        let id: u32 = 0x0007_C130;
        assert_eq!(id & PHY_OUI_MASK, PHY_OUI);
    }

    #[test]
    fn phy_oui_rejects_unknown() {
        let id: u32 = 0x0022_1619;
        assert_ne!(id & PHY_OUI_MASK, PHY_OUI);
    }

    #[test]
    fn pscsr_speed_duplex_values() {
        assert_eq!(pscsr::SPEED_10_HD, 0x04);
        assert_eq!(pscsr::SPEED_10_FD, 0x14);
        assert_eq!(pscsr::SPEED_100_HD, 0x08);
        assert_eq!(pscsr::SPEED_100_FD, 0x18);
    }

    #[test]
    fn pscsr_speed_duplex_mask() {
        // Mask isolates exactly the speed/duplex bits
        assert_eq!(
            pscsr::SPEED_10_HD & pscsr::SPEED_DUPLEX_MASK,
            pscsr::SPEED_10_HD
        );
        assert_eq!(
            pscsr::SPEED_10_FD & pscsr::SPEED_DUPLEX_MASK,
            pscsr::SPEED_10_FD
        );
        assert_eq!(
            pscsr::SPEED_100_HD & pscsr::SPEED_DUPLEX_MASK,
            pscsr::SPEED_100_HD
        );
        assert_eq!(
            pscsr::SPEED_100_FD & pscsr::SPEED_DUPLEX_MASK,
            pscsr::SPEED_100_FD
        );

        // Surrounding bits are not captured
        let val_with_noise: u16 = pscsr::SPEED_100_FD | 0xFF00 | 0x0003;
        assert_eq!(
            val_with_noise & pscsr::SPEED_DUPLEX_MASK,
            pscsr::SPEED_100_FD
        );
    }

    #[test]
    fn mcsr_edpd_bit() {
        assert_eq!(mcsr::EDPD_EN, 0x4000);
        assert_eq!(mcsr::EDPD_EN.count_ones(), 1);
    }
}
