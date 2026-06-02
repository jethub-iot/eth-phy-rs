# Changelog

All notable changes to `eth-mdio-phy` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

*No pending changes.*

## [0.3.0] - 2026-05-22

### Breaking

- Public shape now mirrors `esp_hal::ethernet::mac`: the
  `LinkStatus { speed, duplex }` struct is renamed to
  `LinkState { up: bool, speed, duplex }` (an explicit `up` flag
  replaces the `Option<LinkStatus>` carrier signalling), and the
  `Speed` variants `Mbps10` / `Mbps100` are renamed to `_10M` /
  `_100M`. Adopting the upstream names makes a `PhyDriver` →
  `esp_hal::ethernet::phy::Phy` adapter a near-identity transform
  rather than a field-by-field rebuild.
  - `LinkStatus::new(speed, duplex)` → `LinkState::up(speed, duplex)`
    (`up = true`) or `LinkState::down()` (`up = false`).
  - When `up == false` the `speed` / `duplex` fields are
    unspecified — callers must check `up` before acting on them.
- `PhyDriver::poll_link` signature changed from
  `Result<Option<LinkStatus>, PhyError<M::Error>>` to
  `Result<LinkState, PhyError<M::Error>>`. Match a polled
  `LinkState` against the `up` flag instead of `Option::is_some()`.
- `ieee802_3::force_link` signature changed from
  `force_link(mdio, phy_addr, LinkStatus)` to
  `force_link(mdio, phy_addr, Speed, Duplex)`. `LinkState` is
  intended for *querying* link state — building a synthetic
  `LinkState { up: true, .. }` just to thread it through a force
  helper was awkward, so the helper now takes the two scalars
  directly.

## [0.2.0] - 2026-05-06

### Breaking

- `Speed` and `Duplex` are now `#[non_exhaustive]`. Callers that
  pattern-match on these enums must add a wildcard arm. Done so
  that adding a future variant (e.g. `Speed::Mbps1000` for a
  gigabit-capable PHY) can ship as a non-breaking minor release.
  `LinkStatus` and `PhyCapabilities` keep their plain layout —
  their public fields are already an open extension surface, and
  `Default` impls plus `..Default::default()` give downstream
  code a forward-compatible build path.

### Added

- `PhyError::UnsupportedPackage { strap: u32 }` for drivers whose
  family covers more than one silicon package and which have to
  discriminate the concrete part at runtime (e.g. LAN867x via
  `STRAP_CTRL0.PKGTYP`). Distinct from `UnsupportedChip { id }`,
  which now strictly means "PHY ID does not match the family".
  Lands as a non-breaking variant addition because `PhyError` is
  already `#[non_exhaustive]`.

### Fixed

- `ieee802_3::force_link` now also clears `BMCR.AN_RESTART` from
  the read-modify-write mask. Per IEEE 802.3 Clause 22.2.4.1.5
  the bit is self-clearing on hardware that initiates negotiation,
  but with `AN_ENABLE = 0` the negotiation FSM never runs, so the
  bit can sit at 1 indefinitely. A subsequent
  `enable_auto_negotiation` would then read `AN_RESTART = 1` from
  BMCR and re-write it as part of its own RMW, accidentally
  short-cutting its restart cycle. Adds the regression test
  `force_link_clears_an_restart`.
- Five clippy errors under `cargo clippy --all-targets -D warnings`
  in test code (3× `bool_assert_comparison` in `ieee802_3.rs`,
  2× `clone_on_copy` in `types.rs`). Test-only — nothing in the
  published `.crate` changes.

### Documentation

- Trait-crate seam clarifications for the v0.2 publication round:
  `MdioBus::Error` rustdoc recommends `Debug + Clone` (and
  `defmt::Format` under the feature) so `PhyError<E>` can be
  printed; `PhyDriver::phy_id` rustdoc generalises away from
  the implicit Clause-22 100BASE-TX framing (for 10BASE-T1S
  drivers the same registers carry a different field layout);
  `ieee802_3::read_capabilities` documents the `pause: false`
  hardcode (BMSR has no PAUSE-capability bit per Clause 22.2.4.2).
- Drop the WIP shields.io badge from README and remove the
  `### Pre-publication` section. Anchor the License-badge link
  from the relative `../../LICENSE-APACHE` (which only resolves
  inside the workspace tree, not on the rendered crates.io page)
  to the same-page `#license` anchor.

### Internal

- Ship `LICENSE-APACHE` and `LICENSE-GPL` in the crate directory
  so downstream packaging tooling (Debian, FreeBSD, openSUSE)
  finds the license texts in the expected location.

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

[Unreleased]: https://github.com/jethub-iot/eth-phy-rs/compare/eth-mdio-phy-v0.3.0...HEAD
[0.3.0]: https://github.com/jethub-iot/eth-phy-rs/compare/eth-mdio-phy-v0.2.0...eth-mdio-phy-v0.3.0
[0.2.0]: https://github.com/jethub-iot/eth-phy-rs/compare/eth-mdio-phy-v0.1.1...eth-mdio-phy-v0.2.0
[0.1.1]: https://github.com/jethub-iot/eth-phy-rs/compare/eth-mdio-phy-v0.1.0...eth-mdio-phy-v0.1.1
[0.1.0]: https://github.com/jethub-iot/eth-phy-rs/releases/tag/eth-mdio-phy-v0.1.0
