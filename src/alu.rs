//! Arithmetic, logic, compare and bit-test instructions (`_alu.py`).
//!
//! Every handler here reads one operand from the effective address its
//! addressing mode computed, so each is the same shape: read, compute, set
//! flags. Binary ADC and SBC are PM's; the NMOS decimal-mode results,
//! including the flags the chip leaves after a BCD operation, are NMS's
//! "Unintended decimal mode" chapter (pp. 70-80), which the SingleStepTests
//! corpus and Klaus Dormann's decimal exerciser both check.

use crate::core::{Bus, Mos6502, C, N, NZ, V, Z};

/// A + M + C, binary or NMOS decimal; return (A, P).
///
/// `decimal` is P's D bit as the adder sees it: the handler passes
/// `p & decimal_mode`, which is 0 on a part whose decimal adjust is
/// disconnected. In decimal mode the NMOS 6502 sets Z from the binary sum and
/// N and V from the sum after the low digit is adjusted and before the high
/// digit is (NMS pp. 73-75), so only C is a decimal flag.
pub(crate) fn add(a: u8, value: u8, p: u8, decimal: u8) -> (u8, u8) {
    let (a, value) = (a as u32, value as u32);
    let carry = (p & C) as u32;
    let mut total = a + value + carry;
    let mut p = p & !(N | V | Z | C);
    if total & 0xFF == 0 {
        p |= Z;
    }
    if decimal != 0 {
        let mut low = (a & 0x0F) + (value & 0x0F) + carry;
        if low >= 0x0A {
            low = ((low + 0x06) & 0x0F) + 0x10;
        }
        total = (a & 0xF0) + (value & 0xF0) + low;
        p |= total as u8 & N;
        if !(a ^ value) & (a ^ total) & 0x80 != 0 {
            p |= V;
        }
        if total >= 0xA0 {
            total += 0x60;
        }
        if total >= 0x100 {
            p |= C;
        }
        return (total as u8, p);
    }
    let result = total & 0xFF;
    p |= result as u8 & N;
    if !(a ^ value) & (a ^ result) & 0x80 != 0 {
        p |= V;
    }
    if total > 0xFF {
        p |= C;
    }
    (result as u8, p)
}

/// A - M - (1 - C), binary or NMOS decimal; return (A, P).
///
/// `decimal` is as for [`add`]. Every flag comes from the binary difference,
/// in decimal mode too; only A is adjusted (NMS pp. 76-77).
pub(crate) fn subtract(a: u8, value: u8, p: u8, decimal: u8) -> (u8, u8) {
    let (a, value) = (a as i32, value as i32);
    let borrow = 1 - (p & C) as i32;
    let difference = a - value - borrow;
    let binary = (difference & 0xFF) as u8;
    let mut p = (p & !(N | V | Z | C)) | NZ[binary as usize];
    if (a ^ binary as i32) & (a ^ value) & 0x80 != 0 {
        p |= V;
    }
    if difference >= 0 {
        p |= C;
    }
    if decimal == 0 {
        return (binary, p);
    }
    let mut low = (a & 0x0F) - (value & 0x0F) - borrow;
    let mut high = (a >> 4) - (value >> 4);
    if low < 0 {
        low -= 6;
        high -= 1;
    }
    if high < 0 {
        high -= 6;
    }
    ((((high << 4) | (low & 0x0F)) & 0xFF) as u8, p)
}

impl<T: Bus> Mos6502<T> {
    /// ADC -- A + M + C -> A, C (PM p. B-3).
    pub(crate) fn op_adc(&mut self, address: u16) {
        let p = self.p;
        let value = self.read(address);
        (self.a, self.p) = add(self.a, value, p, p & self.decimal_mode);
    }

    /// SBC -- A - M - (1 - C) -> A (PM p. B-24).
    pub(crate) fn op_sbc(&mut self, address: u16) {
        let p = self.p;
        let value = self.read(address);
        (self.a, self.p) = subtract(self.a, value, p, p & self.decimal_mode);
    }

    /// AND -- A AND M -> A (PM p. B-3).
    pub(crate) fn op_and(&mut self, address: u16) {
        let a = self.a & self.read(address);
        self.a = a;
        self.p = (self.p & !(N | Z)) | NZ[a as usize];
    }

    /// ORA -- A OR M -> A (PM p. B-20).
    pub(crate) fn op_ora(&mut self, address: u16) {
        let a = self.a | self.read(address);
        self.a = a;
        self.p = (self.p & !(N | Z)) | NZ[a as usize];
    }

    /// EOR -- A EOR M -> A (PM p. B-14).
    pub(crate) fn op_eor(&mut self, address: u16) {
        let a = self.a ^ self.read(address);
        self.a = a;
        self.p = (self.p & !(N | Z)) | NZ[a as usize];
    }

    pub(crate) fn compare(&mut self, register: u8, value: u8) {
        // C is "no borrow": register >= M, unsigned; N and Z from the
        // difference, which is not kept.
        let mut p = (self.p & !(N | Z | C)) | NZ[register.wrapping_sub(value) as usize];
        if register >= value {
            p |= C;
        }
        self.p = p;
    }

    /// CMP -- A - M, flags only (PM p. B-11).
    pub(crate) fn op_cmp(&mut self, address: u16) {
        let value = self.read(address);
        self.compare(self.a, value);
    }

    /// CPX -- X - M, flags only (PM p. B-12).
    pub(crate) fn op_cpx(&mut self, address: u16) {
        let value = self.read(address);
        self.compare(self.x, value);
    }

    /// CPY -- Y - M, flags only (PM p. B-12).
    pub(crate) fn op_cpy(&mut self, address: u16) {
        let value = self.read(address);
        self.compare(self.y, value);
    }

    /// BIT -- A AND M sets Z; M7 -> N, M6 -> V (PM p. B-6).
    pub(crate) fn op_bit(&mut self, address: u16) {
        let value = self.read(address);
        let mut p = (self.p & !(N | V | Z)) | (value & (N | V));
        if self.a & value == 0 {
            p |= Z;
        }
        self.p = p;
    }
}
