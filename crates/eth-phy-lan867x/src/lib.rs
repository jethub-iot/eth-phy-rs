// SPDX-License-Identifier: GPL-2.0-or-later OR Apache-2.0
// Copyright (c) Viacheslav Bocharov <v@baodeep.com> and JetHome (r)

//! `#![no_std]` MDIO driver for the Microchip LAN867x family of
//! 10BASE-T1S Ethernet PHYs (IEEE 802.3cg-2019 Clause 147):
//!
//! - LAN8670 (32-VQFN, MII or RMII)
//! - LAN8671 (24-VQFN, RMII only) — JetHome hardware target
//! - LAN8672 (36-VQFN, MII only)
//!
//! Implements [`eth_mdio_phy::PhyDriver`], so any MAC that exposes
//! [`eth_mdio_phy::MdioBus`] can drive the chip.
//!
//! 10BASE-T1S is single-pair, half-duplex, multidrop Ethernet — quite
//! different from the point-to-point 10/100BASE-T flavours covered by
//! [`eth-phy-lan87xx`](https://docs.rs/eth-phy-lan87xx). Notably:
//!
//! - There is no auto-negotiation (`BMCR.AUTO_NEG_EN` is hard-wired 0).
//! - `BMSR.LINK_STATUS` is hard-wired 1 — useless for link detection.
//!   Use [`PhyLan867x::poll_link`], which reads `PLCA_STS.PST` when PLCA
//!   is enabled.
//! - Most operational state lives in MMD-31 (Vendor Specific 2),
//!   accessed via the IEEE Annex 22D MMDCTRL/MMDAD indirection.
//! - The chip ships in a multidrop-disabled state — driver `init()`
//!   sets `T1SPMACTL.MDE = 1`.
//!
//! Reference datasheet: Microchip DS60001573C (silicon revision 2,
//! product revision B1).

#![no_std]

mod mmd;
mod regs;

pub mod plca;

pub use plca::{PlcaConfig, PlcaError, PlcaStatus};

use eth_mdio_phy::{
    ieee802_3, Duplex, LinkStatus, MdioBus, PhyCapabilities, PhyDriver, PhyError, Speed,
};

/// Concrete LAN867x family member, identified at [`PhyLan867x::init`]
/// time from `STRAP_CTRL0.PKGTYP`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Chip {
    /// LAN8670 — 32-VQFN, supports both MII and RMII.
    Lan8670,
    /// LAN8671 — 24-VQFN, RMII only. JetHome hardware target.
    Lan8671,
    /// LAN8672 — 36-VQFN, MII only.
    Lan8672,
}

/// LAN867x PHY driver (software-only, no reset pin).
///
/// For a variant with a hardware reset pin, see [`PhyLan867xWithReset`].
pub struct PhyLan867x {
    addr: u8,
    chip: Option<Chip>,
    /// `Some(id)` when [`PhyLan867x::configure_plca`] has succeeded;
    /// `None` otherwise. `poll_link` uses this to decide whether to gate
    /// link status on `PLCA_STS.PST` or always report linked.
    pub(crate) plca_id: Option<u8>,
}

impl PhyLan867x {
    /// Create a new driver bound to the given MDIO/SMI address.
    ///
    /// Address discovery: `STRAP_CTRL0.SMIADR` reflects the strap-pin
    /// state; on JXD-CPU-E1T1S all PHYAD pins are pulled low ⇒ addr = 0.
    pub fn new(addr: u8) -> Self {
        Self {
            addr,
            chip: None,
            plca_id: None,
        }
    }

    /// Concrete chip discovered at `init()`. `None` until `init()` runs.
    pub fn chip(&self) -> Option<Chip> {
        self.chip
    }
}

impl PhyDriver for PhyLan867x {
    fn phy_addr(&self) -> u8 {
        self.addr
    }

    fn init<M: MdioBus>(&mut self, mdio: &mut M) -> Result<(), PhyError<M::Error>> {
        // 0. Drop cached driver state. `poll_link` uses `chip.is_some()`
        //    as the "init has succeeded" gate, so we must not leave a
        //    stale `Some(_)` from a previous successful init in case
        //    THIS init fails partway through. Soft reset (step 1) also
        //    wipes the chip's PLCA_CTRL0/CTRL1 to defaults, so the
        //    driver's PLCA cache must follow. Single-owner contract:
        //    this driver is the sole writer to the chip's registers, so
        //    we don't need to read PLCA state back from silicon.
        self.plca_id = None;
        self.chip = None;

        // 1. Software reset (BMCR.SW_RESET, self-clearing). Bounded poll —
        //    matches the lan87xx driver's allowance.
        let cleared = ieee802_3::soft_reset(mdio, self.addr, 500).map_err(PhyError::Mdio)?;
        if !cleared {
            return Err(PhyError::ResetTimeout);
        }

        // 2. Reset-complete handshake (DS60001573C sec 4.7).
        //
        //    After any reset (including the soft reset above), the chip
        //    holds IRQ_N low until the host reads STS2 in MMD-31. Until
        //    that read, register writes from this point onward are NOT
        //    guaranteed to take effect — the device is still completing
        //    its internal initialisation. Poll STS2.RESETC; the read is
        //    what clears the bit and releases IRQ_N.
        let mut got_resetc = false;
        for _ in 0..500 {
            let sts2 = mmd::mmd_read(mdio, self.addr, regs::MMD_VS2, regs::MMD_REG_STS2)
                .map_err(PhyError::Mdio)?;
            if sts2 & regs::STS2_RESETC != 0 {
                got_resetc = true;
                break;
            }
        }
        if !got_resetc {
            return Err(PhyError::ResetTimeout);
        }

        // 3. Verify family identity from PHY_ID0 / PHY_ID1.
        //
        //    Mask out the silicon-revision nibble — the driver supports
        //    every revision Microchip has shipped to date (Rev 0 and
        //    Rev 2). If a future revision changes register semantics,
        //    add silicon-rev branching here.
        let id = ieee802_3::read_phy_id(mdio, self.addr).map_err(PhyError::Mdio)?;
        if id & regs::PHY_OUI_MODEL_MASK != regs::PHY_OUI_MODEL_LAN867X {
            return Err(PhyError::UnsupportedChip { id });
        }

        // 4. Discriminate the concrete package from STRAP_CTRL0.PKGTYP.
        //    The strap is latched at hardware reset and survives soft
        //    reset (NASR), so reading it after step 1 is safe.
        //
        //    Hold the discovered chip in a local — `self.chip` is only
        //    written at the very end so that any later step's failure
        //    leaves the driver in the "uninitialised" state and the
        //    `poll_link` gate stays honest.
        let strap = mdio
            .read(self.addr, regs::REG_STRAP_CTRL0)
            .map_err(PhyError::Mdio)?;
        let chip = match strap & regs::STRAP_CTRL0_PKGTYP_MASK {
            regs::STRAP_CTRL0_PKGTYP_LAN8670 => Chip::Lan8670,
            regs::STRAP_CTRL0_PKGTYP_LAN8671 => Chip::Lan8671,
            regs::STRAP_CTRL0_PKGTYP_LAN8672 => Chip::Lan8672,
            _ => {
                // PHY ID matched LAN867x family but PKGTYP is not one
                // of the three documented packages. Surface the strap
                // value rather than the PHY ID so the caller doesn't
                // mistakenly conclude the chip is unsupported.
                return Err(PhyError::UnsupportedPackage {
                    strap: u32::from(strap),
                });
            }
        };

        // 5. Sanity-probe the OPEN Alliance map identifier in MMD-31 —
        //    confirms the indirection sequence is functional and that
        //    we're looking at an OPEN Alliance T1S PHY (and not, say,
        //    the wrong vendor of LAN867x clone).
        let midver = mmd::mmd_read(mdio, self.addr, regs::MMD_VS2, regs::MMD_REG_MIDVER)
            .map_err(PhyError::Mdio)?;
        if midver != regs::MIDVER_EXPECTED {
            return Err(PhyError::UnsupportedChip { id });
        }

        // 6. Multidrop enable — required for any > 2-node bus, which is
        //    the topology JetHome boards are designed for. Use RMW to
        //    preserve any other bits the silicon may have come up with.
        mmd::mmd_rmw(
            mdio,
            self.addr,
            regs::MMD_PMA_PMD,
            regs::MMD_REG_T1SPMACTL,
            0,
            regs::T1SPMACTL_MDE,
        )
        .map_err(PhyError::Mdio)?;

        // 7. Commit. Every fallible step has succeeded; `self.chip =
        //    Some(_)` from this point onward truthfully signals to
        //    `poll_link` that the chip is in the post-init multidrop-
        //    ready state.
        self.chip = Some(chip);

        Ok(())
    }

