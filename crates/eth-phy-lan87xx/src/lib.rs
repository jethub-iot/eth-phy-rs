// SPDX-License-Identifier: GPL-2.0-or-later OR Apache-2.0
// Copyright (c) Viacheslav Bocharov <v@baodeep.com> and JetHome (r)

//! `#![no_std]` MDIO driver for the Microchip LAN87xx family of 10/100
//! Ethernet PHYs:
//!
//! - LAN8710A
//! - LAN8720A
//! - LAN8740A
//! - LAN8741A
//! - LAN8742A
//!
//! Implements [`eth_mdio_phy::PhyDriver`], so any MAC that exposes
//! [`eth_mdio_phy::MdioBus`] can drive the chip — typical case is the
//! ESP32 built-in EMAC SMI controller via
//! [`esp_emac::mdio::EspMdio`](https://docs.rs/esp-emac).
//!
//! See the crate-level README (rendered on docs.rs and shipped via
//! `Cargo.toml`'s `readme` field) for installation, the full
//! embassy-net example via `esp-emac`, and a troubleshooting checklist
//! covering the cold-boot ANAR quirk, MDIO bus failures, and strap-pin
//! pitfalls.
//!
//! # Quick start
//!
//! ```no_run
//! use eth_mdio_phy::{MdioBus, PhyDriver};
//! use eth_phy_lan87xx::PhyLan87xx;
//!
//! # fn example<M: MdioBus>(mdio: &mut M)
//! # -> Result<(), eth_mdio_phy::PhyError<M::Error>>
//! # {
//! let mut phy = PhyLan87xx::new(/* PHY MDIO address */ 1);
//! phy.init(mdio)?;
//! loop {
//!     if let Some(status) = phy.poll_link(mdio)? {
//!         // status.speed, status.duplex
//!         break;
//!     }
//! }
//! # Ok(())
//! # }
//! ```
//!
//! # Crate features
//!
//! | Feature | Default | When to enable |
//! | --- | --- | --- |
//! | `defmt` | off | Adds `defmt::Format` derives via `eth-mdio-phy/defmt`. |
//!
//! # Cold-boot ANAR quirk
//!
//! On a cold boot, `BMCR.RESET` does NOT restore `ANAR` to
//! `0x01E1` on the LAN87xx family. The driver writes `ANAR = 0x01E1`
//! explicitly before kicking auto-negotiation; if you reimplement
//! this elsewhere, do the same — see the crate README's troubleshooting
//! section.

#![no_std]

mod regs;

use eth_mdio_phy::ieee802_3;
use eth_mdio_phy::{Duplex, LinkStatus, MdioBus, PhyCapabilities, PhyDriver, PhyError, Speed};

/// LAN87xx PHY driver (software-only, no reset pin).
pub struct PhyLan87xx {
    addr: u8,
    link_up: bool,
}

impl PhyLan87xx {
    /// Create a new driver for the PHY at the given MDIO address.
    pub fn new(addr: u8) -> Self {
        Self {
            addr,
            link_up: false,
        }
    }

    /// Decode the PSCSR speed/duplex indication field into a [`LinkStatus`].
    ///
    /// Returns `None` if the field contains a reserved or unrecognised value.
    fn parse_pscsr(pscsr_val: u16) -> Option<LinkStatus> {
        match pscsr_val & regs::pscsr::SPEED_DUPLEX_MASK {
            regs::pscsr::SPEED_10_HD => Some(LinkStatus::new(Speed::Mbps10, Duplex::Half)),
            regs::pscsr::SPEED_10_FD => Some(LinkStatus::new(Speed::Mbps10, Duplex::Full)),
            regs::pscsr::SPEED_100_HD => Some(LinkStatus::new(Speed::Mbps100, Duplex::Half)),
            regs::pscsr::SPEED_100_FD => Some(LinkStatus::new(Speed::Mbps100, Duplex::Full)),
            _ => None,
        }
    }

