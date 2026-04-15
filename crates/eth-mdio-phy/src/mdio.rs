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
pub trait MdioBus {
    /// Error type for bus operations.
    type Error;

    /// Read a 16-bit PHY register.
    fn read(&mut self, phy_addr: u8, reg_addr: u8) -> Result<u16, Self::Error>;

    /// Write a 16-bit PHY register.
    fn write(&mut self, phy_addr: u8, reg_addr: u8, value: u16) -> Result<(), Self::Error>;
}
