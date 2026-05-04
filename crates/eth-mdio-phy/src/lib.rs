// SPDX-License-Identifier: GPL-2.0-or-later OR Apache-2.0
// Copyright (c) Viacheslav Bocharov <v@baodeep.com> and JetHome (r)

//! MDIO-based Ethernet PHY traits and IEEE 802.3 helpers.
//!
//! This crate defines the contract between MAC and PHY layers so a
//! single PHY driver can be reused across MAC implementations
//! (ESP32 EMAC, STM32 ETH, FPGA SMI, GPIO bit-bang, mocks).
//!
//! - [`MdioBus`] — wire-level read/write trait. Implemented by MAC
//!   layers.
//! - [`PhyDriver`] — what every PHY driver provides. Method receivers
//!   are generic over [`MdioBus`] so the same driver instance can talk
//!   to a real bus and a mock within one session.
//! - [`ieee802_3`] — Clause 22 register addresses, bit constants, and
//!   convenience helpers (`soft_reset`, `enable_auto_negotiation`,
//!   `is_link_up`, `read_phy_id`, `read_capabilities`, `force_link`).
//! - Shared types: [`Speed`], [`Duplex`], [`LinkStatus`],
//!   [`PhyCapabilities`], [`PhyError`].
//!
//! See the crate [README](https://github.com/jethub-iot/eth-phy-rs/tree/main/crates/eth-mdio-phy)
//! for installation, worked PHY-driver and MdioBus examples (including
//! a mock for tests), and a Clause 22 frame breakdown.
//!
//! # Quick start
//!
//! ```no_run
//! use eth_mdio_phy::{ieee802_3, LinkStatus, MdioBus, PhyDriver, PhyError};
//!
//! # fn drive<M: MdioBus, P: PhyDriver>(mdio: &mut M, phy: &mut P)
//! # -> Result<(), PhyError<M::Error>>
//! # {
//! phy.init(mdio)?;
//! while phy.poll_link(mdio)?.is_none() { /* idle */ }
//! # Ok(())
//! # }
//! ```
//!
//! # Crate features
//!
//! | Feature | Default | When to enable |
//! | --- | --- | --- |
//! | `defmt` | off | Adds `defmt::Format` derives on [`LinkStatus`], [`PhyCapabilities`], [`PhyError`]. |
//!
//! # Compatibility
//!
//! - **MSRV:** 1.75
//! - **Target:** any (`#![no_std]`, no transitive runtime deps)
//!
//! # Object-safety note
//!
//! [`PhyDriver`] is **not** object-safe (the methods are generic over
//! [`MdioBus`]), so `dyn PhyDriver` is a compile error. If you need
//! polymorphic storage (a switch driver, a multi-PHY watchdog) keep
//! the PHY drivers as concrete types behind an `enum`, or write a
//! thin object-safe wrapper that fixes a single [`MdioBus`]
//! implementation.

#![no_std]

pub mod ieee802_3;
mod mdio;
mod phy;
mod types;

pub use mdio::MdioBus;
pub use phy::{PhyDriver, PhyError};
pub use types::{Duplex, LinkStatus, PhyCapabilities, Speed};