    /// Enable or disable MII loopback per IEEE 802.3 Clause 22 (BMCR bit 14).
    ///
    /// **MUST be called after [`PhyDriver::init`] completes.** `init`
    /// ends with `enable_auto_negotiation`, which writes BMCR — calling
    /// `set_loopback(true)` before `init` would be overwritten.
    ///
    /// # Behaviour
    ///
    /// The on/off paths are intentionally asymmetric:
    ///
    /// - **`on = true`**: full overwrite of BMCR with
    ///   `LOOPBACK | SPEED_100 | DUPLEX_FULL` (0x6100). Auto-negotiation
    ///   is bypassed deliberately — MII loopback skips PMD link
    ///   establishment, so advertising over the wire is pointless and
    ///   leaving `AN_ENABLE` set would cause the PHY to fight the
    ///   loopback configuration during the next negotiation cycle.
    /// - **`on = false`**: read-modify-write that clears only bit 14,
    ///   preserving all other BMCR settings. Whatever the BMCR held
    ///   before the call (forced link, isolate, etc.) keeps its state.
    ///   Note that calling this after a `set_loopback(true)` does **not**
    ///   restore auto-negotiation — the caller must re-run
    ///   [`PhyDriver::init`] or invoke
    ///   `eth_mdio_phy::ieee802_3::enable_auto_negotiation` to bring the
    ///   PHY back to a negotiated link.
    ///
    /// # Caveats
    ///
    /// - `BMSR.LINK_STATUS` may clamp to 0 in loopback on some LAN8720A
    ///   silicon revisions; callers must NOT gate on
    ///   [`PhyDriver::poll_link`] while loopback is on.
    /// - MII loopback loops at the digital MII layer (LAN8720A datasheet
    ///   rev D §3.5.1). The RMII REF_CLK (50 MHz) is still required
    ///   externally — disabling the external clock source breaks the
    ///   loopback path.
    pub fn set_loopback<M: MdioBus>(
        &mut self,
        mdio: &mut M,
        on: bool,
    ) -> Result<(), PhyError<M::Error>> {
        if on {
            // Full overwrite: loopback bypasses auto-negotiation, so
            // pin speed/duplex deterministically and clear AN_ENABLE
            // along with every other BMCR bit in a single write.
            let value = ieee802_3::bmcr::LOOPBACK
                | ieee802_3::bmcr::SPEED_100
                | ieee802_3::bmcr::DUPLEX_FULL;
            mdio.write(self.addr, ieee802_3::regs::BMCR, value)
                .map_err(PhyError::Mdio)?;
            // Loopback short-circuits the link reporting path on some
            // LAN8720A silicon — invalidate cached link state so the
            // next caller-driven poll starts from "unknown".
            self.link_up = false;
        } else {
            // RMW clear of bit 14: preserve every other BMCR bit so
            // callers can layer loopback on top of forced-link, isolate,
            // or a manually-staged auto-neg restart without surprises.
            let cur = mdio
                .read(self.addr, ieee802_3::regs::BMCR)
                .map_err(PhyError::Mdio)?;
            mdio.write(
                self.addr,
                ieee802_3::regs::BMCR,
                cur & !ieee802_3::bmcr::LOOPBACK,
            )
            .map_err(PhyError::Mdio)?;
        }
        Ok(())
    }
}

impl PhyDriver for PhyLan87xx {
    fn phy_addr(&self) -> u8 {
        self.addr
    }

    fn init<M: MdioBus>(&mut self, mdio: &mut M) -> Result<(), PhyError<M::Error>> {
        // 1. Soft reset — check timeout
        let cleared = ieee802_3::soft_reset(mdio, self.addr, 500).map_err(PhyError::Mdio)?;
        if !cleared {
            return Err(PhyError::ResetTimeout);
        }

        // 2. Verify PHY ID matches LAN87xx family
        let id = ieee802_3::read_phy_id(mdio, self.addr).map_err(PhyError::Mdio)?;
        if id & regs::PHY_OUI_MASK != regs::PHY_OUI {
            return Err(PhyError::UnsupportedChip { id });
        }

        // 3. Disable Energy Detect Power-Down (EDPD) for reliable link detection
        let mcsr = mdio
            .read(self.addr, regs::mcsr::ADDR)
            .map_err(PhyError::Mdio)?;
        mdio.write(self.addr, regs::mcsr::ADDR, mcsr & !regs::mcsr::EDPD_EN)
            .map_err(PhyError::Mdio)?;

        // 4. Advertise standard 10/100 capabilities.
        //
        // After a cold boot, a soft-reset via BMCR.RESET does not always
        // restore ANAR to its documented default of 0x01E1 — the
        // register can retain a partial advertisement seeded from the
        // silicon's reset-strap latch instead of the spec default.
        // Without an explicit write, auto-negotiation starts with a
        // truncated advertisement: the partner negotiates 100/Full,
        // BMSR.LINK_STATUS goes up, but unicast RX is dead at the PHY
        // layer. Write the standard 10/100 selector explicitly to
        // sidestep the strap state.
        let anar = ieee802_3::anar::TX_FD
            | ieee802_3::anar::TX_HD
            | ieee802_3::anar::T10_FD
            | ieee802_3::anar::T10_HD
            | ieee802_3::anar::SELECTOR_IEEE802_3;
        mdio.write(self.addr, ieee802_3::regs::ANAR, anar)
            .map_err(PhyError::Mdio)?;

        // 5. Enable auto-negotiation
        ieee802_3::enable_auto_negotiation(mdio, self.addr).map_err(PhyError::Mdio)?;

        self.link_up = false;
        Ok(())
    }