    fn poll_link<M: MdioBus>(
        &mut self,
        mdio: &mut M,
    ) -> Result<Option<LinkStatus>, PhyError<M::Error>> {
        // On a 10BASE-T1S multidrop bus there is no autonegotiation and
        // no per-link-partner signal. Three distinct cases:
        //
        // - Driver not yet `init`-ed: report None. The chip has neither
        //   completed its RESETC handshake nor had `T1SPMACTL.MDE` set,
        //   so we have no business claiming a working link. Used as the
        //   gate: `self.chip` is populated only by `init()` and never
        //   reset, so `chip.is_some()` ≡ "init has succeeded".
        //
        // - PLCA off (CSMA/CD), post-init: the bus is "always there".
        //   Report linked — the caller can attempt to send and the chip
        //   will handle (possibly colliding) transmissions.
        //
        // - PLCA on: PLCA_STS.PST tracks whether BEACONs are being TX'd
        //   (coordinator) or RX'd (follower). It is the only meaningful
        //   "are we participating in the network" indicator. Report
        //   linked when set; report None until the bus stabilises.
        //
        // PLCA mode is selected by `self.plca_id`, which `configure_plca`
        // sets and `disable_plca` / `init` clear. Single-owner contract:
        // this driver is assumed to be the sole writer to the chip's
        // registers, so the driver-side flag is authoritative. If
        // someone else flips `PLCA_CTRL0.EN` directly via MDIO between
        // our `configure_plca` and a `poll_link`, we will not notice.
        match (self.chip, self.plca_id) {
            (None, _) => Ok(None),
            (Some(_), None) => Ok(Some(LinkStatus::new(Speed::Mbps10, Duplex::Half))),
            (Some(_), Some(_)) => {
                let sts = mmd::mmd_read(mdio, self.addr, regs::MMD_VS2, regs::MMD_REG_PLCA_STS)
                    .map_err(PhyError::Mdio)?;
                if sts & regs::PLCA_STS_PST != 0 {
                    Ok(Some(LinkStatus::new(Speed::Mbps10, Duplex::Half)))
                } else {
                    Ok(None)
                }
            }
        }
    }

    fn capabilities<M: MdioBus>(
        &self,
        mdio: &mut M,
    ) -> Result<PhyCapabilities, PhyError<M::Error>> {
        // BASIC_STATUS reports only the 10BASE-T half-duplex bit on this
        // chip — `read_capabilities` decodes it correctly. The other
        // ability bits are hard-wired 0, which `PhyCapabilities`
        // already reflects via its boolean fields.
        ieee802_3::read_capabilities(mdio, self.addr).map_err(PhyError::Mdio)
    }

    fn phy_id<M: MdioBus>(&self, mdio: &mut M) -> Result<u32, PhyError<M::Error>> {
        ieee802_3::read_phy_id(mdio, self.addr).map_err(PhyError::Mdio)
    }
}

// ── PLCA configuration / introspection (chip-specific, not in PhyDriver) ─

impl PhyLan867x {
    /// Configure and enable PLCA on this node.
    ///
    /// Must be called after [`PhyLan867x::init`] — `init` only puts the
    /// chip in CSMA/CD multidrop mode, with PLCA off.
    ///
    /// Validation:
    ///
    /// - `node_id == 0xFF` is rejected (silicon sentinel for "disabled");
    ///   use [`PhyLan867x::disable_plca`] instead.
    /// - For followers (`node_id != 0`) with a non-zero `node_count`,
    ///   `node_id < node_count` must hold; otherwise this node would
    ///   never be granted a transmit opportunity.
    ///
    /// # Failure semantics — NOT transactional
    ///
    /// The wire protocol is three sequential MDIO writes — `PLCA_CTRL1`
    /// (NCNT/ID), `PLCA_BURST` (MAXBC/BTMR), `PLCA_CTRL0.EN` (RMW). An
    /// MDIO bus error after one of those writes succeeds leaves the
    /// chip in a partially configured state and the driver-side
    /// `plca_id` cache out of sync with the silicon:
    ///
    /// - Failure of step 1 (CTRL1) or step 2 (BURST): silicon's
    ///   previous CTRL0.EN is intact, so PLCA continues to run with
    ///   the previously-configured `node_id`. `plca_id` is unchanged.
    /// - Failure of step 3 (CTRL0.EN RMW): silicon's CTRL1 and BURST
    ///   already hold the new values, but EN may carry the previous
    ///   state. `plca_id` is unchanged. If EN was already 1 from a
    ///   prior successful configure, PLCA now runs with the **new**
    ///   CTRL1/BURST while the driver still believes it's running
    ///   with the **old** ones — `poll_link` will read `PLCA_STS.PST`
    ///   correctly but any `plca_status()` interpretation that
    ///   crosses the call boundary may surprise.
    ///
    /// Recovery: retry `configure_plca` with the same parameters, or
    /// call [`PhyLan867x::init`] to reset the chip and the driver
    /// state together.
    ///
    /// A future release may add transactional semantics (write-then-
    /// readback or rollback-on-error) once the broader 10BASE-T1S +
    /// PLCA architectural plan in `docs/plans/eth-phy-lan867x-plca.md`
    /// (in the parent repository) settles the runtime-toggling story.
    pub fn configure_plca<M: MdioBus>(
        &mut self,
        mdio: &mut M,
        config: &PlcaConfig,
    ) -> Result<(), PlcaError<M::Error>> {
        if config.node_id == regs::PLCA_ID_DISABLED {
            return Err(PlcaError::InvalidConfig);
        }
        if config.node_count != 0 && config.node_id >= config.node_count {
            return Err(PlcaError::InvalidConfig);
        }

        // PLCA_CTRL1 = NCNT[15:8] | ID[7:0]
        let ctrl1 = (u16::from(config.node_count) << regs::PLCA_CTRL1_NCNT_SHIFT)
            | u16::from(config.node_id);
        mmd::mmd_write(
            mdio,
            self.addr,
            regs::MMD_VS2,
            regs::MMD_REG_PLCA_CTRL1,
            ctrl1,
        )
        .map_err(PlcaError::Mdio)?;

        // Always program PLCA_BURST so a re-configuration with
        // `burst_count = 0` reliably clears MAXBC, undoing any prior
        // configure_plca that enabled bursting. Per datasheet sec
        // 5.4.18, MAXBC = 0 is the explicit "burst disabled" encoding.
        // burst_timer = 0 in PlcaConfig is a sentinel that means "leave
        // chip default of 0x80" — interpret it here so users don't
        // accidentally write BTMR = 0 (which would make burst mode
        // non-functional even when MAXBC > 0).
        let btmr = if config.burst_timer == 0 {
            0x80_u16
        } else {
            u16::from(config.burst_timer)
        };
        let burst = (u16::from(config.burst_count) << 8) | btmr;
        mmd::mmd_write(
            mdio,
            self.addr,
            regs::MMD_VS2,
            regs::MMD_REG_PLCA_BURST,
            burst,
        )
        .map_err(PlcaError::Mdio)?;

        // Flip the enable bit last, after CTRL1 is in place. RMW preserves
        // any other bits that future silicon revisions might define.
        mmd::mmd_rmw(
            mdio,
            self.addr,
            regs::MMD_VS2,
            regs::MMD_REG_PLCA_CTRL0,
            0,
            regs::PLCA_CTRL0_EN,
        )
        .map_err(PlcaError::Mdio)?;

        self.plca_id = Some(config.node_id);
        Ok(())
    }

