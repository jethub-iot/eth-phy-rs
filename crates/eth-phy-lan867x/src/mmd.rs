// SPDX-License-Identifier: GPL-2.0-or-later OR Apache-2.0
// Copyright (c) Viacheslav Bocharov <v@baodeep.com> and JetHome (r)

//! IEEE 802.3 Annex 22D MMD register indirection.
//!
//! The LAN867x supports the Clause 22 MDIO frame format only.
//! Clause-45 (MMD-managed) registers are reached via a 4-step
//! indirection through `MMDCTRL` (Clause 22 register 0x0D) and
//! `MMDAD` (Clause 22 register 0x0E):
//!
//! 1. `MMDCTRL = (FNCTN_ADDR | DEVAD)` — selects MMD device, function = address-write.
//! 2. `MMDAD = <register address>`     — programs the target MMD register address.
//! 3. `MMDCTRL = (FNCTN_DATA | DEVAD)` — switches to data-access mode.
//! 4. read or write `MMDAD`            — accesses the selected MMD register.
//!
//! Helpers in this module hide that 4-step dance behind plain
//! `mmd_read` / `mmd_write` calls.

use eth_mdio_phy::MdioBus;

use crate::regs;

/// Read a 16-bit MMD register at `(devad, reg)`.
pub(crate) fn mmd_read<M: MdioBus>(
    mdio: &mut M,
    phy_addr: u8,
    devad: u8,
    reg: u16,
) -> Result<u16, M::Error> {
    let devad_field = u16::from(devad) & regs::MMDCTRL_DEVAD_MASK;
    mdio.write(
        phy_addr,
        regs::REG_MMDCTRL,
        regs::MMDCTRL_FNCTN_ADDR | devad_field,
    )?;
    mdio.write(phy_addr, regs::REG_MMDAD, reg)?;
    mdio.write(
        phy_addr,
        regs::REG_MMDCTRL,
        regs::MMDCTRL_FNCTN_DATA | devad_field,
    )?;
    mdio.read(phy_addr, regs::REG_MMDAD)
}

/// Write a 16-bit value to the MMD register at `(devad, reg)`.
pub(crate) fn mmd_write<M: MdioBus>(
    mdio: &mut M,
    phy_addr: u8,
    devad: u8,
    reg: u16,
    value: u16,
) -> Result<(), M::Error> {
    let devad_field = u16::from(devad) & regs::MMDCTRL_DEVAD_MASK;
    mdio.write(
        phy_addr,
        regs::REG_MMDCTRL,
        regs::MMDCTRL_FNCTN_ADDR | devad_field,
    )?;
    mdio.write(phy_addr, regs::REG_MMDAD, reg)?;
    mdio.write(
        phy_addr,
        regs::REG_MMDCTRL,
        regs::MMDCTRL_FNCTN_DATA | devad_field,
    )?;
    mdio.write(phy_addr, regs::REG_MMDAD, value)
}

/// Read-modify-write helper for an MMD register: clears `clear_mask` and
/// then sets `set_mask` (set takes precedence).
pub(crate) fn mmd_rmw<M: MdioBus>(
    mdio: &mut M,
    phy_addr: u8,
    devad: u8,
    reg: u16,
    clear_mask: u16,
    set_mask: u16,
) -> Result<(), M::Error> {
    let current = mmd_read(mdio, phy_addr, devad, reg)?;
    let next = (current & !clear_mask) | set_mask;
    mmd_write(mdio, phy_addr, devad, reg, next)
}

#[cfg(test)]
mod tests {
    extern crate alloc;

    use super::*;
    use alloc::vec;
    use alloc::vec::Vec;

    #[derive(Debug, PartialEq)]
    struct MockError;

    struct MockMdio {
        reads: Vec<u16>,
        read_idx: usize,
        ops: Vec<Op>,
    }

    #[derive(Debug, PartialEq)]
    enum Op {
        Read { phy: u8, reg: u8 },
        Write { phy: u8, reg: u8, value: u16 },
    }

    impl MockMdio {
        fn new(reads: Vec<u16>) -> Self {
            Self {
                reads,
                read_idx: 0,
                ops: Vec::new(),
            }
        }
    }

    impl MdioBus for MockMdio {
        type Error = MockError;

        fn read(&mut self, phy_addr: u8, reg_addr: u8) -> Result<u16, MockError> {
            self.ops.push(Op::Read {
                phy: phy_addr,
                reg: reg_addr,
            });
            let v = *self
                .reads
                .get(self.read_idx)
                .expect("MockMdio: reads vector exhausted");
            self.read_idx += 1;
            Ok(v)
        }

