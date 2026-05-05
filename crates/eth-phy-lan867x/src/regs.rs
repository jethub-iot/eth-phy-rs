// SPDX-License-Identifier: GPL-2.0-or-later OR Apache-2.0
// Copyright (c) Viacheslav Bocharov <v@baodeep.com> and JetHome (r)

//! LAN867x register addresses and bit constants.
//!
//! Cross-references are to Microchip DS60001573C (silicon revision 2,
//! product revision B1). All values are taken directly from the
//! datasheet — no inferred or guessed encodings.
//!
//! The register map is split across four MMD device addresses:
//!
//! - [`MMD_PMA_PMD`] = 1 — PMA / PMD layer
//! - [`MMD_PCS`]     = 3 — PCS layer
//! - [`MMD_VS2`]     = 31 — Vendor Specific 2 (interrupts, PLCA, misc)
//!
//! plus the standard Clause 22 register window at MDIO addresses
//! 0x00..=0x1F, accessed directly without indirection.

#![allow(dead_code)]

// ── Identification ─────────────────────────────────────────────────────

/// `(PHY_ID0 << 16) | PHY_ID1` masked by [`PHY_OUI_MODEL_MASK`] for
/// any LAN867x family member, all silicon revisions.
///
/// PHY_ID0 = 0x0000 (Microchip OUI 00800Fh maps to all-zero in that reg)
/// PHY_ID1[15:4] = 0xC56 (OUI[18:23]=110001b, MODEL[5:0]=010110b)
pub const PHY_OUI_MODEL_LAN867X: u32 = 0x0000_C560;

/// Mask isolating OUI + MODEL, dropping the 4-bit silicon revision.
pub const PHY_OUI_MODEL_MASK: u32 = 0xFFFF_FFF0;

/// Mask isolating just the silicon-revision nibble.
pub const SILICON_REV_MASK: u32 = 0x0000_000F;

/// Silicon revision 2 = product revision B1 — current production silicon
/// covered by datasheet revisions B and C.
pub const SILICON_REV_B1: u32 = 0x0000_0002;

// ── MMD device addresses (DS60001573C sec 5.1.5 MMDCTRL.DEVAD) ─────────

/// MMD device 1 — PMA / PMD layer.
pub const MMD_PMA_PMD: u8 = 0x01;

/// MMD device 3 — PCS layer.
pub const MMD_PCS: u8 = 0x03;

/// MMD device 31 (`0x1F`) — Vendor Specific 2 — chip-specific status,
/// interrupt control, counters, PLCA configuration.
pub const MMD_VS2: u8 = 0x1F;

// ── MMDCTRL.FNCTN values (sec 5.1.5) ───────────────────────────────────

/// `FNCTN = 00b` — write the address of the MMD register to access.
pub const MMDCTRL_FNCTN_ADDR: u16 = 0b00 << 14;

/// `FNCTN = 01b` — read/write the MMD register data, no auto-increment.
pub const MMDCTRL_FNCTN_DATA: u16 = 0b01 << 14;

/// Mask for the `DEVAD[4:0]` field within `MMDCTRL`.
pub const MMDCTRL_DEVAD_MASK: u16 = 0x001F;

// ── Clause 22 register addresses (sec 5.1) ─────────────────────────────

/// `BASIC_CONTROL` — IEEE 802.3 Clause 22 BMCR.
pub const REG_BMCR: u8 = 0x00;
/// `BASIC_STATUS` — IEEE 802.3 Clause 22 BMSR. **Note:** `LINK_STATUS`
/// (bit 2) is hard-wired 1 on this chip — useless for link detection.
pub const REG_BMSR: u8 = 0x01;
/// `PHY_ID0` — OUI bits [2:17].
pub const REG_PHY_ID0: u8 = 0x02;
/// `PHY_ID1` — OUI[18:23] || MODEL[5:0] || REV[3:0].
pub const REG_PHY_ID1: u8 = 0x03;
/// `MMDCTRL` — IEEE Annex 22D indirection control.
pub const REG_MMDCTRL: u8 = 0x0D;
/// `MMDAD` — IEEE Annex 22D address/data window.
pub const REG_MMDAD: u8 = 0x0E;
/// `STRAP_CTRL0` — readback of `MITYP`, `PKGTYP`, `SMIADR` straps.
pub const REG_STRAP_CTRL0: u8 = 0x12;