    /// Disable PLCA — chip falls back to CSMA/CD on the segment.
    pub fn disable_plca<M: MdioBus>(&mut self, mdio: &mut M) -> Result<(), PlcaError<M::Error>> {
        mmd::mmd_rmw(
            mdio,
            self.addr,
            regs::MMD_VS2,
            regs::MMD_REG_PLCA_CTRL0,
            regs::PLCA_CTRL0_EN,
            0,
        )
        .map_err(PlcaError::Mdio)?;
        self.plca_id = None;
        Ok(())
    }

    /// Snapshot the chip's PLCA registers.
    ///
    /// Returns a chip-truth view: even if `configure_plca` was never
    /// called on this driver instance (e.g. a different host configured
    /// the chip earlier), the result reflects what the silicon currently
    /// reports.
    pub fn plca_status<M: MdioBus>(&self, mdio: &mut M) -> Result<PlcaStatus, PlcaError<M::Error>> {
        let ctrl0 = mmd::mmd_read(mdio, self.addr, regs::MMD_VS2, regs::MMD_REG_PLCA_CTRL0)
            .map_err(PlcaError::Mdio)?;
        let ctrl1 = mmd::mmd_read(mdio, self.addr, regs::MMD_VS2, regs::MMD_REG_PLCA_CTRL1)
            .map_err(PlcaError::Mdio)?;
        let sts = mmd::mmd_read(mdio, self.addr, regs::MMD_VS2, regs::MMD_REG_PLCA_STS)
            .map_err(PlcaError::Mdio)?;

        let id = (ctrl1 & regs::PLCA_CTRL1_ID_MASK) as u8;
        Ok(PlcaStatus {
            enabled: ctrl0 & regs::PLCA_CTRL0_EN != 0,
            node_id: id,
            is_coordinator: id == regs::PLCA_ID_COORDINATOR,
            stable: sts & regs::PLCA_STS_PST != 0,
        })
    }
}

/// LAN867x PHY driver with a hardware reset pin.
///
/// Wraps [`PhyLan867x`] and adds [`hardware_reset`](Self::hardware_reset)
/// for toggling the PHY `RESET_N` line before `init()`. On JetHome
/// JXD-CPU-E1T1S the reset pin is wired to ESP32 GPIO17.
pub struct PhyLan867xWithReset<P: embedded_hal::digital::OutputPin> {
    inner: PhyLan867x,
    reset_pin: P,
}

impl<P: embedded_hal::digital::OutputPin> PhyLan867xWithReset<P> {
    /// Create a new driver with the given MDIO address and reset pin.
    pub fn new(addr: u8, pin: P) -> Self {
        Self {
            inner: PhyLan867x::new(addr),
            reset_pin: pin,
        }
    }

    /// Drive `RESET_N` low for 10 ms, then wait 25 ms after release
    /// before MDIO is touched.
    ///
    /// The 10 ms low-pulse is conservative — datasheet sec 7.6.4 allows
    /// shorter — and is intentionally longer than the 2 ms used by
    /// `eth-phy-lan87xx`'s wrapper, since LAN867x boards (e.g. JXD-CPU-
    /// E1T1S) drive `RESET_N` from a slow GPIO that may have noticeable
    /// rise/fall times. The 25 ms post-release delay matches the
    /// lan87xx wrapper.
    pub fn hardware_reset<D: embedded_hal::delay::DelayNs>(
        &mut self,
        delay: &mut D,
    ) -> Result<(), P::Error> {
        self.reset_pin.set_low()?;
        delay.delay_ms(10);
        self.reset_pin.set_high()?;
        delay.delay_ms(25);
        Ok(())
    }

    /// Borrow the inner [`PhyLan867x`] for chip-specific operations
    /// (e.g. PLCA configuration) that aren't part of [`PhyDriver`].
    pub fn inner_mut(&mut self) -> &mut PhyLan867x {
        &mut self.inner
    }
}

impl<P: embedded_hal::digital::OutputPin> PhyDriver for PhyLan867xWithReset<P> {
    fn phy_addr(&self) -> u8 {
        self.inner.phy_addr()
    }

    fn init<M: MdioBus>(&mut self, mdio: &mut M) -> Result<(), PhyError<M::Error>> {
        self.inner.init(mdio)
    }

    fn poll_link<M: MdioBus>(
        &mut self,
        mdio: &mut M,
    ) -> Result<Option<LinkStatus>, PhyError<M::Error>> {
        self.inner.poll_link(mdio)
    }

    fn capabilities<M: MdioBus>(
        &self,
        mdio: &mut M,
    ) -> Result<PhyCapabilities, PhyError<M::Error>> {
        self.inner.capabilities(mdio)
    }

    fn phy_id<M: MdioBus>(&self, mdio: &mut M) -> Result<u32, PhyError<M::Error>> {
        self.inner.phy_id(mdio)
    }
}

#[cfg(test)]
mod tests {
    extern crate alloc;

