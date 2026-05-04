# eth-phy

[![License: GPL-2.0-or-later OR Apache-2.0](https://img.shields.io/badge/license-GPL--2.0--or--later%20OR%20Apache--2.0-blue.svg)](#license)
[![Status: WIP](https://img.shields.io/badge/status-WIP-orange.svg)](#pre-publication)

Modular Ethernet PHY stack for bare-metal Rust (`#![no_std]`, no
heap, no platform dependency).

The workspace is split into two crates so a board-bringup author can
pick exactly the abstraction they need:

| Crate | Purpose | Crates.io | docs.rs |
| --- | --- | --- | --- |
| [`eth-mdio-phy`](crates/eth-mdio-phy/) | `MdioBus` and `PhyDriver` traits, IEEE 802.3 Clause 22 helpers, shared `Speed`/`Duplex`/`LinkStatus`/`PhyCapabilities` types | [![Crates.io](https://img.shields.io/crates/v/eth-mdio-phy.svg)](https://crates.io/crates/eth-mdio-phy) | [![docs](https://docs.rs/eth-mdio-phy/badge.svg)](https://docs.rs/eth-mdio-phy) |
| [`eth-phy-lan87xx`](crates/eth-phy-lan87xx/) | Driver for the Microchip LAN87xx family (LAN8710A / LAN8720A / LAN8740A / LAN8741A / LAN8742A) | [![Crates.io](https://img.shields.io/crates/v/eth-phy-lan87xx.svg)](https://crates.io/crates/eth-phy-lan87xx) | [![docs](https://docs.rs/eth-phy-lan87xx/badge.svg)](https://docs.rs/eth-phy-lan87xx) |

The MAC side is intentionally not part of this stack — provide any
`MdioBus` impl and you can drive the PHY from any Ethernet MAC
(ESP32 EMAC, STM32 ETH, custom FPGA SMI, mocks for unit tests, ...).

## Installation

### Driving a LAN87xx-family PHY

```toml
[dependencies]
eth-mdio-phy    = "0.1"
eth-phy-lan87xx = "0.1"
```

For ESP32 the MAC implementation is in
[`esp-emac`](https://crates.io/crates/esp-emac):

```toml
esp-emac = { version = "0.1", features = ["esp-hal", "mdio-phy", "embassy-net"] }
```

### Implementing your own PHY driver

```toml
[dependencies]
eth-mdio-phy = "0.1"
```

Then `impl PhyDriver for MyPhy { ... }` against the trait — see
[`eth-mdio-phy/README.md`](crates/eth-mdio-phy/) for a worked
example, including how to write a GPIO bit-bang `MdioBus` for
boards that don't have an SMI peripheral.

### Pre-publication

> The crates **are not yet on crates.io** (this is the WIP badge).
> Until they ship, vendor them through git submodules and reference
> via local `path` instead of `version`:
>
> ```sh
> git submodule add https://github.com/jethub-iot/eth-phy-rs.git   vendor/eth-phy
> git submodule add https://github.com/jethub-iot/esp-emac-rs.git  vendor/esp-emac   # if you also need the ESP32 MAC
> git submodule update --init --recursive
> ```
>
> ```toml
> [dependencies]
> eth-mdio-phy    = { path = "vendor/eth-phy/crates/eth-mdio-phy" }
> eth-phy-lan87xx = { path = "vendor/eth-phy/crates/eth-phy-lan87xx" }
> esp-emac        = { path = "vendor/esp-emac", features = ["esp-hal", "mdio-phy", "embassy-net"] }
> ```
>
> Once published the snippet collapses to plain `version = "0.1"` deps.

## Why a separate trait crate

`eth-mdio-phy` lets you write **one** PHY driver and reuse it across
MACs. It also gives PHY-agnostic crates (DHCP smoke tests,
auto-negotiation diagnostics, link-state watchdogs) a stable API to
target.

The trait set is deliberately small:

```rust no_run
pub trait MdioBus {
    type Error;
    fn read(&mut self, phy_addr: u8, reg_addr: u8) -> Result<u16, Self::Error>;
    fn write(&mut self, phy_addr: u8, reg_addr: u8, value: u16)
        -> Result<(), Self::Error>;
}

pub trait PhyDriver {
    fn phy_addr(&self) -> u8;
    fn phy_id<M: MdioBus>(&self, mdio: &mut M)
        -> Result<u32, PhyError<M::Error>>;
    fn init<M: MdioBus>(&mut self, mdio: &mut M)
        -> Result<(), PhyError<M::Error>>;
    fn poll_link<M: MdioBus>(&mut self, mdio: &mut M)
        -> Result<Option<LinkStatus>, PhyError<M::Error>>;
    fn capabilities<M: MdioBus>(&self, mdio: &mut M)
        -> Result<PhyCapabilities, PhyError<M::Error>>;
}
```

That's enough to bring a typical RMII PHY up: probe the chip ID,
soft-reset, programme `ANAR`, kick auto-neg, then poll for link.
Each `PhyDriver` method takes the bus generically (`<M: MdioBus>`)
rather than via an associated type, so the same driver instance can
be reused with different bus implementations across a session
(MAC bring-up vs. diagnostic mock vs. logging passthrough).

The trade-off: `PhyDriver` is **not** object-safe — `dyn PhyDriver`
is a compile error because of those generic methods. If you need
polymorphic storage (a switch driver, a multi-PHY watchdog) keep the
PHY drivers as concrete types behind an `enum`, or write a thin
object-safe wrapper that fixes a single `MdioBus` implementation.

## Quick start

The pairing with [`esp-emac`](https://crates.io/crates/esp-emac) on an
ESP32 + LAN8720A board (PHY on MDIO addr 1):

```rust no_run
use esp_emac::mdio::EspMdio;
use eth_phy_lan87xx::PhyLan87xx;
use eth_mdio_phy::{MdioBus, PhyDriver};

# fn example<E>() -> Result<(), eth_mdio_phy::PhyError<E>>
# where EspMdio: MdioBus<Error = E> {
let mut mdio = EspMdio::new();
let mut phy = PhyLan87xx::new(1);

phy.init(&mut mdio)?;
loop {
    if let Some(status) = phy.poll_link(&mut mdio)? {
        // status.speed: Mbps10 / Mbps100
        // status.duplex: Half / Full
        break;
    }
}
# Ok(())
# }
```

For the full embassy-net + DHCP example see
[`esp-emac/examples/embassy_net_lan8720a.rs`](https://github.com/jethub-iot/esp-emac-rs/blob/main/examples/embassy_net_lan8720a.rs).

## Hardware verified on

* JXD-PM3-80-E1ETH and JXD-R6-E1ETH-LCD (LAN8720A on RMII, RMII clock
  driven from ESP32 APLL output via GPIO17). Cold boot, soft reset
  and USB power-cycle all bring the link up reliably; see the
  [crate-level driver docs](crates/eth-phy-lan87xx/) for the
  `ANAR=0x01E1` quirk that affects cold-boot auto-neg recovery.

## License

Licensed under either of:

* GNU General Public License, Version 2.0 or later
  ([LICENSE-GPL](LICENSE-GPL))
* Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))

at your option.

Copyright (c) Viacheslav Bocharov (v at baodeep dot com) and JetHome (r).
