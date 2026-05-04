# eth-phy-lan87xx

[![License: GPL-2.0-or-later OR Apache-2.0](https://img.shields.io/badge/license-GPL--2.0--or--later%20OR%20Apache--2.0-blue.svg)](../../LICENSE-APACHE)
[![Status: WIP](https://img.shields.io/badge/status-WIP-orange.svg)](#installation) — not yet on crates.io / docs.rs

`#![no_std]` MDIO driver for the Microchip LAN87xx family of 10/100
Ethernet PHYs:

* LAN8710A
* LAN8720A
* LAN8740A
* LAN8741A
* LAN8742A

Implements [`eth_mdio_phy::PhyDriver`](../eth-mdio-phy/), so any MAC
that exposes `eth_mdio_phy::MdioBus` can drive the chip — typical
case is the ESP32 built-in EMAC SMI controller via
[`esp_emac::mdio::EspMdio`](https://github.com/jethub-iot/esp-emac-rs).

---

## Installation

The crate is **not yet published to crates.io.** Add the parent
`eth-phy` repository as a git submodule and reference both crates
via local paths:

```sh
git submodule add https://github.com/jethub-iot/eth-phy-rs.git vendor/eth-phy
git submodule update --init --recursive
```

```toml
[dependencies]
eth-mdio-phy    = { path = "vendor/eth-phy/crates/eth-mdio-phy" }
eth-phy-lan87xx = { path = "vendor/eth-phy/crates/eth-phy-lan87xx" }
```

Once published, both become plain `version = "..."` deps.

| Feature | Default | Pulls in |
| --- | --- | --- |
| `defmt` | off | `defmt::Format` derives via `eth-mdio-phy/defmt` |

**MSRV: 1.75.** Pure `#![no_std]`. Works on any target — picking the
target is the MAC layer's problem, not this crate's.

## Compatibility

| Crate | Version |
| --- | --- |
| `eth-mdio-phy` | 0.1.x (sibling crate) |
| For ESP32: [`esp-emac`](https://github.com/jethub-iot/esp-emac-rs) | 0.1.x |

---

## Quick start

Driving a LAN8720A on an ESP32 board (PHY at MDIO addr 1):

```rust no_run
use esp_emac::mdio::EspMdio;
use eth_phy_lan87xx::PhyLan87xx;
use eth_mdio_phy::{MdioBus, PhyDriver};

# fn example<E>() -> Result<(), eth_mdio_phy::PhyError<E>>
# where EspMdio: MdioBus<Error = E> {
let mut mdio = EspMdio::new();
let mut phy = PhyLan87xx::new(1);

// Probe + soft reset + ANAR + kick auto-neg.
phy.init(&mut mdio)?;

// Poll until link is up. Returns `Some(LinkStatus)` when the link
// comes up; `None` while still negotiating.
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

## Boards with a PHY reset pin

If your board exposes a GPIO-driven PHY `nRST` line, use
`PhyLan87xxWithReset<P>` instead of `PhyLan87xx`. It wraps the same
driver and adds a `hardware_reset()` method that drives `nRST` low
for 2 ms, then deasserts and waits 25 ms before MDIO becomes
accessible (LAN8720A datasheet Table 4-2). Most JXD modules do
**not** route `nRST` to the MCU; for those use plain `PhyLan87xx`.

```rust no_run
use embedded_hal::{delay::DelayNs, digital::OutputPin};
use eth_mdio_phy::{MdioBus, PhyDriver, PhyError};
use eth_phy_lan87xx::PhyLan87xxWithReset;

# fn example<P, D, M>(reset: P, delay: &mut D, mdio: &mut M)
#     -> Result<(), MyError<P::Error, M::Error>>
# where
#     P: OutputPin,
#     D: DelayNs,
#     M: MdioBus,
# {
let mut phy = PhyLan87xxWithReset::new(/* MDIO addr */ 1, reset);
phy.hardware_reset(delay).map_err(MyError::Pin)?;
phy.init(mdio).map_err(MyError::Phy)?;
# Ok(())
# }
# enum MyError<P, M> { Pin(P), Phy(PhyError<M>) }
```

## Bypassing auto-negotiation

Auto-neg covers the common case. If a board needs a forced link (e.g.
a fixed-speed back-to-back connection) call
[`eth_mdio_phy::ieee802_3::force_link`](https://docs.rs/eth-mdio-phy/latest/eth_mdio_phy/ieee802_3/fn.force_link.html)
directly with the chosen `Speed` / `Duplex` after `PhyLan87xx::init`
returns — that helper clears `AN_ENABLE` and sets the `SPEED_100` /
`DUPLEX_FULL` bits in BMCR for you.

`poll_link` automatically detects the BMCR mode (auto-neg vs forced)
and decodes the link state appropriately.

---

## What `init` does

1. Issues `BMCR.RESET` (soft reset) and waits for the bit to
   self-clear.
2. Reads `PHYIDR1/2` and rejects anything that doesn't decode to a
   known LAN87xx OUI / model.
3. Disables Energy-Detect Power-Down by clearing `MCSR.EDPD_EN`.
   With EDPD on, the LAN87xx silently drops 10 Mbps frames during
   the wake-up window after auto-neg — turning it off keeps the RX
   path active at all times.
4. **Writes `ANAR = 0x01E1`** explicitly — both the
   10BASE-T / 10BASE-T-FD / 100BASE-TX / 100BASE-TX-FD ability bits
   and the IEEE 802.3 selector field. This step is crucial; see the
   troubleshooting note below.
5. Sets `BMCR.AN_ENABLE | BMCR.AN_RESTART` to kick auto-negotiation.

## What `poll_link` does

* Reads `BMSR` for the link bit.
* If the PHY is in auto-neg mode (`BMCR.AN_ENABLE = 1`), waits for
  `PSCSR.AUTODONE` then decodes the negotiated speed / duplex from
  the LAN87xx-specific PSCSR register (faster and more reliable than
  reading `ANLPAR` because it reflects the actual result rather than
  the partner's advertisement).
* If auto-neg is disabled (forced mode), decodes speed / duplex
  directly from `BMCR.SPEED_100` / `BMCR.DUPLEX_FULL`.

---

## Troubleshooting

### Link comes up but unicast RX is dead (cold boot only)

Symptoms: ARP requests get replies, but ICMP/TCP times out. Reproduces
on cold boot; goes away after a re-flash without power-cycle.

Root cause: on a **cold boot** of the LAN8720A (and confirmed on its
siblings), issuing `BMCR.RESET` does NOT restore `ANAR` to the default
`0x01E1`. Whatever the PHY has in non-volatile state survives, and
that's typically a subset of the full 10/100 + half/full advertisement.
Auto-neg then converges on the partial subset and the link comes up
at the lowest common denominator — or, worse, succeeds on a speed
that the MAC isn't ready for, so unicast RX wedges and only
broadcast / multicast survive.

This driver handles the case by writing `ANAR = 0x01E1` explicitly
between the soft reset and `AN_RESTART`. **If you reimplement this
PHY init elsewhere, do the same** — it is the single most common cold-
boot Ethernet failure on LAN87xx.

### `PhyError::UnsupportedChip { id: 0 }` on `init`

The PHY is on a different MDIO address than the one passed to `new()`.
Typical strap-pin variants put LAN8720A at addr 0 or 1. Try both. On
ESP32 modules, the strap-pin assignment depends on PCB-level pull-up
configuration — check the schematic.

### `PhyError::UnsupportedChip { id: 0xFFFF_FFFF }` on `init`

MDIO bus reads are floating high — typical signs:

* MDIO line not pulled up (LAN87xx datasheet requires 1.5 kΩ pull-up
  on MDIO).
* RMII reference clock is not running. The MDIO state machine inside
  the PHY needs the 25 MHz clock to be alive even though MDIO is
  electrically asynchronous; on LAN8720A, no REF_CLK = no MDIO ACK.
  Verify `Emac::init` (or your equivalent) brings up the clock before
  the first MDIO transaction.

### Link reports 10 Mbps despite a 100 Mbps switch

ANAR didn't get programmed correctly — see the cold-boot section
above. Other possibilities:

* MDIO writes aren't actually reaching the PHY (verify with a
  scope or a software MDIO trace).
* The PHY's `BMCR.SPEED_100` strap pin is tied low and overrides the
  software value at reset (LAN8720A datasheet table 7-1).

---

## Hardware verified on

* JXD-PM3-80-E1ETH and JXD-R6-E1ETH-LCD (LAN8720A on RMII; ESP32 APLL
  drives the 50 MHz reference into the PHY through GPIO17). Cold boot,
  soft reset and USB power-cycle all converge on link up
  100 Mbps full duplex within a few hundred milliseconds.

## License

Licensed under either of:

* GNU General Public License, Version 2.0 or later
* Apache License, Version 2.0

at your option.