    use super::*;
    use alloc::vec;
    use alloc::vec::Vec;
    use eth_mdio_phy::ieee802_3::bmcr;

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
            let v = *self
                .reads
                .get(self.read_idx)
                .expect("MockMdio: reads vector exhausted");
            self.read_idx += 1;
            Ok(v)
        }

        fn write(&mut self, phy: u8, reg: u8, value: u16) -> Result<(), Self::Error> {
            if self.fail_at == Some(self.call_count) {
                self.call_count += 1;
                return Err(MockError);
            }
            self.call_count += 1;
            self.writes.push((phy, reg, value));
            Ok(())
        }
    }

    // PHY_ID readout helpers — silicon Rev 2 (B1) for each package variant.
    const PHY_ID0_LAN867X: u16 = 0x0000;
    const PHY_ID1_LAN867X_REV2: u16 = 0xC562;

    /// Successful-init read sequence shared across happy-path tests.
    fn reads_for_successful_init(strap_pkgtyp: u16) -> Vec<u16> {
        vec![
            // (1) BMCR poll inside soft_reset — SW_RESET cleared.
            0x0000,
            // (2) MMDAD read → STS2 with RESETC asserted.
            regs::STS2_RESETC,
            // (3) PHY_ID0 → 0x0000
            PHY_ID0_LAN867X,
            // (4) PHY_ID1 → silicon-rev-2 LAN867x family.
            PHY_ID1_LAN867X_REV2,
            // (5) STRAP_CTRL0 → caller-supplied PKGTYP encoding.
            strap_pkgtyp,
            // (6) MMDAD read → MIDVER, must be 0x0A10.
            regs::MIDVER_EXPECTED,
            // (7) MMDAD read → T1SPMACTL pre-RMW: chip default 0.
            0x0000,
        ]
    }

    // ── Constructor tests ──────────────────────────────────────────────

    #[test]
    fn new_sets_address_and_no_chip() {
        let phy = PhyLan867x::new(7);
        assert_eq!(phy.phy_addr(), 7);
        assert_eq!(phy.chip(), None);
    }

    // ── init() tests ───────────────────────────────────────────────────

    #[test]
    fn init_success_lan8671_jethome_target() {
        let mut mdio = MockMdio::new(reads_for_successful_init(regs::STRAP_CTRL0_PKGTYP_LAN8671));
        let mut phy = PhyLan867x::new(0);
        phy.init(&mut mdio).unwrap();
        assert_eq!(phy.chip(), Some(Chip::Lan8671));
    }

    #[test]
    fn init_success_lan8670() {
        let mut mdio = MockMdio::new(reads_for_successful_init(regs::STRAP_CTRL0_PKGTYP_LAN8670));
        let mut phy = PhyLan867x::new(0);
        phy.init(&mut mdio).unwrap();
        assert_eq!(phy.chip(), Some(Chip::Lan8670));
    }

    #[test]
    fn init_success_lan8672() {
        let mut mdio = MockMdio::new(reads_for_successful_init(regs::STRAP_CTRL0_PKGTYP_LAN8672));
        let mut phy = PhyLan867x::new(0);
        phy.init(&mut mdio).unwrap();
        assert_eq!(phy.chip(), Some(Chip::Lan8672));
    }

    #[test]
    fn init_rejects_invalid_pkgtyp() {
        // PKGTYP = 00b is "Undefined" per datasheet sec 5.1.7.
        let mut mdio = MockMdio::new(vec![
            0x0000,
            regs::STS2_RESETC,
            PHY_ID0_LAN867X,
            PHY_ID1_LAN867X_REV2,
            0x0000, // STRAP_CTRL0 with PKGTYP = 00b
        ]);
        let mut phy = PhyLan867x::new(0);
        let err = phy.init(&mut mdio).unwrap_err();
        // Distinct from UnsupportedChip: the PHY ID matched correctly,
        // only the package strap is unrecognised.
        match err {
            PhyError::UnsupportedPackage { strap } => assert_eq!(strap, 0x0000),
            other => panic!("expected UnsupportedPackage, got {other:?}"),
        }
    }

    #[test]
    fn init_writes_t1spmactl_mde_bit() {
        let mut mdio = MockMdio::new(reads_for_successful_init(regs::STRAP_CTRL0_PKGTYP_LAN8671));
        let mut phy = PhyLan867x::new(0);
        phy.init(&mut mdio).unwrap();

        // Find the data-write to MMDAD at the end of the T1SPMACTL RMW.
        // The pre-RMW read returned 0x0000 → final write must be exactly
        // T1SPMACTL_MDE with no other bits set.
        let last_mmdad_data_write = mdio
            .writes
            .iter()
            .rev()
            .find(|&&(_, reg, _)| reg == regs::REG_MMDAD)
            .expect("expected an MMDAD data write");
        assert_eq!(
            last_mmdad_data_write.2,
            regs::T1SPMACTL_MDE,
            "init must set T1SPMACTL.MDE = 1 (multidrop enable)"
        );
    }

    #[test]
    fn init_t1spmactl_rmw_preserves_other_bits() {
        // Same flow but the chip's pre-RMW T1SPMACTL has TXD set.
        // After init, the data-write must keep TXD AND set MDE.
        let mut reads = reads_for_successful_init(regs::STRAP_CTRL0_PKGTYP_LAN8671);
        // Replace position 6 (the T1SPMACTL pre-RMW read) with TXD-set.
        reads[6] = regs::T1SPMACTL_TXD;
        let mut mdio = MockMdio::new(reads);
        let mut phy = PhyLan867x::new(0);
        phy.init(&mut mdio).unwrap();

        let last_mmdad_data_write = mdio
            .writes
            .iter()
            .rev()
            .find(|&&(_, reg, _)| reg == regs::REG_MMDAD)
            .unwrap();
        assert_eq!(
            last_mmdad_data_write.2,
            regs::T1SPMACTL_TXD | regs::T1SPMACTL_MDE,
            "RMW must preserve pre-existing T1SPMACTL bits"
        );
    }

    #[test]
    fn init_reset_timeout_when_bmcr_never_clears() {
        // 1000 reads of BMCR all returning RESET set → soft_reset returns
        // false → ResetTimeout. Buffer larger than the 500-attempt limit
        // to avoid coupling the test to the precise loop count.
        let mut mdio = MockMdio::new(vec![bmcr::RESET; 1000]);
        let mut phy = PhyLan867x::new(0);
        let err = phy.init(&mut mdio).unwrap_err();
        assert!(matches!(err, PhyError::ResetTimeout));
    }

    #[test]
    fn init_reset_timeout_when_resetc_never_asserts() {
        // BMCR clears immediately, but STS2.RESETC never goes high — the
        // chip never reports reset-complete. We must time out, not block.
        let mut reads = vec![0x0000_u16; 1001]; // [0]=BMCR cleared, rest=STS2 with RESETC never set
        reads[0] = 0x0000;
        let mut mdio = MockMdio::new(reads);
        let mut phy = PhyLan867x::new(0);
        let err = phy.init(&mut mdio).unwrap_err();
        assert!(matches!(err, PhyError::ResetTimeout));
    }

    #[test]
    fn init_rejects_wrong_phy_id() {
        let mut mdio = MockMdio::new(vec![
            0x0000,
            regs::STS2_RESETC,
            0x0007, // PHY_ID0 — not LAN867x
            0xC0F0, // PHY_ID1 — looks like LAN8720A
        ]);
        let mut phy = PhyLan867x::new(0);
        let err = phy.init(&mut mdio).unwrap_err();
        match err {
            PhyError::UnsupportedChip { id } => assert_eq!(id, 0x0007_C0F0),
            e => panic!("expected UnsupportedChip, got {e:?}"),
        }
    }

    #[test]
    fn init_rejects_wrong_midver() {
        // Right PHY_ID, right STRAP, but MMD-31 MIDVER returns garbage —
        // either the chip is mis-clocked or the MMD indirection broke.
        let mut mdio = MockMdio::new(vec![
            0x0000,
            regs::STS2_RESETC,
            PHY_ID0_LAN867X,
            PHY_ID1_LAN867X_REV2,
            regs::STRAP_CTRL0_PKGTYP_LAN8671,
            0xDEAD, // MIDVER — wrong
        ]);
        let mut phy = PhyLan867x::new(0);
        let err = phy.init(&mut mdio).unwrap_err();
        assert!(matches!(err, PhyError::UnsupportedChip { .. }));
    }

    #[test]
    fn init_mdio_error_propagates() {
        // Fail on call 0 (the BMCR write inside soft_reset).
        let mut mdio = MockMdio::with_failure(vec![], 0);
        let mut phy = PhyLan867x::new(0);
        let err = phy.init(&mut mdio).unwrap_err();
        assert!(matches!(err, PhyError::Mdio(MockError)));
    }

    #[test]
    fn init_partial_failure_at_midver_leaves_chip_none() {
        // The PKGTYP read succeeded (so we know which Chip variant the
        // package is), but the MIDVER probe returned garbage and init
        // bailed with UnsupportedChip. The driver MUST NOT leave
        // `self.chip = Some(...)` from the PKGTYP step — otherwise the
        // poll_link gate becomes a lie. Pre-seed `chip = Some(...)` to
        // simulate a prior successful init and confirm the failed
        // re-init resets it back to None.
        let mut mdio = MockMdio::new(vec![
            0x0000,                           // BMCR poll cleared
            regs::STS2_RESETC,                // STS2 with RESETC
            PHY_ID0_LAN867X,                  // PHY_ID0
            PHY_ID1_LAN867X_REV2,             // PHY_ID1
            regs::STRAP_CTRL0_PKGTYP_LAN8671, // STRAP_CTRL0 → would map to Lan8671
            0xDEAD,                           // MIDVER — wrong
        ]);
        let mut phy = PhyLan867x::new(0);
        phy.chip = Some(Chip::Lan8670); // pretend a previous init landed
        phy.plca_id = Some(5); // and configured PLCA
        let err = phy.init(&mut mdio).unwrap_err();
        assert!(matches!(err, PhyError::UnsupportedChip { .. }));
        assert_eq!(phy.chip, None, "chip must be None after MIDVER failure");
        assert_eq!(phy.plca_id, None);
    }

    #[test]
    fn init_partial_failure_at_mde_rmw_leaves_chip_none() {
        // Every MDIO call up through MIDVER succeeds, but the very
        // first write of the T1SPMACTL RMW fails — meaning MDE never
        // made it to the chip, so the driver isn't in the documented
        // post-init state. chip must stay None.
        //
        // Call layout (read+write count interleaved):
        //   0  BMCR write (soft_reset begin)
        //   1  BMCR read  (poll)
        //   2-5 STS2 indirection: 3 writes + 1 read
        //   6  PHY_ID0 read
        //   7  PHY_ID1 read
        //   8  STRAP_CTRL0 read
        //   9-12 MIDVER indirection
        //   13  ← first MMDCTRL write of T1SPMACTL RMW
        let mut mdio = MockMdio::with_failure(
            vec![
                0x0000,                           // BMCR poll
                regs::STS2_RESETC,                // STS2.RESETC
                PHY_ID0_LAN867X,                  // PHY_ID0
                PHY_ID1_LAN867X_REV2,             // PHY_ID1
                regs::STRAP_CTRL0_PKGTYP_LAN8671, // STRAP_CTRL0
                regs::MIDVER_EXPECTED,            // MIDVER OK
            ],
            13,
        );
        let mut phy = PhyLan867x::new(0);
        phy.chip = Some(Chip::Lan8672); // simulate prior init state
        let err = phy.init(&mut mdio).unwrap_err();
        assert!(matches!(err, PhyError::Mdio(MockError)));
        assert_eq!(phy.chip, None, "chip must be None after MDE step failure");
    }

    #[test]
    fn init_failure_makes_poll_link_report_none() {
        // End-to-end behavioural invariant: a failed init means
        // poll_link reports None, not the "always linked" shortcut.
        // This is the user-visible payoff of the atomic-init
        // contract.
        let mut mdio = MockMdio::new(vec![
            0x0000,
            regs::STS2_RESETC,
            PHY_ID0_LAN867X,
            PHY_ID1_LAN867X_REV2,
            regs::STRAP_CTRL0_PKGTYP_LAN8671,
            0xDEAD, // MIDVER bad → init fails
        ]);
        let mut phy = PhyLan867x::new(0);
        phy.chip = Some(Chip::Lan8671); // prior good init
        let _ = phy.init(&mut mdio); // expected to fail
                                     // After the failed re-init: poll_link must NOT claim a link.
        let mut empty_mdio = MockMdio::new(vec![]);
        let result = phy.poll_link(&mut empty_mdio).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn init_writes_resetc_handshake_indirection_before_phy_id_read() {
        // Behavioural ordering: the MMDCTRL/MMDAD writes that drive the
        // STS2 read MUST be issued BEFORE the PHY_ID0/1 reads. Without
        // the handshake first, the chip might still be holding its
        // configuration registers in reset.
        let mut mdio = MockMdio::new(reads_for_successful_init(regs::STRAP_CTRL0_PKGTYP_LAN8671));
        let mut phy = PhyLan867x::new(0);
        phy.init(&mut mdio).unwrap();

        // Find the position of the first MMDAD write addressing STS2,
        // and confirm that no PHY_ID read happens before it. The MMDAD
        // *value-write* is what carries the STS2 register address (it's
        // the second write in any read sequence).
        let sts2_addr_write_idx = mdio
            .writes
            .iter()
            .position(|&(_, reg, val)| reg == regs::REG_MMDAD && val == regs::MMD_REG_STS2)
            .expect("expected an MMDAD address-write targeting STS2");

        // PHY_ID reads come from `read_phy_id` — those are *reads*, not
        // writes, so we have to look at the call-count timing through
        // the read_idx instead. Equivalent invariant: the index in the
        // reads vector at which PHY_ID0 sits is index 2, and STS2's
        // value sits at index 1. The fact that mdio.read_idx hit 2
        // means STS2 was already consumed.
        assert!(mdio.read_idx >= 2);
        // And the writes log starts with: BMCR.RESET write, then the
        // MMDCTRL ADDR write, then MMDAD STS2 addr write — i.e. the
        // STS2 indirection precedes everything that comes after.
        assert!(
            sts2_addr_write_idx <= 2,
            "MMDAD STS2 write must be amongst the first three writes"
        );
    }

    // ── poll_link tests ────────────────────────────────────────────────

    #[test]
    fn poll_link_before_init_returns_none() {
        // Pre-init: chip = None ⇒ poll_link must NOT claim a working
        // link. The reset handshake hasn't run, T1SPMACTL.MDE is at the
        // chip's power-on default (zero), so there is no link to report.
        let mut mdio = MockMdio::new(vec![]);
        let mut phy = PhyLan867x::new(0);
        assert!(phy.poll_link(&mut mdio).unwrap().is_none());
        // No MDIO traffic on the un-init path either.
        assert!(mdio.writes.is_empty());
        assert_eq!(mdio.read_idx, 0);
    }

    #[test]
    fn poll_link_plca_disabled_reports_linked() {
        // No PLCA configured → "always linked" once init done. Simulate
        // the post-init state by seeding `chip` directly so the test
        // stays focused on the poll_link branch logic without dragging
        // in the seven-read init sequence.
        let mut mdio = MockMdio::new(vec![]);
        let mut phy = PhyLan867x::new(0);
        phy.chip = Some(Chip::Lan8671);
        let result = phy.poll_link(&mut mdio).unwrap();
        assert_eq!(result, Some(LinkStatus::new(Speed::Mbps10, Duplex::Half)));
        // Crucially: no MDIO traffic in the PLCA-off branch.
        assert!(mdio.writes.is_empty());
        assert_eq!(mdio.read_idx, 0);
    }

    #[test]
    fn poll_link_plca_enabled_pst_set_reports_linked() {
        let mut mdio = MockMdio::new(vec![regs::PLCA_STS_PST]);
        let mut phy = PhyLan867x::new(0);
        phy.chip = Some(Chip::Lan8671); // simulate post-init state
        phy.plca_id = Some(0); // simulate post-configure_plca state
        let result = phy.poll_link(&mut mdio).unwrap();
        assert_eq!(result, Some(LinkStatus::new(Speed::Mbps10, Duplex::Half)));
    }

    #[test]
    fn poll_link_plca_enabled_pst_clear_reports_none() {
        let mut mdio = MockMdio::new(vec![0x0000]); // PST clear
        let mut phy = PhyLan867x::new(0);
        phy.chip = Some(Chip::Lan8671);
        phy.plca_id = Some(1); // follower waiting for BEACONs
        let result = phy.poll_link(&mut mdio).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn poll_link_propagates_mdio_error() {
        let mut mdio = MockMdio::with_failure(vec![], 0);
        let mut phy = PhyLan867x::new(0);
        phy.chip = Some(Chip::Lan8671);
        phy.plca_id = Some(0); // forces the MDIO path
        let err = phy.poll_link(&mut mdio).unwrap_err();
        assert!(matches!(err, PhyError::Mdio(MockError)));
    }

    // ── capabilities / phy_id passthroughs ─────────────────────────────

    #[test]
    fn capabilities_passes_through_to_helper() {
        // BASIC_STATUS reset value on this chip: bit 11 (10BASE-T HD) = 1,
        // bit 0 (EXT_CAP) = 1. read_capabilities decodes the relevant bit.
        let bmsr = 1 << 11;
        let mut mdio = MockMdio::new(vec![bmsr]);
        let phy = PhyLan867x::new(0);
        let caps = phy.capabilities(&mut mdio).unwrap();
        assert!(caps.speed_10_hd);
    }

    #[test]
    fn phy_id_passes_through_to_helper() {
        let mut mdio = MockMdio::new(vec![PHY_ID0_LAN867X, PHY_ID1_LAN867X_REV2]);
        let phy = PhyLan867x::new(0);
        let id = phy.phy_id(&mut mdio).unwrap();
        assert_eq!(id, 0x0000_C562);
    }

    // ── PhyLan867xWithReset tests ──────────────────────────────────────

    #[derive(Default)]
    struct MockPin {
        history: Vec<bool>, // true = high, false = low
    }

    impl embedded_hal::digital::ErrorType for MockPin {
        type Error = core::convert::Infallible;
    }

    impl embedded_hal::digital::OutputPin for MockPin {
        fn set_low(&mut self) -> Result<(), core::convert::Infallible> {
            self.history.push(false);
            Ok(())
        }
        fn set_high(&mut self) -> Result<(), core::convert::Infallible> {
            self.history.push(true);
            Ok(())
        }
    }

    #[derive(Default)]
    struct MockDelay {
        delays_ms: Vec<u32>,
    }

    impl embedded_hal::delay::DelayNs for MockDelay {
        fn delay_ns(&mut self, ns: u32) {
            // Record in millisecond resolution — that's what
            // hardware_reset uses.
            self.delays_ms.push(ns / 1_000_000);
        }
        fn delay_ms(&mut self, ms: u32) {
            self.delays_ms.push(ms);
        }
    }

    #[test]
    fn with_reset_hardware_reset_drives_pin_low_then_high_with_delays() {
        let mut phy = PhyLan867xWithReset::new(0, MockPin::default());
        let mut delay = MockDelay::default();
        phy.hardware_reset(&mut delay).unwrap();
        assert_eq!(phy.reset_pin.history, vec![false, true]);
        // 10 ms low + 25 ms post-release.
        assert_eq!(delay.delays_ms, vec![10, 25]);
    }

    #[test]
    fn with_reset_phy_addr_passes_through() {
        let phy = PhyLan867xWithReset::new(5, MockPin::default());
        assert_eq!(phy.phy_addr(), 5);
    }

    // ── PLCA configure / disable / status tests ────────────────────────

    /// Find the last `MMDAD` data-write — i.e. the value the driver
    /// pushed to the most recent MMD register access. Used heavily in
    /// the PLCA tests since each `mmd_write` ends with an MMDAD write
    /// carrying the data.
    fn last_mmdad_data_write(mdio: &MockMdio) -> u16 {
        mdio.writes
            .iter()
            .rev()
            .find(|&&(_, reg, _)| reg == regs::REG_MMDAD)
            .map(|&(_, _, v)| v)
            .expect("expected at least one MMDAD data write")
    }

    /// Find the n-th MMDAD data-write (0-indexed). MMD writes alternate
    /// "address-write" (whose value is the MMD register address) and
    /// "data-write" (whose value is the register payload). To
    /// distinguish, we use the fact that the second write to MMDAD in
    /// any sequence carries the actual payload.
    fn mmdad_data_writes(mdio: &MockMdio) -> Vec<u16> {
        // Sequence per mmd_write call: [MMDCTRL_ADDR, MMDAD_addr,
        // MMDCTRL_DATA, MMDAD_data]. So every fourth write is a data
        // write — but only when each MMDCTRL/MMDAD pair belongs to a
        // single mmd_write. For mmd_rmw we have a read in the middle
        // (read-then-write), changing the pattern.
        //
        // Pragmatic approach: collect every MMDAD write, then keep the
        // ones whose preceding MMDCTRL.FNCTN = DATA. Since MMDCTRL is
        // always the one immediately before any MMDAD touch, the test
        // becomes: walk the writes, track the latest FNCTN, classify
        // each MMDAD write accordingly.
        let mut current_fnctn: u16 = 0;
        let mut data_values = Vec::new();
        for &(_, reg, val) in &mdio.writes {
            if reg == regs::REG_MMDCTRL {
                current_fnctn = val & 0xC000; // top two bits
            } else if reg == regs::REG_MMDAD && current_fnctn == regs::MMDCTRL_FNCTN_DATA {
                data_values.push(val);
            }
        }
        data_values
    }

    #[test]
    fn configure_plca_coordinator_writes_ctrl1_then_enables() {
        // Pre-RMW PLCA_CTRL0 read returns 0 (chip default), so the EN
        // write is exactly PLCA_CTRL0_EN.
        let mut mdio = MockMdio::new(vec![0x0000]);
        let mut phy = PhyLan867x::new(0);
        phy.configure_plca(
            &mut mdio,
            &PlcaConfig {
                node_id: 0,
                node_count: 8,
                burst_count: 0,
                burst_timer: 0,
            },
        )
        .unwrap();

        let writes = mmdad_data_writes(&mdio);
        // CTRL1 = (8 << 8) | 0 = 0x0800
        assert_eq!(writes[0], 0x0800, "CTRL1 = NCNT(8) << 8 | ID(0)");
        // PLCA_CTRL0 RMW final value = EN. Whatever else gets programmed
        // in between (PLCA_BURST), EN must be the last write — the
        // ordering invariant is what the dedicated test below enforces.
        assert_eq!(*writes.last().unwrap(), regs::PLCA_CTRL0_EN);
        assert_eq!(phy.plca_id, Some(0));
    }

    #[test]
    fn configure_plca_follower_with_burst() {
        let mut mdio = MockMdio::new(vec![0x0000]); // pre-RMW PLCA_CTRL0
        let mut phy = PhyLan867x::new(0);
        phy.configure_plca(
            &mut mdio,
            &PlcaConfig {
                node_id: 3,
                node_count: 8,
                burst_count: 2,
                burst_timer: 0x40,
            },
        )
        .unwrap();

        let writes = mmdad_data_writes(&mdio);
        // CTRL1 = 0x0803
        assert_eq!(writes[0], 0x0803);
        // BURST = (MAXBC << 8) | BTMR = 0x0240
        assert_eq!(writes[1], 0x0240);
        // CTRL0 EN.
        assert_eq!(writes[2], regs::PLCA_CTRL0_EN);
        assert_eq!(phy.plca_id, Some(3));
    }

    #[test]
    fn configure_plca_always_writes_burst_register_with_maxbc_zero() {
        // Re-configuring a previously-bursting node with `burst_count = 0`
        // must clear MAXBC on the chip. configure_plca therefore writes
        // PLCA_BURST unconditionally — MAXBC = 0 is the explicit
        // "burst disabled" encoding per datasheet sec 5.4.18.
        let mut mdio = MockMdio::new(vec![0x0000]);
        let mut phy = PhyLan867x::new(0);
        phy.configure_plca(
            &mut mdio,
            &PlcaConfig {
                node_id: 1,
                node_count: 8,
                burst_count: 0,
                burst_timer: 0, // sentinel
            },
        )
        .unwrap();

        // Find the PLCA_BURST write — its MMDAD address-write carries
        // 0xCA05; the immediately-following data-write is the value.
        let burst_data_idx = mdio
            .writes
            .iter()
            .position(|&(_, reg, val)| reg == regs::REG_MMDAD && val == regs::MMD_REG_PLCA_BURST)
            .expect("expected an MMDAD address-write to PLCA_BURST")
            + 2; // skip MMDCTRL_DATA write that follows
        let burst_value = mdio.writes[burst_data_idx].2;
        // MAXBC = 0 (burst disabled), BTMR = 0x80 (sentinel default).
        assert_eq!(burst_value, 0x0080);
    }

    #[test]
    fn configure_plca_burst_timer_zero_uses_sentinel_default() {
        // burst_count > 0 with burst_timer = 0 — the sentinel MUST be
        // applied so BTMR lands at the chip default (0x80) rather than
        // literally 0 (which would make burst mode non-functional).
        let mut mdio = MockMdio::new(vec![0x0000]);
        let mut phy = PhyLan867x::new(0);
        phy.configure_plca(
            &mut mdio,
            &PlcaConfig {
                node_id: 0,
                node_count: 8,
                burst_count: 4,
                burst_timer: 0, // sentinel ⇒ BTMR = 0x80
            },
        )
        .unwrap();

        let burst_data_idx = mdio
            .writes
            .iter()
            .position(|&(_, reg, val)| reg == regs::REG_MMDAD && val == regs::MMD_REG_PLCA_BURST)
            .unwrap()
            + 2;
        let burst_value = mdio.writes[burst_data_idx].2;
        // MAXBC = 4, BTMR = 0x80.
        assert_eq!(burst_value, 0x0480);
    }

    #[test]
    fn configure_plca_burst_timer_nonzero_passes_through() {
        // Non-zero burst_timer must NOT be touched by the sentinel.
        let mut mdio = MockMdio::new(vec![0x0000]);
        let mut phy = PhyLan867x::new(0);
        phy.configure_plca(
            &mut mdio,
            &PlcaConfig {
                node_id: 0,
                node_count: 8,
                burst_count: 4,
                burst_timer: 0x40,
            },
        )
        .unwrap();

        let burst_data_idx = mdio
            .writes
            .iter()
            .position(|&(_, reg, val)| reg == regs::REG_MMDAD && val == regs::MMD_REG_PLCA_BURST)
            .unwrap()
            + 2;
        let burst_value = mdio.writes[burst_data_idx].2;
        assert_eq!(burst_value, 0x0440);
    }

    #[test]
    fn configure_plca_rejects_id_0xff() {
        let mut mdio = MockMdio::new(vec![]);
        let mut phy = PhyLan867x::new(0);
        let err = phy
            .configure_plca(
                &mut mdio,
                &PlcaConfig {
                    node_id: 0xFF,
                    node_count: 8,
                    burst_count: 0,
                    burst_timer: 0,
                },
            )
            .unwrap_err();
        assert!(matches!(err, PlcaError::InvalidConfig));
        // Driver state must not have been touched on rejection.
        assert_eq!(phy.plca_id, None);
        // No MDIO traffic on early-rejection path.
        assert!(mdio.writes.is_empty());
    }

    #[test]
    fn configure_plca_rejects_follower_id_at_or_above_count() {
        // id=8 with count=8 means slot 8 doesn't exist (slots are 0..7).
        let mut mdio = MockMdio::new(vec![]);
        let mut phy = PhyLan867x::new(0);
        let err = phy
            .configure_plca(
                &mut mdio,
                &PlcaConfig {
                    node_id: 8,
                    node_count: 8,
                    burst_count: 0,
                    burst_timer: 0,
                },
            )
            .unwrap_err();
        assert!(matches!(err, PlcaError::InvalidConfig));
    }

    #[test]
    fn configure_plca_allows_follower_with_count_zero() {
        // Followers may legitimately set NCNT=0 (only the coordinator
        // strictly cares about that value). configure_plca must not
        // reject this combination.
        let mut mdio = MockMdio::new(vec![0x0000]);
        let mut phy = PhyLan867x::new(0);
        phy.configure_plca(
            &mut mdio,
            &PlcaConfig {
                node_id: 5,
                node_count: 0,
                burst_count: 0,
                burst_timer: 0,
            },
        )
        .unwrap();
    }

    #[test]
    fn configure_plca_writes_ctrl1_before_enable() {
        // Ordering invariant: PLCA_CTRL1 (NCNT/ID) must be programmed
        // *before* PLCA_CTRL0.EN flips on. Otherwise the chip would
        // start operating with stale ID/NCNT for one or more PLCA
        // cycles.
        let mut mdio = MockMdio::new(vec![0x0000]);
        let mut phy = PhyLan867x::new(0);
        phy.configure_plca(&mut mdio, &PlcaConfig::default())
            .unwrap();

        let writes = mmdad_data_writes(&mdio);
        // PLCA_CTRL0_EN must be the LAST write.
        assert_eq!(writes.last().copied().unwrap(), regs::PLCA_CTRL0_EN);
    }

    #[test]
    fn disable_plca_clears_en_and_internal_state() {
        // Pre-RMW returns EN set; expect the post-RMW write to clear it.
        let mut mdio = MockMdio::new(vec![regs::PLCA_CTRL0_EN]);
        let mut phy = PhyLan867x::new(0);
        phy.plca_id = Some(2);
        phy.disable_plca(&mut mdio).unwrap();
        assert_eq!(last_mmdad_data_write(&mdio), 0);
        assert_eq!(phy.plca_id, None);
    }

    #[test]
    fn plca_status_decodes_chip_state() {
        // Sequence of three MMD reads: CTRL0 (EN=1), CTRL1 (NCNT=8 ID=3),
        // STS (PST=1).
        let mut mdio = MockMdio::new(vec![regs::PLCA_CTRL0_EN, 0x0803, regs::PLCA_STS_PST]);
        let phy = PhyLan867x::new(0);
        let s = phy.plca_status(&mut mdio).unwrap();
        assert_eq!(
            s,
            PlcaStatus {
                enabled: true,
                node_id: 3,
                is_coordinator: false,
                stable: true,
            }
        );
    }

    #[test]
    fn plca_status_recognises_coordinator() {
        let mut mdio = MockMdio::new(vec![regs::PLCA_CTRL0_EN, 0x0800, regs::PLCA_STS_PST]);
        let phy = PhyLan867x::new(0);
        let s = phy.plca_status(&mut mdio).unwrap();
        assert!(s.is_coordinator);
        assert_eq!(s.node_id, 0);
    }

    #[test]
    fn plca_status_when_disabled() {
        // Chip default after init: EN=0, ID=0xFF (silicon power-up
        // value), PST=0.
        let mut mdio = MockMdio::new(vec![0x0000, 0x00FF, 0x0000]);
        let phy = PhyLan867x::new(0);
        let s = phy.plca_status(&mut mdio).unwrap();
        assert_eq!(
            s,
            PlcaStatus {
                enabled: false,
                node_id: 0xFF,
                is_coordinator: false,
                stable: false,
            }
        );
    }

    #[test]
    fn poll_link_after_configure_plca_uses_pst_branch() {
        // End-to-end: configure_plca records plca_id, then poll_link
        // must consult PLCA_STS instead of returning the "always linked"
        // shortcut. PST=0 ⇒ None.
        let mut mdio = MockMdio::new(vec![
            0x0000, // pre-RMW PLCA_CTRL0 read
            0x0000, // PLCA_STS read in poll_link
        ]);
        let mut phy = PhyLan867x::new(0);
        phy.chip = Some(Chip::Lan8671); // simulate post-init state
        phy.configure_plca(&mut mdio, &PlcaConfig::default())
            .unwrap();
        let result = phy.poll_link(&mut mdio).unwrap();
        assert!(result.is_none(), "PST=0 ⇒ link not yet up");
    }

    #[test]
    fn init_clears_cached_plca_id() {
        // After a soft reset the chip's PLCA_CTRL0/CTRL1 are back to
        // defaults (PLCA off). Driver-side `plca_id` must follow, or
        // `poll_link` would keep reading PLCA_STS and reporting None
        // forever even though the chip is actually CSMA/CD-ready.
        // Single-owner contract justifies clearing rather than reading
        // chip state back.
        let mut mdio = MockMdio::new(reads_for_successful_init(regs::STRAP_CTRL0_PKGTYP_LAN8671));
        let mut phy = PhyLan867x::new(0);
        phy.plca_id = Some(3); // simulate prior configure_plca
        phy.init(&mut mdio).unwrap();
        assert_eq!(phy.plca_id, None);
    }

    #[test]
    fn configure_plca_reconfigure_with_burst_zero_clears_chip_burst() {
        // Regression: before this fix, configure_plca skipped the
        // PLCA_BURST write whenever burst_count = 0, leaving prior
        // burst settings on the chip. Now the BURST register is
        // always programmed.
        let mut mdio = MockMdio::new(vec![
            0x0000, // pre-RMW PLCA_CTRL0 for first configure
            0x0000, // pre-RMW PLCA_CTRL0 for second configure
        ]);
        let mut phy = PhyLan867x::new(0);

        // First call: enable burst.
        phy.configure_plca(
            &mut mdio,
            &PlcaConfig {
                node_id: 0,
                node_count: 8,
                burst_count: 4,
                burst_timer: 0x40,
            },
        )
        .unwrap();

        // Second call: disable burst by passing burst_count = 0.
        phy.configure_plca(
            &mut mdio,
            &PlcaConfig {
                node_id: 0,
                node_count: 8,
                burst_count: 0,
                burst_timer: 0,
            },
        )
        .unwrap();

        // Walk the writes vector picking out every data-write to
        // PLCA_BURST. The data-write follows two writes after the
        // address-write that targets MMD_REG_PLCA_BURST: [MMDCTRL_ADDR,
        // MMDAD<addr>, MMDCTRL_DATA, MMDAD<data>].
        let mut burst_data_writes = Vec::new();
        for (i, &(_, reg, val)) in mdio.writes.iter().enumerate() {
            if reg == regs::REG_MMDAD && val == regs::MMD_REG_PLCA_BURST {
                // Next MMDAD write after this one is the data write.
                if let Some(data) = mdio.writes[i + 1..]
                    .iter()
                    .find(|w| w.1 == regs::REG_MMDAD)
                    .map(|w| w.2)
                {
                    burst_data_writes.push(data);
                }
            }
        }
        assert_eq!(
            burst_data_writes.len(),
            2,
            "expected one PLCA_BURST write per configure_plca call"
        );
        // First call enabled burst.
        assert_eq!(
            burst_data_writes[0], 0x0440,
            "first call: MAXBC=4, BTMR=0x40"
        );
        // Second call MUST land MAXBC=0 on the chip.
        assert_eq!(
            burst_data_writes[1] & 0xFF00,
            0x0000,
            "second call MUST clear MAXBC to 0"
        );
    }

    #[test]
    fn poll_link_after_disable_plca_returns_to_always_linked() {
        // configure → disable → poll_link must short-circuit to "linked"
        // again, with no MDIO traffic.
        let mut mdio = MockMdio::new(vec![
            0x0000,              // pre-RMW PLCA_CTRL0 for configure
            regs::PLCA_CTRL0_EN, // pre-RMW PLCA_CTRL0 for disable
        ]);
        let mut phy = PhyLan867x::new(0);
        phy.chip = Some(Chip::Lan8671); // simulate post-init state
        phy.configure_plca(&mut mdio, &PlcaConfig::default())
            .unwrap();
        phy.disable_plca(&mut mdio).unwrap();

        let writes_before = mdio.writes.len();
        let reads_before = mdio.read_idx;
        let result = phy.poll_link(&mut mdio).unwrap();
        assert_eq!(result, Some(LinkStatus::new(Speed::Mbps10, Duplex::Half)));
        // No additional MDIO traffic from poll_link.
        assert_eq!(mdio.writes.len(), writes_before);
        assert_eq!(mdio.read_idx, reads_before);
    }
}
