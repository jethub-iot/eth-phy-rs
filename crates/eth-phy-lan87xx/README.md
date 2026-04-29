# eth-phy-lan87xx

[![License: GPL-2.0-or-later OR Apache-2.0](https://img.shields.io/badge/license-GPL--2.0--or--later%20OR%20Apache--2.0-blue.svg)](../../LICENSE-APACHE)

`#![no_std]` MDIO driver for the Microchip LAN87xx family of 10/100
Ethernet PHYs:

* LAN8710A
* LAN8720A
* LAN8740A
* LAN8741A
* LAN8742A

Implements [`eth_mdio_phy::PhyDriver`](../eth-mdio-phy/), so any MAC
that exposes [`eth_mdio_phy::MdioBus`](../eth-mdio-phy/) can drive the
chip — typical case is the ESP32 built-in EMAC SMI controller via
[`esp_emac::mdio::EspMdio`](https://github.com/jethub-iot/esp-emac-rs).

## Quick start

```rust no_run
use eth_phy_lan87xx::PhyLan87xx;
use eth_mdio_phy::{MdioBus, PhyDriver};

# fn example<B: MdioBus>(mdio: &mut B) -> Result<(), eth_mdio_phy::PhyError<B::Error>> {
// PHY is at MDIO address 1 on this board.
let mut phy = PhyLan87xx::new(1);

// Probe + soft reset + ANAR + kick auto-neg.
phy.init(mdio)?;

// Poll until link is up. Returns `Some(LinkStatus)` when the link
// comes up; `None` while still negotiating.
loop {
    if let Some(status) = phy.poll_link(mdio)? {
        // status.speed: Mbps10 / Mbps100
        // status.duplex: Half / Full
        break;
    }
}
# Ok(())
# }
```

## What `init` does

1. Reads `PHYIDR1/2` and rejects anything that doesn't decode to a
   known LAN87xx OUI / model.
2. Issues `BMCR.RESET` (soft reset) and waits for the bit to
   self-clear.
3. **Writes `ANAR = 0x01E1`** explicitly — both the
   10BASE-T / 10BASE-T-FD / 100BASE-TX / 100BASE-TX-FD ability bits
   and the IEEE 802.3 selector field. This step is crucial; see the
   gotcha below.
4. Sets `BMCR.AN_ENABLE | BMCR.AN_RESTART` to kick auto-negotiation.

## What `poll_link` does

Reads `BMSR` for the link bit, then if up, decodes the negotiated
speed / duplex from the LAN87xx-specific PSCSR register (faster and
more reliable than reading `ANLPAR` because it reflects the actual
result rather than the partner's advertisement).

## Hardware gotcha — cold-boot ANAR

On a **cold boot** of the LAN8720A (and confirmed on its siblings),
issuing `BMCR.RESET` does NOT restore `ANAR` to the default
`0x01E1`. Whatever the PHY has in non-volatile state survives, and
that's typically a subset of the full 10/100 + half/full advertisement.
Auto-neg then converges on the partial subset and the link comes up
at the lowest common denominator — or, worse, succeeds on a speed
that the MAC isn't ready for, so unicast RX wedges and only
broadcast / multicast survive.

The driver writes `ANAR = 0x01E1` explicitly between the soft reset
and `AN_RESTART` to side-step this. If you reimplement this PHY init
elsewhere, do the same.

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
