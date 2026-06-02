# eth-mdio-phy

[![License: GPL-2.0-or-later OR Apache-2.0](https://img.shields.io/badge/license-GPL--2.0--or--later%20OR%20Apache--2.0-blue.svg)](#license)
[![Crates.io](https://img.shields.io/crates/v/eth-mdio-phy.svg)](https://crates.io/crates/eth-mdio-phy)
[![Documentation](https://docs.rs/eth-mdio-phy/badge.svg)](https://docs.rs/eth-mdio-phy)

Trait crate that decouples MDIO bus implementations from the PHYs
they talk to. `#![no_std]`, no allocations, no platform dependency.

---

## Installation

```toml
[dependencies]
eth-mdio-phy = "0.3"
```

| Feature | Default | Pulls in |
| --- | --- | --- |
| `defmt` | off | `defmt::Format` derives on `LinkState`, `PhyCapabilities`, `PhyError` |

**MSRV: 1.75.** Pure `#![no_std]` — works on any target.

> **Pre-1.0 SemVer note.** Cargo's caret on `^0.2` will *not* pick up
> `0.3.x` — both digits are treated as the major axis below 1.0.
> Consumers tracking this crate must bump the minor explicitly when
> a new minor release lands.

## Compatibility

This crate exposes only traits and constants — no transitive runtime
dependencies. It is consumed by:

| Consumer | Role |
| --- | --- |
| MAC implementations (e.g. [`esp-emac`](https://crates.io/crates/esp-emac), STM32 ETH peripherals, FPGA SMI controllers) | Provide `MdioBus` |
| PHY drivers (e.g. [`eth-phy-lan87xx`](https://crates.io/crates/eth-phy-lan87xx)) | Implement `PhyDriver` |
| Higher-level Ethernet stacks (e.g. `embassy-net` adaptors) | Compose the two and stay PHY-agnostic |

---

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
* **Shared types** — `Speed { _10M, _100M }` and `Duplex { Half, Full }`
  (variant names mirror `esp_hal::ethernet::phy::Speed` / `Duplex`);
  `LinkState { up: bool, speed: Speed, duplex: Duplex }` (the `up` flag
  is the canonical signal — link-down does NOT mean "missing speed/
  duplex", and link-up returns all three together); `PhyCapabilities`
  (chip identification + advertised abilities). All three are
  `#[non_exhaustive]` so future variants land as additive minor releases.

---

## Implementing your own PHY driver

A PHY driver is just a `struct + impl PhyDriver`. Each method takes
the bus generically — there is no associated `Bus` type, so the same
driver instance can talk to a real `EspMdio` and a `MockMdioBus`
within one session.

```rust no_run
use eth_mdio_phy::{
    ieee802_3, LinkState, MdioBus, PhyCapabilities, PhyDriver, PhyError,
};

pub struct MyPhy {
    addr: u8,
}

impl MyPhy {
    pub fn new(addr: u8) -> Self {
        Self { addr }
    }
}

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
        // 1. Soft reset. `soft_reset` returns Ok(true) when the
        //    BMCR.RESET bit self-cleared within `max_attempts`,
        //    Ok(false) on timeout. The caller decides how to map
        //    a timeout to a higher-level error.
        let cleared = ieee802_3::soft_reset(mdio, self.addr, /* max_attempts */ 100)
            .map_err(PhyError::Mdio)?;
        if !cleared {
            return Err(PhyError::ResetTimeout);
        }
        // 2. Programme ANAR — *write the value you actually want*,
        //    do not rely on hardware default coming back from reset.
        mdio.write(self.addr, ieee802_3::regs::ANAR, 0x01E1)
            .map_err(PhyError::Mdio)?;
        // 3. Kick auto-negotiation.
        ieee802_3::enable_auto_negotiation(mdio, self.addr)
            .map_err(PhyError::Mdio)
    }

    fn poll_link<M: MdioBus>(&mut self, mdio: &mut M)
        -> Result<LinkState, PhyError<M::Error>>
    {
        let up = ieee802_3::is_link_up(mdio, self.addr)
            .map_err(PhyError::Mdio)?;
        if !up { return Ok(LinkState::down()); }
        // Decode speed/duplex from a chip-specific register here, or
        // fall back to ANLPAR via ieee802_3 helpers. On link-up always
        // construct via `LinkState::up(speed, duplex)`.
        # Ok(LinkState::down())
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
while !phy.poll_link(mdio)?.up { /* idle */ }
# Ok(())
# }
```

---

## Implementing your own MdioBus

If your MAC has an SMI peripheral, just wrap its read/write registers
in a struct:

```rust no_run
use eth_mdio_phy::MdioBus;

pub struct MyMacMdio { /* registers / handles */ }

#[derive(Debug)]
pub enum MdioError { Timeout, BusError }

impl MdioBus for MyMacMdio {
    type Error = MdioError;

    fn read(&mut self, phy_addr: u8, reg_addr: u8)
        -> Result<u16, Self::Error>
    {
        // poke the SMI controller, return the 16-bit register value
        # Ok(0)
    }

    fn write(&mut self, phy_addr: u8, reg_addr: u8, value: u16)
        -> Result<(), Self::Error>
    {
        // poke the SMI controller; wait for completion
        # Ok(())
    }
}
```

If your board doesn't have a hardware SMI peripheral (some Cortex-M0
parts, FPGA bring-ups, exotic MCUs) a simple Clause 22 GPIO bit-bang
sits in well under 100 lines of `embedded-hal::digital::OutputPin`
manipulation. The Clause 22 frame is:

```text
preamble    (32 ones)
ST          (01)        — start of frame
OP          (10 = read, 01 = write)
PHY addr    (5 bits)
REG addr    (5 bits)
TA          (z0 read / 10 write)  — turnaround
DATA        (16 bits)
```

The `ieee802_3` register constants in this crate save you from
re-deriving the bit numbers; the wire-level shifting is the only
chip-specific code.

### Mocking for tests

Two patterns work well; pick whichever maps better to what your driver
actually does.

**Scripted-response mock** — pre-load the exact reads in order, record
writes for assertions, optionally inject a failure at a specific call
index. This is what [`eth-phy-lan87xx`](../eth-phy-lan87xx/) uses to
verify its init/poll sequences:

```rust no_run
use eth_mdio_phy::MdioBus;

#[derive(Debug, PartialEq)]
struct MockError;

struct MockMdio {
    reads: Vec<u16>,
    read_idx: usize,
    writes: Vec<(u8, u8, u16)>,
    fail_at: Option<usize>,
    call_count: usize,
}

impl MdioBus for MockMdio {
    type Error = MockError;

    fn read(&mut self, _phy_addr: u8, _reg_addr: u8)
        -> Result<u16, Self::Error>
    {
        if self.fail_at == Some(self.call_count) {
            self.call_count += 1;
            return Err(MockError);
        }
        self.call_count += 1;
        let val = self.reads[self.read_idx];
        self.read_idx += 1;
        Ok(val)
    }

    fn write(&mut self, phy_addr: u8, reg_addr: u8, value: u16)
        -> Result<(), Self::Error>
    {
        if self.fail_at == Some(self.call_count) {
            self.call_count += 1;
            return Err(MockError);
        }
        self.call_count += 1;
        self.writes.push((phy_addr, reg_addr, value));
        Ok(())
    }
}
```

**Map-based mock** — random-access registers, useful when the driver's
behaviour depends on arbitrary read order or many cross-register
side-effects:

```rust no_run
use eth_mdio_phy::MdioBus;
use std::collections::HashMap;
use core::convert::Infallible;

pub struct MockMdioMap {
    pub regs: HashMap<(u8, u8), u16>,
    pub writes: Vec<(u8, u8, u16)>,
}

impl MdioBus for MockMdioMap {
    type Error = Infallible;

    fn read(&mut self, phy_addr: u8, reg_addr: u8)
        -> Result<u16, Self::Error>
    {
        Ok(*self.regs.get(&(phy_addr, reg_addr)).unwrap_or(&0))
    }

    fn write(&mut self, phy_addr: u8, reg_addr: u8, value: u16)
        -> Result<(), Self::Error>
    {
        self.writes.push((phy_addr, reg_addr, value));
        self.regs.insert((phy_addr, reg_addr), value);
        Ok(())
    }
}
```

Both keep tests on the host, no hardware-in-the-loop fixture required.

---

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

---

## Relation to the rest of the stack

* MAC implementations provide a `MdioBus`. For ESP32 see
  [`esp_emac::mdio::EspMdio`](https://docs.rs/esp-emac).
* PHY drivers implement `PhyDriver`. For LAN87xx see
  [`eth-phy-lan87xx`](https://crates.io/crates/eth-phy-lan87xx).
* Higher-level Ethernet stacks (e.g. `embassy-net` adaptors) compose
  the two and stay PHY-agnostic.

## License

Licensed under either of:

* GNU General Public License, Version 2.0 or later
* Apache License, Version 2.0

at your option.
