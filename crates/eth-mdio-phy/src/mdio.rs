// SPDX-License-Identifier: GPL-2.0-or-later OR Apache-2.0
// Copyright (c) Viacheslav Bocharov <v@baodeep.com> and JetHome (r)

//! MDIO bus trait — the interface between MAC and PHY hardware.

/// MDIO bus for reading/writing PHY registers.
///
/// Every MAC that supports MDIO implements this trait. PHY drivers are
/// generic over `MdioBus` and never access hardware directly.
///
/// Register addresses follow IEEE 802.3 Clause 22: PHY address 0-31,
/// register address 0-31.
///
/// # `Self::Error`
///
/// No bound is forced on the associated `Error` type so this trait
/// stays usable on the smallest hosts (e.g. a host-side mock with a
/// zero-sized error). However, [`crate::PhyError<E>`] derives `Debug`
/// and `defmt::Format` (under the `defmt` feature) conditionally on
/// the wrapped `E`, so consumers that want to print or log a
/// `PhyError<M::Error>` should give their `M::Error` the corresponding
/// implementations. The recommended set for a real-world MAC is
/// `Debug + Clone` plus, where applicable, `defmt::Format`.
pub trait MdioBus {
    /// Error type for bus operations.
    type Error;

    /// Read a 16-bit PHY register.
    fn read(&mut self, phy_addr: u8, reg_addr: u8) -> Result<u16, Self::Error>;

    /// Write a 16-bit PHY register.
    fn write(&mut self, phy_addr: u8, reg_addr: u8, value: u16) -> Result<(), Self::Error>;
}
