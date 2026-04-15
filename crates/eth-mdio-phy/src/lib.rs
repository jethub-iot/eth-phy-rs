// SPDX-License-Identifier: GPL-2.0-or-later OR Apache-2.0
// Copyright (c) Viacheslav Bocharov <v@baodeep.com> and JetHome (r)

//! MDIO-based Ethernet PHY traits and IEEE 802.3 helpers.
//!
//! This crate defines the contract between MAC and PHY layers:
//! - [`MdioBus`] trait for MDIO register read/write
//! - [`PhyDriver`] trait for PHY initialization and link polling
//! - IEEE 802.3 Clause 22 standard register helpers
//! - Shared types: [`Speed`], [`Duplex`], [`LinkStatus`], [`PhyCapabilities`]
//!
//! Platform-independent. Works with any MAC that provides [`MdioBus`].

#![no_std]

pub mod ieee802_3;
mod mdio;
mod phy;
mod types;

pub use mdio::MdioBus;
pub use phy::{PhyDriver, PhyError};
pub use types::{Duplex, LinkStatus, PhyCapabilities, Speed};
