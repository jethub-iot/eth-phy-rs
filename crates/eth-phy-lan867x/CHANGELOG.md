# Changelog

All notable changes to `eth-phy-lan867x` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
