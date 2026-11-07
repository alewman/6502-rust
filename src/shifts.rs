//! Shifts, rotates, and memory increment and decrement: the read-modify-writes
//! (`_shifts.py`).
//!
//! A read-modify-write reads its operand, writes it back unchanged while the
//! ALU works, then writes the result: two writes to the same address. MOS
//! lists the extra cycle with the operand's address on the bus (HM section
//! A.4, pp. A-7 to A-9, cycle T3 of each table); NMS documents that it is a
//! write of the unmodified byte ("Dummy writes", p. 88). Hardware that counts
//! writes or triggers on them sees both, so the core makes both.

use crate::core::{Bus, Mos6502, C, N, NZ, Z};

pub(crate) fn asl(value: u8, p: u8) -> (u8, u8) {
    let result = value << 1;
    (
        result,
        (p & !(N | Z | C)) | NZ[result as usize] | (value >> 7),
    )
}

pub(crate) fn lsr(value: u8, p: u8) -> (u8, u8) {
    let result = value >> 1;
    (
        result,
        (p & !(N | Z | C)) | NZ[result as usize] | (value & C),
    )
}

pub(crate) fn rol(value: u8, p: u8) -> (u8, u8) {
    let result = (value << 1) | (p & C);
    (
        result,
        (p & !(N | Z | C)) | NZ[result as usize] | (value >> 7),
    )
}

pub(crate) fn ror(value: u8, p: u8) -> (u8, u8) {
    let result = (value >> 1) | ((p & C) << 7);
    (
        result,
        (p & !(N | Z | C)) | NZ[result as usize] | (value & C),
    )
}

impl<T: Bus> Mos6502<T> {
    /// The read and the first write of a read-modify-write: the unmodified
    /// byte goes back first (NMS p. 88; HM p. A-8, cycle T3).
    pub(crate) fn modify(&mut self, address: u16) -> u8 {
        let value = self.read(address);
        self.write(address, value);
        value
    }

    /// ASL -- C <- M7..M0 <- 0 (PM p. B-4).
    pub(crate) fn op_asl(&mut self, address: u16) {
        let value;
        (value, self.p) = asl(self.modify(address), self.p);
        self.write(address, value);
    }

    /// LSR -- 0 -> M7..M0 -> C (PM p. B-19).
    pub(crate) fn op_lsr(&mut self, address: u16) {
        let value;
        (value, self.p) = lsr(self.modify(address), self.p);
        self.write(address, value);
    }

    /// ROL -- C <- M7..M0 <- C (PM p. B-22).
    pub(crate) fn op_rol(&mut self, address: u16) {
        let value;
        (value, self.p) = rol(self.modify(address), self.p);
        self.write(address, value);
    }

    /// ROR -- C -> M7..M0 -> C (PM p. B-23).
    pub(crate) fn op_ror(&mut self, address: u16) {
        let value;
        (value, self.p) = ror(self.modify(address), self.p);
        self.write(address, value);
    }

    /// INC -- M + 1 -> M (PM p. B-15).
    pub(crate) fn op_inc(&mut self, address: u16) {
        let value = self.modify(address).wrapping_add(1);
        self.p = (self.p & !(N | Z)) | NZ[value as usize];
        self.write(address, value);
    }

    /// DEC -- M - 1 -> M (PM p. B-13).
    pub(crate) fn op_dec(&mut self, address: u16) {
        let value = self.modify(address).wrapping_sub(1);
        self.p = (self.p & !(N | Z)) | NZ[value as usize];
        self.write(address, value);
    }

    /// ASL A -- C <- A7..A0 <- 0 (PM p. B-4).
    pub(crate) fn op_asl_a(&mut self) {
        self.read_pc();
        (self.a, self.p) = asl(self.a, self.p);
    }

    /// LSR A -- 0 -> A7..A0 -> C (PM p. B-19).
    pub(crate) fn op_lsr_a(&mut self) {
        self.read_pc();
        (self.a, self.p) = lsr(self.a, self.p);
    }

    /// ROL A -- C <- A7..A0 <- C (PM p. B-22).
    pub(crate) fn op_rol_a(&mut self) {
        self.read_pc();
        (self.a, self.p) = rol(self.a, self.p);
    }

    /// ROR A -- C -> A7..A0 -> C (PM p. B-23).
    pub(crate) fn op_ror_a(&mut self) {
        self.read_pc();
        (self.a, self.p) = ror(self.a, self.p);
    }
}
