//! The 105 opcodes MOS did not document, as the NMOS 6502 executes them
//! (`_undocumented.py`).
//!
//! Every one does something repeatable enough to be tested, and
//! SingleStepTests records all of them. Names and behaviour are NMS's; the
//! "magic constant" of ANE and LXA varies from chip to chip and with
//! temperature (NMS p. 60), so it is a setting of each CPU, `ane_constant`
//! and `lxa_constant`, $EE by default, the value SingleStepTests records.

use crate::alu::{add, subtract};
use crate::core::{Bus, Mos6502, C, N, NZ, V, Z};
use crate::shifts::{asl, lsr, rol, ror};

/// What a JAMmed CPU puts on the address bus after its second cycle, as the
/// SingleStepTests corpus (emulator-derived) records it.
const JAM_READS: [u16; 9] = [
    0xFFFF, 0xFFFE, 0xFFFE, 0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF,
];

impl<T: Bus> Mos6502<T> {
    // -- a read-modify-write, then an ALU operation with the result ------

    /// SLO -- ASL M, then A OR M -> A (NMS p. 7).
    pub(crate) fn op_slo(&mut self, address: u16) {
        let (value, p) = asl(self.modify(address), self.p);
        self.write(address, value);
        let a = self.a | value;
        self.a = a;
        self.p = (p & !(N | Z)) | NZ[a as usize];
    }

    /// RLA -- ROL M, then A AND M -> A (NMS p. 9).
    pub(crate) fn op_rla(&mut self, address: u16) {
        let (value, p) = rol(self.modify(address), self.p);
        self.write(address, value);
        let a = self.a & value;
        self.a = a;
        self.p = (p & !(N | Z)) | NZ[a as usize];
    }

    /// SRE -- LSR M, then A EOR M -> A (NMS p. 11).
    pub(crate) fn op_sre(&mut self, address: u16) {
        let (value, p) = lsr(self.modify(address), self.p);
        self.write(address, value);
        let a = self.a ^ value;
        self.a = a;
        self.p = (p & !(N | Z)) | NZ[a as usize];
    }

    /// RRA -- ROR M, then A + M + C -> A, decimal mode included (NMS p. 14).
    pub(crate) fn op_rra(&mut self, address: u16) {
        let (value, p) = ror(self.modify(address), self.p);
        self.write(address, value);
        (self.a, self.p) = add(self.a, value, p, p & self.decimal_mode);
    }

    /// DCP -- DEC M, then CMP M (NMS p. 22).
    pub(crate) fn op_dcp(&mut self, address: u16) {
        let value = self.modify(address).wrapping_sub(1);
        self.write(address, value);
        self.compare(self.a, value);
    }

    /// ISC -- INC M, then A - M - (1 - C) -> A, decimal mode included (NMS p. 26).
    pub(crate) fn op_isc(&mut self, address: u16) {
        let value = self.modify(address).wrapping_add(1);
        self.write(address, value);
        let p = self.p;
        (self.a, self.p) = subtract(self.a, value, p, p & self.decimal_mode);
    }

    // -- loads and stores of two registers -------------------------------

    /// SAX -- A AND X -> M, no flags (NMS p. 16).
    pub(crate) fn op_sax(&mut self, address: u16) {
        self.write(address, self.a & self.x);
    }

    /// LAX -- M -> A and X (NMS p. 19).
    pub(crate) fn op_lax(&mut self, address: u16) {
        let value = self.read(address);
        self.a = value;
        self.x = value;
        self.p = (self.p & !(N | Z)) | NZ[value as usize];
    }

    /// LAS -- M AND S -> A, X and S (NMS p. 41).
    pub(crate) fn op_las(&mut self, address: u16) {
        let value = self.read(address) & self.s;
        self.a = value;
        self.x = value;
        self.s = value;
        self.p = (self.p & !(N | Z)) | NZ[value as usize];
    }

    // -- immediate combinations ------------------------------------------

    /// ANC -- A AND #imm -> A, then N -> C (NMS p. 28).
    pub(crate) fn op_anc(&mut self, address: u16) {
        let a = self.a & self.read(address);
        self.a = a;
        self.p = (self.p & !(N | Z | C)) | NZ[a as usize] | (a >> 7);
    }

    /// ALR -- A AND #imm, then LSR A (NMS p. 30).
    pub(crate) fn op_alr(&mut self, address: u16) {
        let value = self.a & self.read(address);
        (self.a, self.p) = lsr(value, self.p);
    }