    fn poll_link<M: MdioBus>(
        &mut self,
        mdio: &mut M,
    ) -> Result<Option<LinkStatus>, PhyError<M::Error>> {
        let up = ieee802_3::is_link_up(mdio, self.addr).map_err(PhyError::Mdio)?;
        if !up {
            self.link_up = false;
            return Ok(None);
        }

        // Auto-negotiation vs. forced link is decided by BMCR.AN_ENABLE.
        // The two paths have different validity rules and use different
        // registers to read back speed/duplex.
        let bmcr = mdio
            .read(self.addr, ieee802_3::regs::BMCR)
            .map_err(PhyError::Mdio)?;

        let status = if bmcr & ieee802_3::bmcr::AN_ENABLE != 0 {
            // Auto-neg path: PSCSR speed/duplex bits are only valid
            // after AUTODONE is set. On parallel-detection links
            // BMSR.LINK_STATUS can latch high while auto-negotiation
            // is still converging, and reading PSCSR in that window
            // returns indeterminate speed bits — exactly the class of
            // bug the explicit ANAR write is meant to prevent.
            let pscsr = mdio
                .read(self.addr, regs::pscsr::ADDR)
                .map_err(PhyError::Mdio)?;
            if pscsr & regs::pscsr::AUTODONE == 0 {
                self.link_up = false;
                return Ok(None);
            }
            Self::parse_pscsr(pscsr)
        } else {
            // Forced-link path (`ieee802_3::force_link` clears
            // AN_ENABLE and programs SPEED_100 / DUPLEX_FULL directly
            // in BMCR). PSCSR may never set AUTODONE in this mode, so
            // read speed/duplex straight from BMCR. Link is reported
            // as soon as BMSR.LINK_STATUS goes up.
            let speed = if bmcr & ieee802_3::bmcr::SPEED_100 != 0 {
                Speed::Mbps100
            } else {
                Speed::Mbps10
            };
            let duplex = if bmcr & ieee802_3::bmcr::DUPLEX_FULL != 0 {
                Duplex::Full
            } else {
                Duplex::Half
            };
            Some(LinkStatus::new(speed, duplex))
        };

        self.link_up = status.is_some();
        Ok(status)
    }

    fn capabilities<M: MdioBus>(
        &self,
        mdio: &mut M,
    ) -> Result<PhyCapabilities, PhyError<M::Error>> {
        ieee802_3::read_capabilities(mdio, self.addr).map_err(PhyError::Mdio)
    }

    fn phy_id<M: MdioBus>(&self, mdio: &mut M) -> Result<u32, PhyError<M::Error>> {
        ieee802_3::read_phy_id(mdio, self.addr).map_err(PhyError::Mdio)
    }
}

/// LAN87xx PHY driver with a hardware reset pin.
///
/// Wraps [`PhyLan87xx`] and adds [`hardware_reset`](Self::hardware_reset)
/// for toggling the PHY nRST line before initialisation.
pub struct PhyLan87xxWithReset<P: embedded_hal::digital::OutputPin> {
    inner: PhyLan87xx,
    reset_pin: P,
}

impl<P: embedded_hal::digital::OutputPin> PhyLan87xxWithReset<P> {
    /// Create a new driver with the given MDIO address and reset pin.
    pub fn new(addr: u8, pin: P) -> Self {
        Self {
            inner: PhyLan87xx::new(addr),
            reset_pin: pin,
        }
    }

    /// Perform a hardware reset via the nRST pin.
    ///
    /// Drives the pin low for 2 ms (min 100 us per datasheet), then
    /// releases and waits 25 ms for PHY internal init to complete
    /// before MDIO is accessible (LAN8720A datasheet Table 4-2).
    pub fn hardware_reset<D: embedded_hal::delay::DelayNs>(
        &mut self,
        delay: &mut D,
    ) -> Result<(), P::Error> {
        self.reset_pin.set_low()?;
        delay.delay_ms(2);
        self.reset_pin.set_high()?;
        delay.delay_ms(25);
        Ok(())
    }

    /// Enable or disable MII loopback. Delegates to
    /// [`PhyLan87xx::set_loopback`] — see that method for the full
    /// contract (asymmetric on/off semantics, must follow `init`,
    /// LAN8720A-specific caveats).
    pub fn set_loopback<M: MdioBus>(
        &mut self,
        mdio: &mut M,
        on: bool,
    ) -> Result<(), PhyError<M::Error>> {
        self.inner.set_loopback(mdio, on)
    }
}

