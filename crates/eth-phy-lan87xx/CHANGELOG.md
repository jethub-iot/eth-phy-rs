# Changelog

All notable changes to `eth-phy-lan87xx` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] - 2026-05-06

### Breaking

- Bump dependency on `eth-mdio-phy` to `^0.2`. The trait crate's
  `Speed` and `Duplex` enums became `#[non_exhaustive]` in that
  release, which propagates to anyone matching on the
  `LinkStatus`-typed return of `poll_link`. Add a wildcard arm
  to existing `match` blocks.

### Documentation

- Drop the WIP shields.io badge from README and remove the
  `### Pre-publication` section now that 0.2.0 is on crates.io.
  Anchor the License-badge link from the relative
  `../../LICENSE-APACHE` to the same-page `#license` anchor —
  the relative form does not resolve on the rendered crates.io
  page.

### Internal

- Move the LAN8720A datasheet (PDF + cleaned markdown extract +
  figure images) out of `doc/` and into the parent project's
  reference tree. The previous layout was untracked locally but
  cargo `--allow-dirty` packaged the lot anyway, adding 13 MB of
  pdf2md staging artefacts to the published `.crate`. Removing
  the directory entirely from the crate folder closes the door
  at the file-system level.
- Ship `LICENSE-APACHE` and `LICENSE-GPL` in the crate directory.

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

[0.2.0]: https://github.com/jethub-iot/eth-phy-rs/compare/eth-phy-lan87xx-v0.1.1...eth-phy-lan87xx-v0.2.0
[0.1.1]: https://github.com/jethub-iot/eth-phy-rs/compare/eth-phy-lan87xx-v0.1.0...eth-phy-lan87xx-v0.1.1
[0.1.0]: https://github.com/jethub-iot/eth-phy-rs/releases/tag/eth-phy-lan87xx-v0.1.0
