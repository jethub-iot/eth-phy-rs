# eth-phy-lan867x

[![License: GPL-2.0-or-later OR Apache-2.0](https://img.shields.io/badge/license-GPL--2.0--or--later%20OR%20Apache--2.0-blue.svg)](../../LICENSE-APACHE)
[![Crates.io](https://img.shields.io/crates/v/eth-phy-lan867x.svg)](https://crates.io/crates/eth-phy-lan867x)
[![Documentation](https://docs.rs/eth-phy-lan867x/badge.svg)](https://docs.rs/eth-phy-lan867x)
[![Status: WIP](https://img.shields.io/badge/status-WIP-orange.svg)](#pre-publication)

`#![no_std]` MDIO driver for the Microchip LAN867x family of
**10BASE-T1S** Ethernet PHYs (IEEE 802.3cg-2019 Clause 147):

* **LAN8670** — 32-VQFN, MII or RMII
* **LAN8671** — 24-VQFN, RMII only
* **LAN8672** — 36-VQFN, MII only

10BASE-T1S is single-pair, half-duplex, multidrop Ethernet — quite
different from the point-to-point 10/100BASE-T flavours covered by
[`eth-phy-lan87xx`](https://docs.rs/eth-phy-lan87xx). If you're
plugging into a switch, you want lan87xx; if you're building a
sensor / actuator backbone with up to 8 nodes on a shared single
twisted pair, you want this crate.

Implements [`eth_mdio_phy::PhyDriver`](https://docs.rs/eth-mdio-phy)
so any MAC that exposes `eth_mdio_phy::MdioBus` can drive the chip.
On JetHome boards the MAC is the ESP32 built-in EMAC SMI controller
via [`esp_emac::mdio::EspMdio`](https://docs.rs/esp-emac).

---

## Installation

```toml
[dependencies]
eth-mdio-phy    = "0.1"
eth-phy-lan867x = "0.1"
```

| Feature | Default | Pulls in |
| --- | --- | --- |
| `defmt` | off | `defmt::Format` derives via `eth-mdio-phy/defmt` |

**MSRV: 1.75.** Pure `#![no_std]`, no allocations. Works on any target.

### Pre-publication

> The crates **are not yet on crates.io** (this is the WIP badge).
> Until they ship, vendor the parent `eth-phy-rs` repository via a
> git submodule and reference both crates by `path`:
>
> ```sh
> git submodule add https://github.com/jethub-iot/eth-phy-rs.git vendor/eth-phy
> ```
>
> ```toml
> eth-mdio-phy    = { path = "vendor/eth-phy/crates/eth-mdio-phy" }
> eth-phy-lan867x = { path = "vendor/eth-phy/crates/eth-phy-lan867x" }
> ```

## Compatibility

| Crate | Version |
| --- | --- |
| [`eth-mdio-phy`](https://crates.io/crates/eth-mdio-phy) | 0.1.x |
| For ESP32: [`esp-emac`](https://crates.io/crates/esp-emac) | 0.1.x |

---

## Quick start

CSMA/CD multidrop bus (no PLCA), PHY at MDIO addr 0:

```rust no_run
use eth_phy_lan867x::PhyLan867x;
use eth_mdio_phy::{MdioBus, PhyDriver};

# fn example<M: MdioBus>(mdio: &mut M)
# -> Result<(), eth_mdio_phy::PhyError<M::Error>>
# {
let mut phy = PhyLan867x::new(0);

// Probe + soft reset + RESETC handshake + PHY-ID + MIDVER + MDE=1.
phy.init(mdio)?;

// On a CSMA/CD bus there is no per-link-partner signal — the bus is
// "always there". poll_link returns Some(LinkStatus { Mbps10, Half })
// once init has succeeded.
let status = phy.poll_link(mdio)?.expect("CSMA/CD always reports linked");
assert_eq!(status.speed, eth_mdio_phy::Speed::Mbps10);
assert_eq!(status.duplex, eth_mdio_phy::Duplex::Half);
# Ok(())
# }
```

## With PLCA (recommended for > 2-node buses)

PLCA (IEEE 802.3 Clause 148) gives the bus collision-free TDMA-style
arbitration on top of CSMA/CD. One node is the **coordinator**
(`node_id = 0`); the rest are **followers** (`1..=0xFE`).

```rust no_run
use eth_phy_lan867x::{PhyLan867x, PlcaConfig};
use eth_mdio_phy::{MdioBus, PhyDriver};

# fn coordinator<M: MdioBus>(mdio: &mut M)
# -> Result<(), eth_phy_lan867x::PlcaError<M::Error>>
# {
let mut phy = PhyLan867x::new(0);
phy.init(mdio).map_err(|_| /* convert to PlcaError */ todo!())?;

phy.configure_plca(mdio, &PlcaConfig {
    node_id:    0,    // coordinator
    node_count: 8,    // up to 8 nodes on the segment
    burst_count: 0,   // single-frame TXOPs
    burst_timer: 0,
})?;
// poll_link will now consult PLCA_STS.PST and report linked when
// BEACONs are flowing.
# Ok(())
# }
```

For a **follower**:

```rust no_run
# use eth_phy_lan867x::{PhyLan867x, PlcaConfig};
# use eth_mdio_phy::MdioBus;
# fn follower<M: MdioBus>(phy: &mut PhyLan867x, mdio: &mut M)
# -> Result<(), eth_phy_lan867x::PlcaError<M::Error>>
# {
phy.configure_plca(mdio, &PlcaConfig {
    node_id:    3,    // unique on the segment
    node_count: 8,    // must match the coordinator's NCNT
    burst_count: 0,
    burst_timer: 0,
})?;
# Ok(())
# }
```

## Boards with a PHY reset pin

If your board exposes a GPIO-driven PHY `RESET_N` line — and JetHome
JXD-CPU-E1T1S does, on ESP32 GPIO17 — use `PhyLan867xWithReset<P>`:

```rust no_run
use embedded_hal::{delay::DelayNs, digital::OutputPin};
use eth_mdio_phy::{MdioBus, PhyDriver, PhyError};
use eth_phy_lan867x::PhyLan867xWithReset;

# fn example<P, D, M>(reset: P, delay: &mut D, mdio: &mut M)
#     -> Result<(), MyError<P::Error, M::Error>>
# where
#     P: OutputPin,
#     D: DelayNs,
#     M: MdioBus,
# {
let mut phy = PhyLan867xWithReset::new(/* MDIO addr */ 0, reset);
phy.hardware_reset(delay).map_err(MyError::Pin)?;
phy.init(mdio).map_err(MyError::Phy)?;
# Ok(())
# }
# enum MyError<P, M> { Pin(P), Phy(PhyError<M>) }
```

`hardware_reset` drives `RESET_N` low for 10 ms then waits 25 ms
after release before MDIO is allowed — conservative timings that
also match `PhyLan87xxWithReset`.

---

## What `init` does

1. **Soft reset** via `BMCR.SW_RESET` (self-clearing, bounded poll).
2. **Reset-complete handshake**: poll `STS2.RESETC` in MMD-31. The
   chip holds `IRQ_N` low after every reset until the host reads
   `STS2`; reading it clears `RESETC` and releases the line. Without
   this step, subsequent register writes may not take effect.
3. **PHY identity check** via `PHY_ID0/1`. Mask `0xFFFF_FFF0` against
   `0x0000_C560` (Microchip OUI 00800Fh + MODEL `010110b`); silicon
   revision is intentionally allowed to vary.
4. **Package discrimination** from `STRAP_CTRL0.PKGTYP` ⇒
   `Chip::Lan8670 / 8671 / 8672`. Available via `chip()`.
5. **MIDVER sanity probe**: MMD-31 `0xCA00` must read `0x0A10` (OPEN
   Alliance register-map identifier, version 1.0). Confirms the MMD
   indirection is functional and the silicon implements the standard
   T1S register layout.
6. **Multidrop enable**: RMW `T1SPMACTL.MDE = 1` in MMD-1. Required
   for any > 2-node bus; chip default is point-to-point.

## What `poll_link` does

* **PLCA off** (CSMA/CD): no MDIO traffic. Returns
  `Some(LinkStatus { Mbps10, Half })` — the bus is "always there".
* **PLCA on**: reads MMD-31 `PLCA_STS.PST`. Returns linked when set
  (BEACONs are being TX'd as coordinator or RX'd as follower);
  returns `None` while the bus is still synchronising.

## What `BMSR.LINK_STATUS` does NOT do

It is hard-wired `1` on this chip. Calling
`eth_mdio_phy::ieee802_3::is_link_up` will always return `true`,
regardless of bus state. **Don't.** Use `poll_link` instead.

---

## Troubleshooting

### `PhyError::ResetTimeout` on init

Two possible sources:

* `BMCR.SW_RESET` never self-clears (~500-attempt window). Likely
  the MDIO bus is silently failing the write. Verify `MDIO` /
  `MDC` wiring and pull-ups.
* `STS2.RESETC` never asserts. The chip never finished its internal
  POR sequence — usually means the 50 MHz `REFCLKIN` (RMII) or
  25 MHz crystal (MII) is not actually clocking. On JXD-CPU-E1T1S
  the LAN8671 has its own oscillator and exports the 50 MHz to ESP32
  via GPIO0; if ESP32's EMAC is configured wrong (e.g. expecting
  internal APLL output instead of external clock-in) the PHY runs
  but the MAC can't talk to it.

### `PhyError::UnsupportedChip { id: 0 }` on init

The PHY is on a different MDIO address than the one passed to
`new()`, OR the MDIO clock isn't running. LAN8671 latches its SMI
address from `PHYAD[3:0]` at hardware reset; the JetHome JXD-CPU-
E1T1S pulls all four pins low ⇒ addr = 0. Other boards will differ;
check the schematic and/or read `STRAP_CTRL0.SMIADR`.

### `PhyError::UnsupportedChip { id: 0xFFFF_FFFF }` on init

MDIO bus reads are floating high — typical signs:

* No external pull-up on `MDIO` (Microchip recommends 10 kΩ).
* The PHY is held in `RESET_N`. Use `PhyLan867xWithReset` and call
  `hardware_reset` first.

### PLCA configured but `poll_link` always returns `None`

Most common cause: every node thinks it is the coordinator
(`node_id = 0`). Datasheet sec 4.9.2 covers the diagnostic flags
(`STS1.UNEXPB` is set on coordinator collisions). Other causes:

* `node_count` < actual node count: some followers' TXOPs never
  come up, but the coordinator still BEACONs and PST will
  oscillate.
* `node_id >= node_count` on a follower: this is rejected by
  `configure_plca` with `PlcaError::InvalidConfig`.

### Need PLCA diagnostic counters

`STS1` decoding, `TOCNT` / `BCNCNT` readers — deferred to v0.2.
For v0.1.x, read MMD-31 registers `0x18` / `0x24-0x27` directly
through your `MdioBus` if you need them in the meantime.

---

## Hardware verified on

The driver compiles, runs unit tests against a `MockMdio`, and
matches the datasheet register-by-register against
[DS60001573C](https://www.microchip.com/) (silicon revision 2 =
product revision B1). Hardware bring-up on JXD-R6-E1T1S is in
progress and will land in a follow-up release.

## License

Licensed under either of:

* GNU General Public License, Version 2.0 or later
* Apache License, Version 2.0

at your option.