impl<P: embedded_hal::digital::OutputPin> PhyDriver for PhyLan87xxWithReset<P> {
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
    use eth_mdio_phy::ieee802_3::{bmcr, bmsr};

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
            let val = *self
                .reads
                .get(self.read_idx)
                .expect("MockMdio: reads vector exhausted — test needs more entries");
            self.read_idx += 1;
            Ok(val)
        }

        fn write(&mut self, phy_addr: u8, reg_addr: u8, value: u16) -> Result<(), Self::Error> {
            if self.fail_at == Some(self.call_count) {
                self.call_count += 1;
                return Err(MockError);
            }
            self.call_count += 1;
            self.writes.push((phy_addr, reg_addr, value));
            Ok(())
        }
    }

    // ── Constructor tests ──────────────────────────────────────────────

    #[test]
    fn new_sets_address() {
        let phy = PhyLan87xx::new(3);
        assert_eq!(phy.phy_addr(), 3);
    }

    #[test]
    fn new_link_starts_down() {
        let phy = PhyLan87xx::new(0);
        assert!(!phy.link_up);
    }

    // ── init tests ─────────────────────────────────────────────────────

    #[test]
    fn init_success() {
        // Mock reads sequence:
        //   [0] BMCR read (0x0000 — reset cleared immediately)
        //   [1] PHYIDR1 (0x0007)
        //   [2] PHYIDR2 (0xC0F0) — LAN8720A
        //   [3] MCSR read (EDPD_EN set)
        //   [4] BMCR read for enable_auto_negotiation
        let mut mdio = MockMdio::new(vec![
            0x0000,              // soft_reset poll → cleared
            0x0007,              // PHYIDR1
            0xC0F0,              // PHYIDR2
            regs::mcsr::EDPD_EN, // MCSR with EDPD set
            0x0000,              // BMCR for enable_auto_negotiation
        ]);
        let mut phy = PhyLan87xx::new(1);
        phy.init(&mut mdio).unwrap();
    }

    #[test]
    fn init_rejects_wrong_phy_id() {
        // soft_reset succeeds, then PHY ID does not match LAN87xx OUI
        let mut mdio = MockMdio::new(vec![
            0x0000, // soft_reset poll → cleared
            0x0022, // PHYIDR1 (wrong)
            0x1619, // PHYIDR2 (wrong)
        ]);
        let mut phy = PhyLan87xx::new(1);
        let err = phy.init(&mut mdio).unwrap_err();
        match err {
            PhyError::UnsupportedChip { id } => assert_eq!(id, 0x0022_1619),
            _ => panic!("expected UnsupportedChip, got {:?}", err),
        }
    }

    #[test]
    fn init_reset_timeout() {
        // soft_reset: all reads return RESET set → returns false → ResetTimeout
        // Buffer larger than max_attempts (500) to avoid brittle coupling
        let mut mdio = MockMdio::new(vec![bmcr::RESET; 1000]);
        let mut phy = PhyLan87xx::new(1);
        let err = phy.init(&mut mdio).unwrap_err();
        match err {
            PhyError::ResetTimeout => {}
            _ => panic!("expected ResetTimeout, got {:?}", err),
        }
    }

    #[test]
    fn init_writes_anar_standard_advertisement() {
        // Cold-boot soft-reset does not always restore ANAR to its
        // default value, so init must write the standard 10/100
        // advertisement explicitly.
        let mut mdio = MockMdio::new(vec![
            0x0000,              // soft_reset poll
            0x0007,              // PHYIDR1
            0xC0F0,              // PHYIDR2
            regs::mcsr::EDPD_EN, // MCSR
            0x0000,              // BMCR for enable_auto_negotiation
        ]);
        let mut phy = PhyLan87xx::new(1);
        phy.init(&mut mdio).unwrap();

        let anar_idx = mdio
            .writes
            .iter()
            .position(|&(_, reg, _)| reg == eth_mdio_phy::ieee802_3::regs::ANAR)
            .expect("expected a write to ANAR");
        let expected = eth_mdio_phy::ieee802_3::anar::TX_FD
            | eth_mdio_phy::ieee802_3::anar::TX_HD
            | eth_mdio_phy::ieee802_3::anar::T10_FD
            | eth_mdio_phy::ieee802_3::anar::T10_HD
            | eth_mdio_phy::ieee802_3::anar::SELECTOR_IEEE802_3;
        assert_eq!(
            mdio.writes[anar_idx].2, expected,
            "ANAR must advertise standard 10/100 full+half + 802.3 selector"
        );

        // The whole point of writing ANAR explicitly is to seed the
        // advertisement BEFORE auto-neg restarts. Use `rposition` rather
        // than `position` so the assertion catches a regression where a
        // future refactor inserts an extra BMCR.AN write *before* ANAR
        // — `position` would find the earliest match and silently pass.
        let bmcr_an_idx = mdio
            .writes
            .iter()
            .rposition(|&(_, reg, val)| {
                reg == eth_mdio_phy::ieee802_3::regs::BMCR
                    && (val
                        & (eth_mdio_phy::ieee802_3::bmcr::AN_ENABLE
                            | eth_mdio_phy::ieee802_3::bmcr::AN_RESTART))
                        != 0
            })
            .expect("expected a BMCR write that enables/restarts auto-neg");
        assert!(
            anar_idx < bmcr_an_idx,
            "ANAR (write #{anar_idx}) must be programmed BEFORE BMCR.AN_ENABLE/AN_RESTART (write #{bmcr_an_idx})",
        );

        // Behavioural invariant (not a write-count one): no BMCR write
        // that enables/restarts auto-negotiation must occur BEFORE the
        // ANAR write — that would kick negotiation against the stale
        // advertisement, defeating the whole point of writing ANAR
        // explicitly. Anything else (vendor setup, status acks, LED
        // tweaks) is fair game: only the AN_RESTART that actually
        // triggers negotiation needs to see the explicit ANAR value.
        let pre_anar_an_restart = mdio.writes[..anar_idx].iter().any(|&(_, reg, val)| {
            reg == eth_mdio_phy::ieee802_3::regs::BMCR
                && (val
                    & (eth_mdio_phy::ieee802_3::bmcr::AN_ENABLE
                        | eth_mdio_phy::ieee802_3::bmcr::AN_RESTART))
                    != 0
        });
        assert!(
            !pre_anar_an_restart,
            "BMCR.AN_ENABLE/AN_RESTART must not be issued before the ANAR write",
        );
    }

    #[test]
    fn init_disables_edpd() {
        // Same as init_success; verify the MCSR write clears EDPD_EN
        let mcsr_initial: u16 = regs::mcsr::EDPD_EN | regs::mcsr::ENERGYON;
        let mut mdio = MockMdio::new(vec![
            0x0000,       // soft_reset poll
            0x0007,       // PHYIDR1
            0xC0F0,       // PHYIDR2
            mcsr_initial, // MCSR read
            0x0000,       // BMCR for enable_auto_negotiation
        ]);
        let mut phy = PhyLan87xx::new(1);
        phy.init(&mut mdio).unwrap();

        // Find the MCSR write — it targets register 17
        let mcsr_write = mdio
            .writes
            .iter()
            .find(|&&(_, reg, _)| reg == regs::mcsr::ADDR)
            .expect("expected a write to MCSR");
        // EDPD_EN must be cleared, ENERGYON must be preserved
        assert_eq!(
            mcsr_write.2 & regs::mcsr::EDPD_EN,
            0,
            "EDPD_EN should be cleared"
        );
        assert_ne!(
            mcsr_write.2 & regs::mcsr::ENERGYON,
            0,
            "other MCSR bits should be preserved"
        );
    }

    #[test]
    fn init_mdio_error_propagates() {
        // Fail on call 0 (the BMCR write inside soft_reset)
        let mut mdio = MockMdio::with_failure(vec![], 0);
        let mut phy = PhyLan87xx::new(1);
        let err = phy.init(&mut mdio).unwrap_err();
        match err {
            PhyError::Mdio(MockError) => {}
            _ => panic!("expected Mdio error, got {:?}", err),
        }
    }

    // ── poll_link tests ────────────────────────────────────────────────

    #[test]
    fn poll_link_down() {
        // BMSR without LINK_STATUS → link down
        let mut mdio = MockMdio::new(vec![0x0000]);
        let mut phy = PhyLan87xx::new(1);
        let result = phy.poll_link(&mut mdio).unwrap();
        assert!(result.is_none());
        assert!(!phy.link_up);
    }

    #[test]
    fn poll_link_100_full() {
        let mut mdio = MockMdio::new(vec![
            bmsr::LINK_STATUS,                                 // is_link_up → true
            ieee802_3::bmcr::AN_ENABLE,                        // BMCR — auto-neg path
            regs::pscsr::AUTODONE | regs::pscsr::SPEED_100_FD, // PSCSR → 100 Mbps full duplex
        ]);
        let mut phy = PhyLan87xx::new(1);
        let result = phy.poll_link(&mut mdio).unwrap();
        assert_eq!(result, Some(LinkStatus::new(Speed::Mbps100, Duplex::Full)));
        assert!(phy.link_up);
    }

    #[test]
    fn poll_link_10_half() {
        let mut mdio = MockMdio::new(vec![
            bmsr::LINK_STATUS,
            ieee802_3::bmcr::AN_ENABLE,
            regs::pscsr::AUTODONE | regs::pscsr::SPEED_10_HD,
        ]);
        let mut phy = PhyLan87xx::new(1);
        let result = phy.poll_link(&mut mdio).unwrap();
        assert_eq!(result, Some(LinkStatus::new(Speed::Mbps10, Duplex::Half)));
    }

    #[test]
    fn poll_link_100_half() {
        let mut mdio = MockMdio::new(vec![
            bmsr::LINK_STATUS,
            ieee802_3::bmcr::AN_ENABLE,
            regs::pscsr::AUTODONE | regs::pscsr::SPEED_100_HD,
        ]);
        let mut phy = PhyLan87xx::new(1);
        let result = phy.poll_link(&mut mdio).unwrap();
        assert_eq!(result, Some(LinkStatus::new(Speed::Mbps100, Duplex::Half)));
    }

    #[test]
    fn poll_link_10_full() {
        let mut mdio = MockMdio::new(vec![
            bmsr::LINK_STATUS,
            ieee802_3::bmcr::AN_ENABLE,
            regs::pscsr::AUTODONE | regs::pscsr::SPEED_10_FD,
        ]);
        let mut phy = PhyLan87xx::new(1);
        let result = phy.poll_link(&mut mdio).unwrap();
        assert_eq!(result, Some(LinkStatus::new(Speed::Mbps10, Duplex::Full)));
    }

    #[test]
    fn poll_link_unknown_speed_returns_none() {
        // PSCSR with 0b000 in speed/duplex field → unrecognised
        let mut mdio = MockMdio::new(vec![
            bmsr::LINK_STATUS,
            ieee802_3::bmcr::AN_ENABLE,
            regs::pscsr::AUTODONE, // AUTODONE set, speed bits = 0b000
        ]);
        let mut phy = PhyLan87xx::new(1);
        let result = phy.poll_link(&mut mdio).unwrap();
        assert!(result.is_none());
        assert!(!phy.link_up);
    }

    #[test]
    fn poll_link_returns_none_when_autodone_clear() {
        // Parallel-detection race: BMSR.LINK_STATUS latches high while
        // auto-negotiation is still converging. PSCSR speed bits are
        // indeterminate in that window; poll_link must report "no link
        // yet" so the caller keeps polling instead of acting on garbage.
        let mut mdio = MockMdio::new(vec![
            bmsr::LINK_STATUS,
            ieee802_3::bmcr::AN_ENABLE,
            regs::pscsr::SPEED_100_FD, // valid-looking, but AUTODONE not set
        ]);
        let mut phy = PhyLan87xx::new(1);
        let result = phy.poll_link(&mut mdio).unwrap();
        assert!(
            result.is_none(),
            "must wait for AUTODONE before decoding speed"
        );
        assert!(!phy.link_up);
    }

    #[test]
    fn poll_link_forced_100_full() {
        // ieee802_3::force_link clears AN_ENABLE and writes
        // SPEED_100 | DUPLEX_FULL into BMCR. AUTODONE may never set
        // in this mode, so poll_link must decode straight from BMCR.
        let mut mdio = MockMdio::new(vec![
            bmsr::LINK_STATUS,
            ieee802_3::bmcr::SPEED_100 | ieee802_3::bmcr::DUPLEX_FULL,
        ]);
        let mut phy = PhyLan87xx::new(1);
        let result = phy.poll_link(&mut mdio).unwrap();
        assert_eq!(result, Some(LinkStatus::new(Speed::Mbps100, Duplex::Full)));
        assert!(phy.link_up);
    }

    #[test]
    fn poll_link_forced_10_half() {
        // forced 10HD: AN_ENABLE/SPEED_100/DUPLEX_FULL all clear.
        let mut mdio = MockMdio::new(vec![bmsr::LINK_STATUS, 0x0000]);
        let mut phy = PhyLan87xx::new(1);
        let result = phy.poll_link(&mut mdio).unwrap();
        assert_eq!(result, Some(LinkStatus::new(Speed::Mbps10, Duplex::Half)));
    }

    #[test]
    fn poll_link_forced_skips_pscsr_read() {
        // In forced-link mode poll_link must NOT read PSCSR — providing
        // only two read responses (BMSR + BMCR) would panic in the
        // mock if a third read happened.
        let mut mdio = MockMdio::new(vec![
            bmsr::LINK_STATUS,
            ieee802_3::bmcr::SPEED_100, // SPEED_100, DUPLEX clear → 100/Half
        ]);
        let mut phy = PhyLan87xx::new(1);
        let result = phy.poll_link(&mut mdio).unwrap();
        assert_eq!(result, Some(LinkStatus::new(Speed::Mbps100, Duplex::Half)));
    }

    #[test]
    fn poll_link_mdio_error() {
        // Fail on the first call (BMSR read inside is_link_up)
        let mut mdio = MockMdio::with_failure(vec![], 0);
        let mut phy = PhyLan87xx::new(1);
        let err = phy.poll_link(&mut mdio).unwrap_err();
        match err {
            PhyError::Mdio(MockError) => {}
            _ => panic!("expected Mdio error"),
        }
    }

    // ── capabilities tests ─────────────────────────────────────────────

    #[test]
    fn capabilities_reads_bmsr() {
        let bmsr_val = bmsr::TX_FD_CAPABLE
            | bmsr::TX_HD_CAPABLE
            | bmsr::T10_FD_CAPABLE
            | bmsr::T10_HD_CAPABLE
            | bmsr::AN_ABILITY;
        let mut mdio = MockMdio::new(vec![bmsr_val]);
        let phy = PhyLan87xx::new(1);
        let caps = phy.capabilities(&mut mdio).unwrap();
        assert!(caps.speed_100_fd);
        assert!(caps.speed_100_hd);
        assert!(caps.speed_10_fd);
        assert!(caps.speed_10_hd);
        assert!(caps.auto_negotiation);
    }

    // ── phy_id tests ───────────────────────────────────────────────────

    #[test]
    fn phy_id_reads_registers() {
        let mut mdio = MockMdio::new(vec![0x0007, 0xC0F0]);
        let phy = PhyLan87xx::new(1);
        let id = phy.phy_id(&mut mdio).unwrap();
        assert_eq!(id, 0x0007_C0F0);
    }

    // ── set_loopback tests ─────────────────────────────────────────────

    #[test]
    fn set_loopback_on_writes_full_bmcr_overwrite() {
        // ON path: single write, full BMCR value, no preceding read.
        // Value must be LOOPBACK | SPEED_100 | DUPLEX_FULL = 0x6100.
        let mut mdio = MockMdio::new(vec![]);
        let mut phy = PhyLan87xx::new(1);
        phy.set_loopback(&mut mdio, true).unwrap();

        assert_eq!(
            mdio.writes.len(),
            1,
            "set_loopback(true) must issue exactly one BMCR write"
        );
        let (phy_addr, reg_addr, value) = mdio.writes[0];
        assert_eq!(phy_addr, 1, "must target the configured PHY address");
        assert_eq!(reg_addr, ieee802_3::regs::BMCR, "must target BMCR");
        assert_eq!(
            value, 0x6100,
            "BMCR overwrite must equal LOOPBACK | SPEED_100 | DUPLEX_FULL (0x6100), got 0x{value:04x}"
        );
        // Sanity: decompose the literal so a future bit-rename surfaces here.
        assert_eq!(
            value,
            bmcr::LOOPBACK | bmcr::SPEED_100 | bmcr::DUPLEX_FULL,
            "BMCR overwrite bit composition drifted from spec"
        );
        // Auto-negotiation must NOT be enabled in the loopback BMCR —
        // leaving AN_ENABLE on would have the PHY fight loopback at the
        // next negotiation tick.
        assert_eq!(
            value & bmcr::AN_ENABLE,
            0,
            "AN_ENABLE must be cleared by the full BMCR overwrite"
        );
    }

    #[test]
    fn set_loopback_off_rmw_preserves_other_bits() {
        // OFF path: read BMCR, write back with bit 14 cleared, every
        // other bit preserved. Seed BMCR with an arbitrary non-loopback
        // value (0x3000 = bits 13+12 = SPEED_SELECT_100 | AN_ENABLE) so
        // we can prove the mask is exactly !LOOPBACK and nothing wider.
        let initial: u16 = 0x3000;
        let mut mdio = MockMdio::new(vec![initial]);
        let mut phy = PhyLan87xx::new(1);
        phy.set_loopback(&mut mdio, false).unwrap();

        assert_eq!(
            mdio.writes.len(),
            1,
            "set_loopback(false) must issue exactly one BMCR write"
        );
        let (phy_addr, reg_addr, value) = mdio.writes[0];
        assert_eq!(phy_addr, 1);
        assert_eq!(reg_addr, ieee802_3::regs::BMCR);
        assert_eq!(
            value,
            initial & !bmcr::LOOPBACK,
            "OFF must clear only bit 14; expected 0x{:04x}, got 0x{:04x}",
            initial & !bmcr::LOOPBACK,
            value
        );
    }

    #[test]
    fn set_loopback_off_clears_existing_loopback_bit() {
        // BMCR already has LOOPBACK set alongside other state — the OFF
        // path must clear bit 14 while leaving the rest intact.
        let initial: u16 = bmcr::LOOPBACK | bmcr::SPEED_100 | bmcr::DUPLEX_FULL;
        let mut mdio = MockMdio::new(vec![initial]);
        let mut phy = PhyLan87xx::new(1);
        phy.set_loopback(&mut mdio, false).unwrap();

        let (_, _, value) = mdio.writes[0];
        assert_eq!(value & bmcr::LOOPBACK, 0, "LOOPBACK must be cleared");
        assert_ne!(
            value & bmcr::SPEED_100,
            0,
            "SPEED_100 must be preserved across the RMW"
        );
        assert_ne!(
            value & bmcr::DUPLEX_FULL,
            0,
            "DUPLEX_FULL must be preserved across the RMW"
        );
    }

    #[test]
    fn set_loopback_on_invalidates_cached_link() {
        // Pretend a prior poll_link reported the link as up, then
        // entering loopback must reset that flag — BMSR.LINK_STATUS is
        // not trustworthy in loopback on some LAN8720A silicon.
        let mut mdio = MockMdio::new(vec![]);
        let mut phy = PhyLan87xx::new(1);
        phy.link_up = true;
        phy.set_loopback(&mut mdio, true).unwrap();
        assert!(
            !phy.link_up,
            "set_loopback(true) must invalidate the cached link-up flag"
        );
    }

    #[test]
    fn set_loopback_on_mdio_error_propagates() {
        // Fail on the first call (the BMCR write).
        let mut mdio = MockMdio::with_failure(vec![], 0);
        let mut phy = PhyLan87xx::new(1);
        let err = phy.set_loopback(&mut mdio, true).unwrap_err();
        match err {
            PhyError::Mdio(MockError) => {}
            _ => panic!("expected Mdio error, got {:?}", err),
        }
    }

    #[test]
    fn set_loopback_off_mdio_error_propagates_on_read() {
        // Fail on the first call (the BMCR read).
        let mut mdio = MockMdio::with_failure(vec![], 0);
        let mut phy = PhyLan87xx::new(1);
        let err = phy.set_loopback(&mut mdio, false).unwrap_err();
        match err {
            PhyError::Mdio(MockError) => {}
            _ => panic!("expected Mdio error, got {:?}", err),
        }
    }

    #[test]
    fn set_loopback_off_mdio_error_propagates_on_write() {
        // Read succeeds (returns 0x3000), then write (call 1) fails.
        // Guards against an accidental drop of `?` on the write half of
        // the RMW path in future refactors.
        let mut mdio = MockMdio::with_failure(vec![0x3000], 1);
        let mut phy = PhyLan87xx::new(1);
        let err = phy.set_loopback(&mut mdio, false).unwrap_err();
        match err {
            PhyError::Mdio(MockError) => {}
            _ => panic!("expected Mdio error, got {:?}", err),
        }
    }

    #[test]
    fn wrapped_set_loopback_delegates_to_inner() {
        // `PhyLan87xxWithReset` forwards `set_loopback` to the inner
        // `PhyLan87xx`. Verify the forwarding actually reaches the
        // inner implementation by observing the BMCR overwrite the
        // ON-path is guaranteed to issue.
        struct NoopPin;
        impl embedded_hal::digital::ErrorType for NoopPin {
            type Error = core::convert::Infallible;
        }
        impl embedded_hal::digital::OutputPin for NoopPin {
            fn set_low(&mut self) -> Result<(), Self::Error> {
                Ok(())
            }
            fn set_high(&mut self) -> Result<(), Self::Error> {
                Ok(())
            }
        }

        let mut mdio = MockMdio::new(vec![]);
        let mut phy = PhyLan87xxWithReset::new(1, NoopPin);
        phy.set_loopback(&mut mdio, true).unwrap();

        assert_eq!(
            mdio.writes.len(),
            1,
            "delegated set_loopback(true) must issue exactly one BMCR write"
        );
        let (addr, reg, value) = mdio.writes[0];
        assert_eq!(addr, 1, "phy address must propagate");
        assert_eq!(reg, ieee802_3::regs::BMCR, "must target BMCR");
        assert_eq!(
            value,
            bmcr::LOOPBACK | bmcr::SPEED_100 | bmcr::DUPLEX_FULL,
            "full BMCR overwrite value must match inner contract"
        );
    }

    // ── parse_pscsr tests ──────────────────────────────────────────────

    #[test]
    fn parse_pscsr_all_modes() {
        assert_eq!(
            PhyLan87xx::parse_pscsr(regs::pscsr::SPEED_10_HD),
            Some(LinkStatus::new(Speed::Mbps10, Duplex::Half))
        );
        assert_eq!(
            PhyLan87xx::parse_pscsr(regs::pscsr::SPEED_10_FD),
            Some(LinkStatus::new(Speed::Mbps10, Duplex::Full))
        );
        assert_eq!(
            PhyLan87xx::parse_pscsr(regs::pscsr::SPEED_100_HD),
            Some(LinkStatus::new(Speed::Mbps100, Duplex::Half))
        );
        assert_eq!(
            PhyLan87xx::parse_pscsr(regs::pscsr::SPEED_100_FD),
            Some(LinkStatus::new(Speed::Mbps100, Duplex::Full))
        );
        // Unknown value (0b000 << 2 = 0x00)
        assert_eq!(PhyLan87xx::parse_pscsr(0x0000), None);
    }

    #[test]
    fn parse_pscsr_ignores_other_bits() {
        // Set noise bits outside the speed/duplex field
        let val = regs::pscsr::SPEED_100_FD | regs::pscsr::AUTODONE | 0x0003 | 0x8000;
        assert_eq!(
            PhyLan87xx::parse_pscsr(val),
            Some(LinkStatus::new(Speed::Mbps100, Duplex::Full))
        );
    }
}
