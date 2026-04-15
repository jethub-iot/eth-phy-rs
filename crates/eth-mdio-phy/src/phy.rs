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
}

impl<E> From<E> for PhyError<E> {
    fn from(e: E) -> Self {
        PhyError::Mdio(e)
    }
}

/// Ethernet PHY driver — one implementation per chip family.
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

    /// Read PHY identifier: `(PHYIDR1 << 16) | PHYIDR2`.
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
