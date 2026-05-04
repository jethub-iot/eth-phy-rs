# Changelog

All notable changes to `eth-phy-lan87xx` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.1] - 2026-05-03

### Documentation

- Rewrite `README.md` as an integration guide: installation,
  compatibility table, embassy-net pointer to the `esp-emac` example,
  forced-link recipe via `eth_mdio_phy::ieee802_3::force_link`,
  troubleshooting checklist (cold-boot ANAR quirk, MDIO bus failures,
  strap-pin pitfalls).
- Replace the `ignore`'d snippet in `src/lib.rs` with a compilable
  `no_run` doctest and expand crate-level rustdoc to mirror the
  README on docs.rs.
- Add `documentation`, `readme`, and `[package.metadata.docs.rs]` to
  `Cargo.toml`.
- Bump dependency on `eth-mdio-phy` to `0.1.1`.

## [0.1.0] - 2026-04-29

### Added

- Initial public release.
- `PhyLan87xx` driver implementing `eth_mdio_phy::PhyDriver`.
- LAN8710A / LAN8720A / LAN8740A / LAN8741A / LAN8742A chip ID
  recognition via `PHYIDR1/2`.
- `init`: soft reset → chip ID check → EDPD disable → explicit
  `ANAR=0x01E1` → auto-neg restart.
- `poll_link`: BMSR link bit + PSCSR speed/duplex decode (auto-neg
  path) or `BMCR.SPEED_100`/`DUPLEX_FULL` (forced-link path).

[0.1.1]: https://github.com/jethub-iot/eth-phy-rs/compare/eth-phy-lan87xx-v0.1.0...eth-phy-lan87xx-v0.1.1
[0.1.0]: https://github.com/jethub-iot/eth-phy-rs/releases/tag/eth-phy-lan87xx-v0.1.0
