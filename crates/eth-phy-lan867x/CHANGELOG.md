# Changelog

All notable changes to `eth-phy-lan867x` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

*No pending changes.*

## [0.2.0] - 2026-05-22

### ⚠ Hardware verification status

The CSMA/CD multidrop path (init + 10BASE-T1S half-duplex link
report) and the full PLCA API surface (`configure_plca`,
`disable_plca`, `plca_status`, MMD indirection helpers) compile and
pass host-side `MockMdio` unit tests. **No bench run against real
silicon has happened in this release window.** Plan for empirical
validation before relying on this driver in production — register-
by-register agreement with the datasheet is a necessary but not
sufficient condition for correctness against a physical LAN867x.
See the matching warning at the top of `README.md`.

### Breaking

- Bump dependency on `eth-mdio-phy` to `^0.3`. The trait crate now
  mirrors `esp_hal::ethernet::mac`: `LinkStatus { speed, duplex }`
  becomes `LinkState { up: bool, speed, duplex }` (signalling moves
  from `Option<LinkStatus>` to the explicit `up` flag), and
  `Speed::Mbps10`/`Mbps100` are renamed to `Speed::_10M`/`_100M`.
- `PhyLan867x::poll_link` (and the `PhyLan867xWithReset` delegate)
  now returns `Result<LinkState, PhyError<M::Error>>` instead of
  `Result<Option<LinkStatus>, PhyError<M::Error>>`. Callers that
  pattern-matched on `Some`/`None` should match on `LinkState.up`
  instead. Behaviour is preserved: pre-init and PLCA-on-PST-clear
  paths return `LinkState::down()`, the CSMA/CD and PST-set paths
  return `LinkState::up(Speed::_10M, Duplex::Half)`.

### Added

- `defmt::warn!` logging (gated on the `defmt` feature) before
  `init()` returns `PhyError::ResetTimeout` (both the BMCR.SW_RESET
  and `STS2.RESETC` cases), `PhyError::UnsupportedChip` (both the
  PHY-ID family mismatch and the MIDVER sentinel mismatch), and
  `PhyError::UnsupportedPackage`. Captures PHY address plus the
  discriminating register read so adapters that collapse the rich
  `PhyError` set down to a narrower error type still leave behind a
  diagnosable trail.

## [0.1.0] - 2026-05-06

First public release. Bundled with the rest of the
`eth-mdio-phy` / `eth-phy-lan87xx` / `esp-emac` v0.2.0 publication
round.

### Fixed (vs the unreleased pre-cuts)

- `init` clears the driver's cached PLCA state at the start, so a
  re-initialisation after a previous `configure_plca` no longer leaves
  `poll_link` querying `PLCA_STS.PST` against a soft-reset chip whose
  PLCA is off (which would have made `poll_link` return `None`
  permanently).
- `configure_plca` writes `PLCA_BURST` unconditionally rather than
  only when `burst_count > 0`. A re-configuration with `burst_count = 0`
  reliably clears the chip's `MAXBC`, undoing any prior burst-enabling
  call. Datasheet sec 5.4.18 specifies `MAXBC = 0` as the explicit
  "burst disabled" encoding.
- `PlcaConfig::burst_timer = 0` now actually behaves as the documented
  sentinel: `configure_plca` writes the chip default (`0x80`, 12.8 µs)
  for `BTMR` instead of literally `0` (which would make burst mode
  non-functional even when `MAXBC > 0`).
- `init` step 4 (PKGTYP discrimination) returns the new
  `PhyError::UnsupportedPackage { strap }` variant instead of
  `UnsupportedChip { id }` when the package strap is unrecognised —
  the PHY ID had matched correctly, only the chip-package strap is
  out of range, and reporting `UnsupportedChip` was misleading.

### Documentation

- Crate-level rustdoc and README clarify the single-owner contract:
  the driver assumes it is the sole writer to the PHY's registers.
  External writes to `PLCA_CTRL0.EN` between driver calls are not
  observed; call `init` to resync.
- `configure_plca` rustdoc spells out the non-transactional failure
  semantics: the three sequential MDIO writes (CTRL1, BURST,
  CTRL0.EN-RMW) are not atomic, and an MDIO bus error after one of
  them succeeds leaves the chip and the driver-side `plca_id` cache
  in inconsistent states. Recovery is `configure_plca` retry or
  `init()`. A 0.2.0-track architectural plan for transactional
  semantics lives in the parent project's
  `docs/plans/eth-phy-lan867x-plca.md`.
- `PlcaConfig::burst_timer` rustdoc reframes the field: BTMR is
  always written, the chip itself ignores it when `MAXBC = 0`. The
  `0` sentinel meaning "use the chip default" is explicit.
- `plca` module docs no longer claim its consumer methods land "in a
  follow-up commit" — they ship in this release.

### Internal

- Replace the digit-leading keyword `10base-t1s` (which crates.io
  rejects on upload — keywords must start with an ASCII letter)
  with `t1s` and `eth-t1s`.
- Drop the WIP shields.io badge from README, remove the
  `### Pre-publication` section, and anchor the License badge link
  on `#license` rather than the relative `../../LICENSE-APACHE`
  (which does not resolve on the rendered crates.io page).
- Ship `LICENSE-APACHE` and `LICENSE-GPL` in the crate directory.

### Added

- `PhyLan867x` driver implementing `eth_mdio_phy::PhyDriver` for the
  Microchip LAN8670 / LAN8671 / LAN8672 family of 10BASE-T1S Ethernet
  PHYs (IEEE 802.3cg-2019 Clause 147).
- `Chip` enum with runtime discrimination of the concrete package
  (`Lan8670` / `Lan8671` / `Lan8672`) from `STRAP_CTRL0.PKGTYP`.
- `init`: soft reset → `STS2.RESETC` handshake → PHY-ID family
  verification → package discrimination → MIDVER sanity probe →
  `T1SPMACTL.MDE = 1` (multidrop enable).
- `poll_link`: gates on `PLCA_STS.PST` when PLCA is configured;
  returns "always linked" with `Speed::Mbps10` / `Duplex::Half`
  on the CSMA/CD path.
- IEEE Annex 22D MMDCTRL/MMDAD indirection helpers (kept private to
  this crate for v0.1.0; promotion into `eth-mdio-phy` deferred until
  a second MMD-using PHY driver lands).
- `PlcaConfig` / `PlcaStatus` / `PlcaError` types and
  `configure_plca` / `disable_plca` / `plca_status` methods. Coordinator
  and follower (with optional burst mode) supported. Validation rejects
  the silicon `0xFF` sentinel and follower IDs ≥ `node_count`.
- `PhyLan867xWithReset<P>` wrapper for boards (incl. JetHome
  JXD-CPU-E1T1S on ESP32 GPIO17) that route the PHY `RESET_N` to a
  MAC-driven GPIO. `hardware_reset` drives the pin low for 10 ms then
  waits 25 ms post-release before MDIO is touched, matching the
  `eth-phy-lan87xx` wrapper.
- `defmt` feature: `defmt::Format` derives via `eth-mdio-phy/defmt`.

Reference: Microchip DS60001573C (silicon revision 2 = product
revision B1).

[Unreleased]: https://github.com/jethub-iot/eth-phy-rs/compare/eth-phy-lan867x-v0.2.0...HEAD
[0.2.0]: https://github.com/jethub-iot/eth-phy-rs/compare/eth-phy-lan867x-v0.1.0...eth-phy-lan867x-v0.2.0
[0.1.0]: https://github.com/jethub-iot/eth-phy-rs/releases/tag/eth-phy-lan867x-v0.1.0