    /// ARR -- A AND #imm, then ROR A, with its own C and V (NMS pp. 32, 78).
    pub(crate) fn op_arr(&mut self, address: u16) {
        let value = self.a & self.read(address);
        let mut p = self.p;
        let mut result = (value >> 1) | ((p & C) << 7);
        // N and Z from the rotated value; V from bits 6 and 5 of it.
        p = (p & !(N | V | Z | C)) | NZ[result as usize] | ((result ^ (result << 1)) & V);
        if p & self.decimal_mode != 0 {
            // Decimal mode: a BCD fix-up of each digit of the rotated value,
            // decided by the digits of the value before rotating (NMS p. 78).
            if (value & 0x0F) >= 0x05 {
                result = (result & 0xF0) | (result.wrapping_add(0x06) & 0x0F);
            }
            if (value & 0xF0) >= 0x50 {
                result = result.wrapping_add(0x60);
                p |= C;
            }
        } else if result & 0x40 != 0 {
            p |= C;
        }
        self.a = result;
        self.p = p;
    }

    /// SBX -- (A AND X) - #imm -> X, C as for CMP, no decimal (NMS p. 35).
    pub(crate) fn op_sbx(&mut self, address: u16) {
        let difference = (self.a & self.x) as i32 - self.read(address) as i32;
        let x = (difference & 0xFF) as u8;
        self.x = x;
        self.p = (self.p & !(N | Z | C)) | NZ[x as usize] | if difference >= 0 { C } else { 0 };
    }

    /// USBC -- $EB, the same operation as SBC #imm (NMS p. 40).
    pub(crate) fn op_usbc(&mut self, address: u16) {
        let p = self.p;
        let value = self.read(address);
        (self.a, self.p) = subtract(self.a, value, p, p & self.decimal_mode);
    }

    // -- unstable: magic constant (NMS p. 60) ----------------------------

    /// ANE -- (A OR CONST) AND X AND #imm -> A (NMS p. 61).
    pub(crate) fn op_ane(&mut self, address: u16) {
        let a = (self.a | self.ane_constant) & self.x & self.read(address);
        self.a = a;
        self.p = (self.p & !(N | Z)) | NZ[a as usize];
    }

    /// LXA -- (A OR CONST) AND #imm -> A and X (NMS p. 64).
    pub(crate) fn op_lxa(&mut self, address: u16) {
        let a = (self.a | self.lxa_constant) & self.read(address);
        self.a = a;
        self.x = a;
        self.p = (self.p & !(N | Z)) | NZ[a as usize];
    }

    // -- unstable: address high byte (NMS p. 48) -------------------------

    fn store_and_high(&mut self, address: u16, index: u8, value: u8) {
        // The stored value is ANDed with the base address's high byte plus
        // one; when indexing crossed a page, that ANDed value replaces the
        // high byte of the address written. After a cycle N stretched by a
        // DMA the AND drops out of the stored value but not of the address
        // (NMS pp. 48-49, the table on p. 48; `Frame::dma_stalled`).
        let base = address.wrapping_sub(index as u16);
        let anded = value & ((base >> 8) as u8).wrapping_add(1);
        let mut address = address;
        if (address ^ base) & 0xFF00 != 0 {
            address = ((anded as u16) << 8) | (address & 0xFF);
        }
        let stored = if self.frame.dma_stalled { value } else { anded };
        self.write(address, stored);
    }

    /// SHA -- A AND X AND (H + 1) -> M (NMS p. 50).
    pub(crate) fn op_sha(&mut self, address: u16) {
        self.store_and_high(address, self.y, self.a & self.x);
    }

    /// SHX -- X AND (H + 1) -> M (NMS p. 52).
    pub(crate) fn op_shx(&mut self, address: u16) {
        self.store_and_high(address, self.y, self.x);
    }

    /// SHY -- Y AND (H + 1) -> M (NMS p. 55).
    pub(crate) fn op_shy(&mut self, address: u16) {
        self.store_and_high(address, self.x, self.y);
    }

    /// TAS -- A AND X -> S, then S AND (H + 1) -> M (NMS p. 58).
    pub(crate) fn op_tas(&mut self, address: u16) {
        self.s = self.a & self.x;
        self.store_and_high(address, self.y, self.s);
    }

    // -- no effect, and no return ----------------------------------------

    /// NOP with an operand -- reads M and discards it (NMS p. 43).
    pub(crate) fn op_nop_read(&mut self, address: u16) {
        self.read(address);
    }

    /// JAM -- the processor stops; only a reset restarts it (NMS p. 47).
    pub(crate) fn op_jam(&mut self) {
        self.read_pc();
        for address in JAM_READS {
            self.read(address);
        }
        self.frame.halted = true;
    }
}
