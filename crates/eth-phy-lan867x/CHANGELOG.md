# Changelog

All notable changes to `eth-phy-lan867x` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- `init` now clears the driver's cached PLCA state at the start, so a
  re-initialisation after a previous `configure_plca` no longer leaves
  `poll_link` querying `PLCA_STS.PST` against a soft-reset chip whose
  PLCA is off (which would have made `poll_link` return `None`
  permanently).
- `configure_plca` now writes `PLCA_BURST` unconditionally rather than
  only when `burst_count > 0`. A re-configuration with `burst_count = 0`
  reliably clears the chip's `MAXBC`, undoing any prior burst-enabling
  call. Datasheet sec 5.4.18 specifies `MAXBC = 0` as the explicit
  "burst disabled" encoding.
- `PlcaConfig::burst_timer = 0` now actually behaves as the documented
  sentinel: `configure_plca` writes the chip default (`0x80`, 12.8 µs)
  for `BTMR` instead of literally `0` (which would make burst mode
  non-functional even when `MAXBC > 0`).

### Documentation

- Crate-level rustdoc and README clarify the single-owner contract:
  the driver assumes it is the sole writer to the PHY's registers.
  External writes to `PLCA_CTRL0.EN` between driver calls are not
  observed; call `init` to resync.
- `plca` module docs no longer claim its consumer methods land "in a
  follow-up commit" — they shipped in v0.1.0.

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
