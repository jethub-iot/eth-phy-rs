# eth-mdio-phy

[![License: GPL-2.0-or-later OR Apache-2.0](https://img.shields.io/badge/license-GPL--2.0--or--later%20OR%20Apache--2.0-blue.svg)](../../LICENSE-APACHE)

Trait crate that decouples MDIO bus implementations from the PHYs
they talk to. `#![no_std]`, no allocations, no platform dependency.

## What's exposed

* **`MdioBus`** — the wire-level read/write trait. Any controller that
  can issue MDIO Clause 22 transactions (ESP32 EMAC SMI, STM32 ETH
  MDIO, FPGA SMI peripheral, a GPIO bit-bang implementation, a mock
  for tests) implements this trait.
* **`PhyDriver`** — what every PHY driver in this stack provides:
  `init`, `poll_link`, `capabilities`. PHY-agnostic code (a DHCP test,
  a link-state watchdog, an `embassy-net` driver) can target this
  trait instead of a specific chip.
* **`ieee802_3`** — Clause 22 standard register addresses (`BMCR`,
  `BMSR`, `ANAR`, `ANLPAR`, `PHYIDR1/2`) and the bit constants
  inside them, plus convenience helpers `soft_reset`,
  `enable_auto_negotiation`, `is_link_up`, `read_phy_id`,
  `read_capabilities`, `force_link`. Reuse these in any
  chip-specific driver instead of re-deriving the bit numbers.
* **Shared types** — `Speed { Mbps10, Mbps100 }`,
  `Duplex { Half, Full }`, `LinkStatus { speed, duplex }`,
  `PhyCapabilities` (chip identification + advertised abilities).

## Usage

A PHY driver is just a `struct + impl PhyDriver`. Each method takes
the bus generically — there is no associated `Bus` type, so the same
driver instance can talk to a real `EspMdio` and a `MockMdioBus`
within one session.

```rust no_run
use eth_mdio_phy::{
    ieee802_3, LinkStatus, MdioBus, PhyCapabilities, PhyDriver, PhyError,
};

pub struct MyPhy { addr: u8 }

impl PhyDriver for MyPhy {
    fn phy_addr(&self) -> u8 {
        self.addr
    }

    fn phy_id<M: MdioBus>(&self, mdio: &mut M)
        -> Result<u32, PhyError<M::Error>>
    {
        ieee802_3::read_phy_id(mdio, self.addr).map_err(PhyError::Mdio)
    }

    fn init<M: MdioBus>(&mut self, mdio: &mut M)
        -> Result<(), PhyError<M::Error>>
    {
        // soft_reset, set ANAR, kick auto-neg, ...
        # Ok(())
    }

    fn poll_link<M: MdioBus>(&mut self, mdio: &mut M)
        -> Result<Option<LinkStatus>, PhyError<M::Error>>
    {
        // read BMSR, return Some(LinkStatus { .. }) when link comes up
        # Ok(None)
    }

    fn capabilities<M: MdioBus>(&self, mdio: &mut M)
        -> Result<PhyCapabilities, PhyError<M::Error>>
    {
        ieee802_3::read_capabilities(mdio, self.addr)
            .map_err(PhyError::Mdio)
    }
}
```

Application code then drives the PHY through the trait without
caring which chip is on the board:

```rust no_run
# fn doc<M: eth_mdio_phy::MdioBus, P: eth_mdio_phy::PhyDriver>
# (mdio: &mut M, phy: &mut P) -> Result<(), eth_mdio_phy::PhyError<M::Error>>
# {
phy.init(mdio)?;
while phy.poll_link(mdio)?.is_none() { /* idle */ }
# Ok(())
# }
```

## Why split `MdioBus` and `PhyDriver`

Three concrete reasons we hit on real hardware:

1. **Cross-MAC reuse.** A LAN87xx driver written against `MdioBus`
   works equally well behind the ESP32 EMAC SMI controller, an STM32
   ETH MDIO peripheral, or a host-side bit-bang implementation used
   by tests.
2. **Faster bring-up.** Auto-negotiation diagnostics, link-state
   watchdogs, and DHCP smoke tests can target `PhyDriver` and run
   against any concrete PHY without recompilation.
3. **Testability.** A `MockMdioBus` in `dev-dependencies` lets the
   PHY driver be unit-tested with deterministic register state — no
   QEMU, no hardware-in-the-loop fixture needed for the wire-level
   logic.

## Relation to the rest of the stack

* MAC implementations provide a `MdioBus`. For ESP32 see
  [`esp_emac::mdio::EspMdio`](https://github.com/jethub-iot/esp-emac-rs).
* PHY drivers implement `PhyDriver`. For LAN87xx see
  [`eth-phy-lan87xx`](../eth-phy-lan87xx/).
* Higher-level Ethernet stacks (e.g. `embassy-net` adaptors) compose
  the two and stay PHY-agnostic.

## License

Licensed under either of:

* GNU General Public License, Version 2.0 or later
* Apache License, Version 2.0

at your option.
