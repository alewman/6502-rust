//! Loads, stores, register transfers and register increments (`_loads.py`).
//!
//! A store writes the register to the effective address and reads nothing
//! there; a one-byte instruction spends its second cycle reading the byte
//! after its opcode and discarding it (HM p. A-2).

use crate::core::{Bus, Mos6502, N, NZ, Z};

impl<T: Bus> Mos6502<T> {
    /// LDA -- M -> A (PM p. B-17).
    pub(crate) fn op_lda(&mut self, address: u16) {
        let a = self.read(address);
        self.a = a;
        self.p = (self.p & !(N | Z)) | NZ[a as usize];
    }

    /// LDX -- M -> X (PM p. B-18).
    pub(crate) fn op_ldx(&mut self, address: u16) {
        let x = self.read(address);
        self.x = x;
        self.p = (self.p & !(N | Z)) | NZ[x as usize];
    }

    /// LDY -- M -> Y (PM p. B-18).
    pub(crate) fn op_ldy(&mut self, address: u16) {
        let y = self.read(address);
        self.y = y;
        self.p = (self.p & !(N | Z)) | NZ[y as usize];
    }

    /// STA -- A -> M (PM p. B-26).
    pub(crate) fn op_sta(&mut self, address: u16) {
        self.write(address, self.a);
    }

    /// STX -- X -> M (PM p. B-26).
    pub(crate) fn op_stx(&mut self, address: u16) {
        self.write(address, self.x);
    }

    /// STY -- Y -> M (PM p. B-27).
    pub(crate) fn op_sty(&mut self, address: u16) {
        self.write(address, self.y);
    }

    /// TAX -- A -> X (PM p. B-27).
    pub(crate) fn op_tax(&mut self) {
        self.read_pc();
        self.x = self.a;
        self.p = (self.p & !(N | Z)) | NZ[self.a as usize];
    }

    /// TAY -- A -> Y (PM p. B-28).
    pub(crate) fn op_tay(&mut self) {
        self.read_pc();
        self.y = self.a;
        self.p = (self.p & !(N | Z)) | NZ[self.a as usize];
    }

    /// TXA -- X -> A (PM p. B-29).
    pub(crate) fn op_txa(&mut self) {
        self.read_pc();
        self.a = self.x;
        self.p = (self.p & !(N | Z)) | NZ[self.x as usize];
    }

    /// TYA -- Y -> A (PM p. B-28).
    pub(crate) fn op_tya(&mut self) {
        self.read_pc();
        self.a = self.y;
        self.p = (self.p & !(N | Z)) | NZ[self.y as usize];
    }

    /// TSX -- S -> X (PM p. B-29).
    pub(crate) fn op_tsx(&mut self) {
        self.read_pc();
        self.x = self.s;
        self.p = (self.p & !(N | Z)) | NZ[self.s as usize];
    }

    /// TXS -- X -> S, no flags (PM p. B-29).
    pub(crate) fn op_txs(&mut self) {
        self.read_pc();
        self.s = self.x;
    }

    /// INX -- X + 1 -> X (PM p. B-15).
    pub(crate) fn op_inx(&mut self) {
        self.read_pc();
        let x = self.x.wrapping_add(1);
        self.x = x;
        self.p = (self.p & !(N | Z)) | NZ[x as usize];
    }

    /// INY -- Y + 1 -> Y (PM p. B-16).
    pub(crate) fn op_iny(&mut self) {
        self.read_pc();
        let y = self.y.wrapping_add(1);
        self.y = y;
        self.p = (self.p & !(N | Z)) | NZ[y as usize];
    }

    /// DEX -- X - 1 -> X (PM p. B-13).
    pub(crate) fn op_dex(&mut self) {
        self.read_pc();
        let x = self.x.wrapping_sub(1);
        self.x = x;
        self.p = (self.p & !(N | Z)) | NZ[x as usize];
    }

    /// DEY -- Y - 1 -> Y (PM p. B-14).
    pub(crate) fn op_dey(&mut self) {
        self.read_pc();
        let y = self.y.wrapping_sub(1);
        self.y = y;
        self.p = (self.p & !(N | Z)) | NZ[y as usize];
    }
}