// ── BMCR bits (sec 5.1.1) ──────────────────────────────────────────────

/// `BMCR.SW_RESET` — self-clearing software reset.
pub const BMCR_SW_RESET: u16 = 1 << 15;
/// `BMCR.LOOPBACK` — near-end MII/RMII loopback.
pub const BMCR_LOOPBACK: u16 = 1 << 14;
/// `BMCR.PD` — power-down PMA (alias of `T1SPMACTL.LPE`).
pub const BMCR_PD: u16 = 1 << 11;
/// `BMCR.ISOLATE` — electrically isolate PHY from MII/RMII.
pub const BMCR_ISOLATE: u16 = 1 << 10;
/// `BMCR.COL_TEST` — collision test, only valid in loopback.
pub const BMCR_COL_TEST: u16 = 1 << 7;

// ── STRAP_CTRL0 fields (sec 5.1.7) ─────────────────────────────────────

/// `STRAP_CTRL0.MITYP[1:0]` mask (bits 8:7).
pub const STRAP_CTRL0_MITYP_MASK: u16 = 0b11 << 7;
/// `MITYP = 01b` — RMII with 50 MHz REFCLKIN.
pub const STRAP_CTRL0_MITYP_RMII: u16 = 0b01 << 7;
/// `MITYP = 10b` — MII with 25 MHz crystal.
pub const STRAP_CTRL0_MITYP_MII: u16 = 0b10 << 7;

/// `STRAP_CTRL0.PKGTYP[1:0]` mask (bits 6:5).
pub const STRAP_CTRL0_PKGTYP_MASK: u16 = 0b11 << 5;
/// `PKGTYP = 01b` — 32-pin LAN8670.
pub const STRAP_CTRL0_PKGTYP_LAN8670: u16 = 0b01 << 5;
/// `PKGTYP = 10b` — 24-pin LAN8671.
pub const STRAP_CTRL0_PKGTYP_LAN8671: u16 = 0b10 << 5;
/// `PKGTYP = 11b` — 36-pin LAN8672.
pub const STRAP_CTRL0_PKGTYP_LAN8672: u16 = 0b11 << 5;

/// `STRAP_CTRL0.SMIADR[4:0]` mask (bits 4:0) — mirror of PHYAD straps.
pub const STRAP_CTRL0_SMIADR_MASK: u16 = 0x001F;

// ── MMD device 1 — PMA/PMD (sec 5.2) ───────────────────────────────────

/// `T1SPMACTL` (MMD 1, addr 0x08F9) — 10BASE-T1S PMA control.
pub const MMD_REG_T1SPMACTL: u16 = 0x08F9;
/// `T1SPMASTS` (MMD 1, addr 0x08FA) — 10BASE-T1S PMA status.
pub const MMD_REG_T1SPMASTS: u16 = 0x08FA;

/// `T1SPMACTL.RST` — PMA soft reset (self-clearing).
pub const T1SPMACTL_RST: u16 = 1 << 15;
/// `T1SPMACTL.TXD` — transmit disable.
pub const T1SPMACTL_TXD: u16 = 1 << 14;
/// `T1SPMACTL.LPE` — low-power enable (alias of `BMCR.PD`).
pub const T1SPMACTL_LPE: u16 = 1 << 11;
/// `T1SPMACTL.MDE` — multidrop enable. Must be set for any > 2-node bus.
pub const T1SPMACTL_MDE: u16 = 1 << 10;
/// `T1SPMACTL.LBE` — PMA loopback enable.
pub const T1SPMACTL_LBE: u16 = 1 << 0;

// ── MMD device 3 — PCS (sec 5.3) ───────────────────────────────────────

