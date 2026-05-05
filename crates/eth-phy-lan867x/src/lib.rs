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
        let strap = mdio
            .read(self.addr, regs::REG_STRAP_CTRL0)
            .map_err(PhyError::Mdio)?;
        self.chip = Some(match strap & regs::STRAP_CTRL0_PKGTYP_MASK {
            regs::STRAP_CTRL0_PKGTYP_LAN8670 => Chip::Lan8670,
            regs::STRAP_CTRL0_PKGTYP_LAN8671 => Chip::Lan8671,
            regs::STRAP_CTRL0_PKGTYP_LAN8672 => Chip::Lan8672,
            _ => return Err(PhyError::UnsupportedChip { id }),
        });

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

        Ok(())
    }

    fn poll_link<M: MdioBus>(
        &mut self,
        mdio: &mut M,
    ) -> Result<Option<LinkStatus>, PhyError<M::Error>> {
        // On a 10BASE-T1S multidrop bus there is no autonegotiation and
        // no per-link-partner signal. Two distinct cases:
        //
        // - PLCA off (CSMA/CD): the bus is "always there". Report linked
        //   once init() has succeeded — the caller can attempt to send
        //   and the chip will handle (possibly colliding) transmissions.
        //
        // - PLCA on: PLCA_STS.PST tracks whether BEACONs are being TX'd
        //   (coordinator) or RX'd (follower). It is the only meaningful
        //   "are we participating in the network" indicator. Report
        //   linked when set; report None until the bus stabilises.
        match self.plca_id {
            None => Ok(Some(LinkStatus::new(Speed::Mbps10, Duplex::Half))),
            Some(_) => {
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

    /// Drive `RESET_N` low for ≥10 ms, then wait 25 ms after release
    /// before MDIO is touched. Conservative timings — datasheet sec 7.6.4
    /// allows shorter, but matches the lan87xx wrapper for consistency.
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
        assert!(matches!(err, PhyError::UnsupportedChip { .. }));
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
    fn poll_link_plca_disabled_reports_linked() {
        // No PLCA configured → "always linked" once init done.
        let mut mdio = MockMdio::new(vec![]);
        let mut phy = PhyLan867x::new(0);
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
        phy.plca_id = Some(0); // simulate post-configure_plca state
        let result = phy.poll_link(&mut mdio).unwrap();
        assert_eq!(result, Some(LinkStatus::new(Speed::Mbps10, Duplex::Half)));
    }

    #[test]
    fn poll_link_plca_enabled_pst_clear_reports_none() {
        let mut mdio = MockMdio::new(vec![0x0000]); // PST clear
        let mut phy = PhyLan867x::new(0);
        phy.plca_id = Some(1); // follower waiting for BEACONs
        let result = phy.poll_link(&mut mdio).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn poll_link_propagates_mdio_error() {
        let mut mdio = MockMdio::with_failure(vec![], 0);
        let mut phy = PhyLan867x::new(0);
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
}