        fn write(&mut self, phy_addr: u8, reg_addr: u8, value: u16) -> Result<(), MockError> {
            self.ops.push(Op::Write {
                phy: phy_addr,
                reg: reg_addr,
                value,
            });
            Ok(())
        }
    }

    #[test]
    fn mmd_read_drives_correct_indirection_sequence() {
        // Target: MMD 31, register 0xCA00 (MIDVER) — expect 0x0A10.
        let mut mdio = MockMdio::new(vec![0x0A10]);
        let v = mmd_read(&mut mdio, 0, regs::MMD_VS2, regs::MMD_REG_MIDVER).unwrap();
        assert_eq!(v, 0x0A10);

        // Exact ordering matters — the chip latches MMDAD only after
        // the FNCTN_ADDR write has committed the register address.
        assert_eq!(
            mdio.ops,
            vec![
                Op::Write {
                    phy: 0,
                    reg: regs::REG_MMDCTRL,
                    value: regs::MMDCTRL_FNCTN_ADDR | u16::from(regs::MMD_VS2),
                },
                Op::Write {
                    phy: 0,
                    reg: regs::REG_MMDAD,
                    value: regs::MMD_REG_MIDVER,
                },
                Op::Write {
                    phy: 0,
                    reg: regs::REG_MMDCTRL,
                    value: regs::MMDCTRL_FNCTN_DATA | u16::from(regs::MMD_VS2),
                },
                Op::Read {
                    phy: 0,
                    reg: regs::REG_MMDAD,
                },
            ]
        );
    }

    #[test]
    fn mmd_write_drives_correct_indirection_sequence() {
        // Target: MMD 31 PLCA_CTRL1 = 0x0801 (NCNT=8, ID=1, follower).
        let mut mdio = MockMdio::new(vec![]);
        mmd_write(
            &mut mdio,
            3,
            regs::MMD_VS2,
            regs::MMD_REG_PLCA_CTRL1,
            0x0801,
        )
        .unwrap();

        assert_eq!(
            mdio.ops,
            vec![
                Op::Write {
                    phy: 3,
                    reg: regs::REG_MMDCTRL,
                    value: regs::MMDCTRL_FNCTN_ADDR | u16::from(regs::MMD_VS2),
                },
                Op::Write {
                    phy: 3,
                    reg: regs::REG_MMDAD,
                    value: regs::MMD_REG_PLCA_CTRL1,
                },
                Op::Write {
                    phy: 3,
                    reg: regs::REG_MMDCTRL,
                    value: regs::MMDCTRL_FNCTN_DATA | u16::from(regs::MMD_VS2),
                },
                Op::Write {
                    phy: 3,
                    reg: regs::REG_MMDAD,
                    value: 0x0801,
                },
            ]
        );
    }

    #[test]
    fn mmd_rmw_preserves_unrelated_bits() {
        // T1SPMACTL pre-existing: TXD=1, MDE=0; we set MDE without touching TXD.
        let pre = regs::T1SPMACTL_TXD;
        let mut mdio = MockMdio::new(vec![pre]);
        mmd_rmw(
            &mut mdio,
            0,
            regs::MMD_PMA_PMD,
            regs::MMD_REG_T1SPMACTL,
            0,
            regs::T1SPMACTL_MDE,
        )
        .unwrap();

        // Find the data-write to MMDAD (the *last* write to MMDAD)
        let last_mmdad_write = mdio
            .ops
            .iter()
            .rev()
            .find_map(|op| match op {
                Op::Write {
                    reg: regs::REG_MMDAD,
                    value,
                    ..
                } => Some(*value),
                _ => None,
            })
            .expect("expected an MMDAD data write");

        assert_eq!(
            last_mmdad_write,
            regs::T1SPMACTL_TXD | regs::T1SPMACTL_MDE,
            "RMW must preserve the previously-set TXD bit"
        );
    }

    #[test]
    fn mmd_rmw_clears_then_sets() {
        // Pre: PLCA_CTRL0 = EN | RST. Clear RST, set EN — net: EN only.
        let pre = regs::PLCA_CTRL0_EN | regs::PLCA_CTRL0_RST;
        let mut mdio = MockMdio::new(vec![pre]);
        mmd_rmw(
            &mut mdio,
            0,
            regs::MMD_VS2,
            regs::MMD_REG_PLCA_CTRL0,
            regs::PLCA_CTRL0_RST,
            regs::PLCA_CTRL0_EN,
        )
        .unwrap();

        let last_mmdad_write = mdio
            .ops
            .iter()
            .rev()
            .find_map(|op| match op {
                Op::Write {
                    reg: regs::REG_MMDAD,
                    value,
                    ..
                } => Some(*value),
                _ => None,
            })
            .unwrap();

        assert_eq!(last_mmdad_write, regs::PLCA_CTRL0_EN);
    }
}