/// `T1SPCSCTL` (MMD 3, addr 0x08F3) — 10BASE-T1S PCS control.
pub const MMD_REG_T1SPCSCTL: u16 = 0x08F3;
/// `T1SPCSSTS` (MMD 3, addr 0x08F4) — 10BASE-T1S PCS status.
pub const MMD_REG_T1SPCSSTS: u16 = 0x08F4;

/// `T1SPCSSTS.FAULT` — PCS fault indication.
pub const T1SPCSSTS_FAULT: u16 = 1 << 7;

// ── MMD device 31 — Vendor Specific 2 (sec 5.4) ────────────────────────

/// `STS1` (MMD 31, addr 0x0018) — PLCA error/status interrupt sources.
pub const MMD_REG_STS1: u16 = 0x0018;
/// `STS2` (MMD 31, addr 0x0019) — reset-complete status.
pub const MMD_REG_STS2: u16 = 0x0019;
/// `STS3` (MMD 31, addr 0x001A) — `ERRTOID` snapshot at last STS1 IRQ.
pub const MMD_REG_STS3: u16 = 0x001A;
/// `IMSK1` (MMD 31, addr 0x001C) — interrupt mask for STS1 events.
pub const MMD_REG_IMSK1: u16 = 0x001C;
/// `IMSK2` (MMD 31, addr 0x001D) — interrupt mask for STS2 events.
pub const MMD_REG_IMSK2: u16 = 0x001D;
/// `MIDVER` (MMD 31, addr 0xCA00) — OPEN Alliance map identifier.
pub const MMD_REG_MIDVER: u16 = 0xCA00;
/// `PLCA_CTRL0` (MMD 31, addr 0xCA01) — PLCA `EN` + `RST`.
pub const MMD_REG_PLCA_CTRL0: u16 = 0xCA01;
/// `PLCA_CTRL1` (MMD 31, addr 0xCA02) — `NCNT` + local `ID`.
pub const MMD_REG_PLCA_CTRL1: u16 = 0xCA02;
/// `PLCA_STS` (MMD 31, addr 0xCA03) — `PST` (BEACONs flowing).
pub const MMD_REG_PLCA_STS: u16 = 0xCA03;
/// `PLCA_TOTMR` (MMD 31, addr 0xCA04) — transmit-opportunity timer (BT).
pub const MMD_REG_PLCA_TOTMR: u16 = 0xCA04;
/// `PLCA_BURST` (MMD 31, addr 0xCA05) — burst `MAXBC` + `BTMR`.
pub const MMD_REG_PLCA_BURST: u16 = 0xCA05;

/// `STS2.RESETC` — reset-complete sticky bit (RC). Reading `STS2`
/// clears `RESETC` and releases `IRQ_N`.
pub const STS2_RESETC: u16 = 1 << 11;

/// `MIDVER` reset value: `IDM = 0x0A` (OPEN Alliance map),
/// `VER = 0x10` (BCD = version 1.0).
pub const MIDVER_EXPECTED: u16 = 0x0A10;

/// `PLCA_CTRL0.EN` — PLCA reconciliation sublayer enable.
pub const PLCA_CTRL0_EN: u16 = 1 << 15;
/// `PLCA_CTRL0.RST` — PLCA sublayer reset (self-clearing).
pub const PLCA_CTRL0_RST: u16 = 1 << 14;

/// `PLCA_CTRL1.NCNT[7:0]` mask (bits 15:8).
pub const PLCA_CTRL1_NCNT_SHIFT: u16 = 8;
/// `PLCA_CTRL1.ID[7:0]` mask (bits 7:0).
pub const PLCA_CTRL1_ID_MASK: u16 = 0x00FF;

/// `PLCA_STS.PST` — BEACONs are being transmitted (coordinator) or
/// received (follower). The closest thing to a "link is up" signal.
pub const PLCA_STS_PST: u16 = 1 << 15;

/// Sentinel `PLCA_CTRL1.ID` value — disables PLCA even when
/// `PLCA_CTRL0.EN = 1`.
pub const PLCA_ID_DISABLED: u8 = 0xFF;

/// PLCA coordinator `ID` value.
pub const PLCA_ID_COORDINATOR: u8 = 0x00;
