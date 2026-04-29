# eth-phy

[![License: GPL-2.0-or-later OR Apache-2.0](https://img.shields.io/badge/license-GPL--2.0--or--later%20OR%20Apache--2.0-blue.svg)](#license)

Modular Ethernet PHY stack for bare-metal Rust (`#![no_std]`, no
heap, no platform dependency).

The workspace is split into two crates so a board-bringup author can
pick exactly the abstraction they need:

* **[`eth-mdio-phy`](crates/eth-mdio-phy/)** — `MdioBus` and
  `PhyDriver` traits, IEEE 802.3 Clause 22 register helpers, shared
  `Speed` / `Duplex` / `LinkStatus` / `PhyCapabilities` types.
* **[`eth-phy-lan87xx`](crates/eth-phy-lan87xx/)** — driver for the
  Microchip LAN87xx family
  (LAN8710A / LAN8720A / LAN8740A / LAN8741A / LAN8742A).

The MAC side is intentionally not part of this stack — provide any
`MdioBus` impl and you can drive the PHY from any Ethernet MAC
(ESP32 EMAC, STM32 ETH, custom FPGA SMI, mocks for unit tests, ...).

## Why a separate trait crate

`eth-mdio-phy` lets you write **one** PHY driver and reuse it across
MACs. It also gives PHY-agnostic crates (DHCP smoke tests,
auto-negotiation diagnostics, link-state watchdogs) a stable API to
target.

The trait set is deliberately small:

```rust no_run
pub trait MdioBus {
    type Error;
    fn read(&mut self, phy_addr: u8, reg: u8) -> Result<u16, Self::Error>;
    fn write(&mut self, phy_addr: u8, reg: u8, value: u16)
        -> Result<(), Self::Error>;
}

pub trait PhyDriver {
    type Bus: MdioBus;
    fn init(&mut self, bus: &mut Self::Bus) -> Result<(), PhyError<...>>;
    fn poll_link(&mut self, bus: &mut Self::Bus)
        -> Result<Option<LinkStatus>, PhyError<...>>;
    fn capabilities(&self) -> PhyCapabilities;
}
```

That's enough to bring a typical RMII PHY up: probe the chip ID,
soft-reset, programme `ANAR`, kick auto-neg, then poll for link.

## Quick start

The pairing with [`esp-emac`](https://github.com/jethub-iot/esp-emac-rs)
on an ESP32 + LAN8720A board (PHY on MDIO addr 1):

```rust no_run
use esp_emac::mdio::EspMdio;
use eth_phy_lan87xx::PhyLan87xx;
use eth_mdio_phy::PhyDriver;

# fn example<E>() -> Result<(), eth_mdio_phy::PhyError<E>> {
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
