// SPDX-License-Identifier: GPL-2.0-or-later OR Apache-2.0
// Copyright (c) Viacheslav Bocharov <v@baodeep.com> and JetHome (r)

//! PLCA (Physical Layer Collision Avoidance) configuration types.
//!
//! Implementation of `PhyLan867x::configure_plca` / `disable_plca` /
//! `plca_status` lives in `lib.rs` and lands in a follow-up commit.

/// PLCA configuration (IEEE 802.3 Clause 148).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct PlcaConfig {
    /// Local node ID. `0` = bus coordinator (must be exactly one on the
    /// segment), `1..=0xFE` = follower.
    ///
    /// `0xFF` is rejected by `configure_plca` — the silicon uses it as
    /// a sentinel that disables PLCA, so the driver refuses to write it
    /// via the configure path. Use `disable_plca` instead.
    pub node_id: u8,
    /// Total node count on the segment (`NCNT`). Must be ≥ active node
    /// count. Only the coordinator strictly cares about this; followers
    /// may set it to 0 and still operate, but supplying the right number
    /// makes diagnostics meaningful on every node.
    pub node_count: u8,
    /// Burst-mode max additional packets per transmit opportunity.
    /// `0` = burst disabled (one frame per TXOP — Clause 148 default).
    pub burst_count: u8,
    /// Burst timer in BT (100 ns units). `0` = leave chip default
    /// (`0x80` = 128 BT). Only consulted when `burst_count > 0`.
    pub burst_timer: u8,
}

impl Default for PlcaConfig {
    /// Coordinator with an 8-node segment, no burst.
    fn default() -> Self {
        Self {
            node_id: 0,
            node_count: 8,
            burst_count: 0,
            burst_timer: 0,
        }
    }
}

/// PLCA runtime state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct PlcaStatus {
    /// PLCA reconciliation sublayer enabled (`PLCA_CTRL0.EN`).
    pub enabled: bool,
    /// Local node ID (`PLCA_CTRL1.ID`).
    pub node_id: u8,
    /// `node_id == 0`. The coordinator transmits the periodic BEACONs.
    pub is_coordinator: bool,
    /// `PLCA_STS.PST` — BEACONs are being transmitted (coordinator) or
    /// received (follower) regularly. The closest thing to a "link is
    /// up" signal on a 10BASE-T1S multidrop bus.
    pub stable: bool,
}

/// Errors from PLCA-related driver methods.
#[derive(Debug)]
#[non_exhaustive]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum PlcaError<E> {
    /// MDIO bus error (passthrough).
    Mdio(E),
    /// Configuration values are invalid: e.g. `node_id == 0xFF` (silicon
    /// reserved sentinel for "disabled"), or a follower with `node_id`
    /// ≥ `node_count` such that no transmit opportunity is ever
    /// granted.
    InvalidConfig,
    /// `plca_status()` was called before `configure_plca()` (or after
    /// `disable_plca()`). The driver has no `node_id` to report.
    NotConfigured,
}
