# Changelog

All notable changes to `eth-mdio-phy` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.1] - 2026-05-03

### Documentation

- Rewrite `README.md` as an integration guide: installation,
  compatibility table, full worked example for both `PhyDriver` and
  `MdioBus` implementations, GPIO bit-bang Clause 22 frame breakdown,
  `MockMdioBus` recipe for tests.
- Expand crate-level rustdoc in `src/lib.rs` to mirror the README on
  docs.rs and to call out the object-safety constraint of `PhyDriver`.
- Add `documentation`, `readme`, and `[package.metadata.docs.rs]` to
  `Cargo.toml`.

## [0.1.0] - 2026-04-29

### Added

- Initial public release.
- `MdioBus` trait — wire-level Clause 22 read/write.
- `PhyDriver` trait — `init`, `poll_link`, `capabilities`,
  generically over `MdioBus`.
- `ieee802_3` module — Clause 22 register addresses, bit constants,
  helpers (`soft_reset`, `enable_auto_negotiation`, `is_link_up`,
  `read_phy_id`, `read_capabilities`, `force_link`).
- Shared types: `Speed`, `Duplex`, `LinkStatus`, `PhyCapabilities`,
  `PhyError`.

[0.1.1]: https://github.com/jethub-iot/eth-phy-rs/compare/eth-mdio-phy-v0.1.0...eth-mdio-phy-v0.1.1
[0.1.0]: https://github.com/jethub-iot/eth-phy-rs/releases/tag/eth-mdio-phy-v0.1.0
