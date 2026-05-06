// SPDX-License-Identifier: GPL-2.0-or-later OR Apache-2.0
// Copyright (c) Viacheslav Bocharov <v@baodeep.com> and JetHome (r)

//! PHY driver trait and common error type.

use crate::mdio::MdioBus;
use crate::types::{LinkStatus, PhyCapabilities};

/// Common error type for PHY driver operations.
#[derive(Debug)]
#[non_exhaustive]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum PhyError<E> {
    /// MDIO bus error (passthrough).
    Mdio(E),
    /// PHY soft reset did not complete within allowed attempts.
    ResetTimeout,
    /// PHY ID does not match expected chip family.
    UnsupportedChip {
        /// The actual PHY ID read from registers.
        id: u32,
    },
    /// PHY ID matched the expected family, but a chip-specific package
    /// or variant strap did not. Reported by drivers whose family
    /// covers more than one silicon SKU and which discriminate the
    /// concrete part at runtime (e.g. LAN867x via `STRAP_CTRL0.PKGTYP`).
    UnsupportedPackage {
        /// Raw strap-register value the driver could not decode,
        /// zero-extended to 32 bits for forward-compatibility with
        /// chips that expose wider strap windows.
        strap: u32,
    },
}

impl<E> From<E> for PhyError<E> {
    fn from(e: E) -> Self {
        PhyError::Mdio(e)
    }
}

/// Ethernet PHY driver — one implementation per chip family.
///
/// Every method except [`phy_addr`](Self::phy_addr) takes the bus
/// generically (`<M: MdioBus>`) so a single driver instance can talk
/// to different concrete buses across a session — useful for unit
/// tests against a mock bus and for diagnostic passthroughs.
///
/// **Object-safety.** Because of those generic methods the trait is
/// **not** object-safe — `dyn PhyDriver` is a compile error. If you
/// need polymorphic storage of multiple PHYs (a switch driver, a
/// link-state watchdog) keep them as concrete types behind an enum,
/// or write your own object-safe wrapper that fixes a single
/// `MdioBus` impl.
pub trait PhyDriver {
    /// PHY address on the MDIO bus (0-31).
    fn phy_addr(&self) -> u8;

    /// Initialize PHY: reset, configure, enable auto-negotiation.
    fn init<M: MdioBus>(&mut self, mdio: &mut M) -> Result<(), PhyError<M::Error>>;

    /// Poll link status. Returns `Some(LinkStatus)` when up, `None` when down.
    fn poll_link<M: MdioBus>(
        &mut self,
        mdio: &mut M,
    ) -> Result<Option<LinkStatus>, PhyError<M::Error>>;

    /// Query hardware capabilities.
    fn capabilities<M: MdioBus>(&self, mdio: &mut M)
        -> Result<PhyCapabilities, PhyError<M::Error>>;

    /// Read the 32-bit PHY identifier composed of the two ID
    /// registers at Clause 22 addresses 0x02 and 0x03:
    /// `(reg_0x02 << 16) | reg_0x03`.
    ///
    /// The bit-layout of the result is **chip-family-specific** —
    /// IEEE 802.3 Clause 22.2.4.3.1 defines OUI/model/revision packing
    /// for 100BASE-TX (e.g. LAN87xx via `PHYIDR1`/`PHYIDR2`), while
    /// 10BASE-T1S chips (LAN867x) use the same registers as a flat
    /// `PHY_ID0`/`PHY_ID1` pair with their own field widths. Drivers
    /// validate the result against a chip-specific mask; downstream
    /// consumers should treat it as opaque or consult the relevant
    /// datasheet.
    fn phy_id<M: MdioBus>(&self, mdio: &mut M) -> Result<u32, PhyError<M::Error>>;
}

#[cfg(test)]
mod tests {
    extern crate alloc;

    use super::*;
    use alloc::format;

    /// A trivial bus error type for testing.
    #[derive(Debug, PartialEq)]
    struct BusError(u8);

    #[test]
    fn phy_error_from_bus_error() {
        let bus_err = BusError(42);
        let phy_err: PhyError<BusError> = PhyError::from(bus_err);
        match phy_err {
            PhyError::Mdio(e) => assert_eq!(e, BusError(42)),
            _ => panic!("expected Mdio variant"),
        }
    }

    #[test]
    fn phy_error_reset_timeout() {
        let err: PhyError<BusError> = PhyError::ResetTimeout;
        match err {
            PhyError::ResetTimeout => {}
            _ => panic!("expected ResetTimeout variant"),
        }
    }

    #[test]
    fn phy_error_unsupported_chip() {
        let err: PhyError<BusError> = PhyError::UnsupportedChip { id: 0x0007_C0F1 };
        match err {
            PhyError::UnsupportedChip { id } => assert_eq!(id, 0x0007_C0F1),
            _ => panic!("expected UnsupportedChip variant"),
        }
    }

    #[test]
    fn phy_error_debug() {
        let err: PhyError<BusError> = PhyError::Mdio(BusError(1));
        let dbg = format!("{:?}", err);
        assert!(dbg.contains("Mdio"), "debug missing 'Mdio': {dbg}");
    }

    #[test]
    fn phy_error_into_from_bus() {
        fn fallible() -> Result<(), BusError> {
            Err(BusError(7))
        }

        let result: Result<(), PhyError<BusError>> = fallible().map_err(PhyError::from);
        match result {
            Err(PhyError::Mdio(e)) => assert_eq!(e, BusError(7)),
            _ => panic!("expected Mdio error"),
        }
    }
}
