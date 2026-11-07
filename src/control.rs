//! Branches, jumps, subroutines, the stack, flag instructions, interrupts and
//! reset (`_control.py`).
//!
//! The vectors are PM's (section 9.0, p. 124): NMI at $FFFA, RESET at $FFFC,
//! IRQ and BRK at $FFFE. Interrupt timing that MOS does not document -- the
//! one-instruction delay after CLI, SEI and PLP, and an NMI taking over a BRK
//! or IRQ already under way -- is from the NESdev wiki's "CPU interrupts"
//! page (documentation tier; it cites the visual6502 transistor-level
//! simulation).

use crate::core::{Bus, Mos6502, B, C, D, I, N, STACK, U, V, Z};
use crate::frame::{NMI_LATCH, NO_POLL};

pub const VECTOR_NMI: u16 = 0xFFFA;
pub const VECTOR_RESET: u16 = 0xFFFC;
pub const VECTOR_IRQ: u16 = 0xFFFE; // shared by BRK

impl<T: Bus> Mos6502<T> {
    // -- branches (HM p. A-13; NMS p. 87) ---------------------------------

    fn branch(&mut self, taken: bool) {
        // A taken branch spends a cycle reading the next opcode while the
        // offset is added to PCL, and one more reading PCH:new-PCL when that
        // crosses a page (NMS p. 87; HM p. A-13 lists the first read at the
        // target instead: see the reference's docs/validation.md,
        // "Divergences").
        //
        // Its interrupt poll is its own (NESdev, "CPU interrupts", "Branch
        // instructions and interrupts"): always at the end of cycle 1, never
        // at the end of cycle 2 of a taken branch, and at the end of cycle 3
        // of one that crosses a page; seen at either point, it is taken. Not
        // taken, cycle 1 is the second-to-last and the usual rule holds.
        let mut early = self.frame.requests; // as they stand at the end of cycle 1
        self.frame.stalled = None;
        self.frame.stalled_before = None;
        let offset = self.fetch();
        if let Some(stalled) = self.frame.stalled {
            // RDY held the operand fetch: requests raised during the stall
            // are polled at its end (note_stall).
            early = stalled;
            self.frame.stalled = None;
            self.frame.stalled_before = None;
        }
        if !taken {
            return;
        }
        let pc = self.pc;
        self.read(pc);
        if let Some(before) = self.frame.stalled_before {
            // A stall of the next-opcode read: requests raised by the start
            // of its last halted cycle count (perfect6502).
            early |= before;
            self.frame.stalled = None;
            self.frame.stalled_before = None;
        }
        let target = pc.wrapping_add(offset as i8 as u16);
        if (target ^ pc) & 0xFF00 != 0 {
            self.read((pc & 0xFF00) | (target & 0xFF));
            self.extra = 2;
            // The usual poll, at the end of cycle 3, or the early one.
            let deferred = self.frame.deferred.unwrap_or(self.frame.requests);
            self.frame.deferred = Some(early | deferred);
        } else {
            self.extra = 1;
            self.frame.deferred = Some(early);
        }
        self.pc = target;
    }

    /// BPL -- branch if N = 0 (PM p. B-7).
    pub(crate) fn op_bpl(&mut self) {
        self.branch(self.p & N == 0);
    }

    /// BMI -- branch if N = 1 (PM p. B-6).
    pub(crate) fn op_bmi(&mut self) {
        self.branch(self.p & N != 0);
    }

    /// BVC -- branch if V = 0 (PM p. B-8).
    pub(crate) fn op_bvc(&mut self) {
        self.branch(self.p & V == 0);
    }

    /// BVS -- branch if V = 1 (PM p. B-9).
    pub(crate) fn op_bvs(&mut self) {
        self.branch(self.p & V != 0);
    }

    /// BCC -- branch if C = 0 (PM p. B-4).
    pub(crate) fn op_bcc(&mut self) {
        self.branch(self.p & C == 0);
    }

    /// BCS -- branch if C = 1 (PM p. B-5).
    pub(crate) fn op_bcs(&mut self) {
        self.branch(self.p & C != 0);
    }

    /// BNE -- branch if Z = 0 (PM p. B-7).
    pub(crate) fn op_bne(&mut self) {
        self.branch(self.p & Z == 0);
    }

    /// BEQ -- branch if Z = 1 (PM p. B-5).
    pub(crate) fn op_beq(&mut self) {
        self.branch(self.p & Z != 0);
    }

    // -- jumps and subroutines ---------------------------------------------

    /// JMP abs -- (PC + 1) -> PCL, (PC + 2) -> PCH (PM p. B-16).
    pub(crate) fn op_jmp(&mut self) {
        self.pc = self.fetch_word();
    }

    /// JMP (abs) -- the pointer's high byte never carries into the next page (PM p. B-16).
    pub(crate) fn op_jmp_ind(&mut self) {
        let pointer = self.fetch_word();
        let low = self.read(pointer) as u16;
        // JMP ($C0FF) reads $C0FF and $C000 (NMS p. 94; HM p. A-12).
        let high = self.read((pointer & 0xFF00) | (pointer.wrapping_add(1) & 0xFF)) as u16;
        self.pc = (high << 8) | low;
    }

    /// JSR -- push PC + 2 - 1, then jump (PM p. B-17).
    pub(crate) fn op_jsr(&mut self) {
        // Low target byte, a stack read, PCH and PCL pushed, and only then
        // the high target byte, from the operand's own address (HM p. A-10;
        // NMS p. 86). If that address is the stack slot just written, the
        // pushed byte is what arrives.
        let low = self.fetch() as u16;
        self.read_stack();
        let pc = self.pc;
        self.push((pc >> 8) as u8);
        self.push(pc as u8);
        self.pc = ((self.read(pc) as u16) << 8) | low;
    }

    /// RTS -- pull PC, then PC + 1 -> PC (PM p. B-23).
    pub(crate) fn op_rts(&mut self) {
        self.read_pc();
        self.read_stack();
        let low = self.pull() as u16;
        let pc = ((self.pull() as u16) << 8) | low;
        self.read(pc); // the increment's cycle reads the pulled address (HM p. A-12)
        self.pc = pc.wrapping_add(1);
    }

    // -- stack -------------------------------------------------------------

    /// PHA -- push A (PM p. B-20).
    pub(crate) fn op_pha(&mut self) {
        self.read_pc();
        self.push(self.a);
    }

    /// PHP -- push P with B and bit 5 set (PM p. B-21; NMS p. 95).
    pub(crate) fn op_php(&mut self) {
        self.read_pc();
        self.push(self.p | B | U);
    }

    /// PLA -- pull A (PM p. B-21).
    pub(crate) fn op_pla(&mut self) {
        self.read_pc();
        self.read_stack();
        let a = self.pull();
        self.a = a;
        self.p = (self.p & !(N | Z)) | (a & N) | if a == 0 { Z } else { 0 };
    }

    /// PLP -- pull P; B and bit 5 keep their fixed values (PM p. B-22).
    pub(crate) fn op_plp(&mut self) {
        self.read_pc();
        self.read_stack();
        // The interrupt poll has already happened, on the old I (NESdev).
        self.frame.polled_i = Some(self.p & I);
        self.p = (self.pull() | U) & !B;
    }

    // -- flags -------------------------------------------------------------

    /// CLC -- 0 -> C (PM p. B-9).
    pub(crate) fn op_clc(&mut self) {
        self.read_pc();
        self.p &= !C;
    }

    /// SEC -- 1 -> C (PM p. B-24).
    pub(crate) fn op_sec(&mut self) {
        self.read_pc();
        self.p |= C;
    }

    /// CLD -- 0 -> D (PM p. B-10).
    pub(crate) fn op_cld(&mut self) {
        self.read_pc();
        self.p &= !D;
    }

    /// SED -- 1 -> D (PM p. B-25).
    pub(crate) fn op_sed(&mut self) {
        self.read_pc();
        self.p |= D;
    }

    /// CLV -- 0 -> V (PM p. B-11).
    pub(crate) fn op_clv(&mut self) {
        self.read_pc();
        self.p &= !V;
    }

    /// CLI -- 0 -> I, one instruction late for a waiting IRQ (PM p. B-10).
    pub(crate) fn op_cli(&mut self) {
        self.read_pc();
        // I changes after this instruction's interrupt poll, so an IRQ
        // already waiting is taken after the next instruction, not this one
        // (NESdev, "Delayed IRQ response after CLI, SEI, and PLP").
        self.frame.polled_i = Some(self.p & I);
        self.p &= !I;
    }

    /// SEI -- 1 -> I; an IRQ polled before it is still taken (PM p. B-25).
    pub(crate) fn op_sei(&mut self) {
        self.read_pc();
        self.frame.polled_i = Some(self.p & I);
        self.p |= I;
    }

    /// NOP -- no operation (PM p. B-19).
    pub(crate) fn op_nop(&mut self) {
        self.read_pc();
    }

    // -- software and hardware interrupts ----------------------------------

    fn interrupt(&mut self, vector: u16, pushed_p: u8) {
        // Shared by BRK, IRQ and NMI after their first two cycles: PCH, PCL
        // and P pushed, I set, then the vector (HM p. A-11; NMS pp. 82-83).
        let mut vector = vector;
        let pc = self.pc;
        self.push((pc >> 8) as u8);
        self.push(pc as u8);
        if vector != VECTOR_NMI && self.frame.requests & NMI_LATCH != 0 {
            // The vector is chosen after cycle 4, the PCL push: an NMI by
            // then takes it over, BRK's or the IRQ's frame with NMI's vector
            // (NESdev, "CPU interrupts", "Interrupt hijacking").
            self.frame.requests &= !NMI_LATCH;
            vector = VECTOR_NMI;
        }
        self.push(pushed_p);
        self.p |= I;
        let low = self.read(vector) as u16;
        self.pc = ((self.read(vector.wrapping_add(1)) as u16) << 8) | low;
        // The sequence does not poll: the handler's first instruction runs
        // before any other interrupt is taken (NESdev, "CPU interrupts").
        self.frame.deferred = Some(NO_POLL);
    }

    /// BRK -- push PC + 2 and P with B set, jump through $FFFE (PM p. B-8).
    pub(crate) fn op_brk(&mut self) {
        self.fetch(); // the byte after BRK is skipped: the return address is PC + 2
        self.interrupt(VECTOR_IRQ, self.p | B | U);
    }

    /// RTI -- pull P, then PC (PM p. B-23).
    pub(crate) fn op_rti(&mut self) {
        self.read_pc();
        self.read_stack();
        // Unlike PLP, the restored I decides the very next poll (NESdev).
        self.p = (self.pull() | U) & !B;
        let low = self.pull() as u16;
        self.pc = ((self.pull() as u16) << 8) | low;
    }

    /// Take IRQ or NMI at an instruction boundary: seven cycles, B clear.
    pub(crate) fn enter_interrupt_sequence(&mut self, kind: u8) -> u32 {
        let vector = if kind == NMI_LATCH {
            VECTOR_NMI
        } else {
            VECTOR_IRQ
        };
        // The opcode at PC is fetched and discarded, then read again: PC is
        // not advanced (NESdev, "CPU interrupts"; NMS p. 83 lists PC + 1 for
        // the second read -- see the reference's docs/validation.md,
        // "Divergences").
        self.frame.polled_i = None;
        let pc = self.pc;
        self.read(pc);
        self.read(pc);
        if vector == VECTOR_NMI {
            self.frame.requests &= !NMI_LATCH;
        }
        self.interrupt(vector, (self.p | U) & !B);
        self.frame.polled = Some(NO_POLL);
        self.frame.deferred = None;
        7
    }

    /// Run the seven-cycle start sequence: I set, PC from $FFFC (PM p. 127).
    pub(crate) fn reset_sequence(&mut self) -> u32 {
        // PM section 9.2, Example 9.1: two cycles whose address MOS calls
        // "don't care" (read here at PC, as NMS p. 83 and NESdev do), three
        // stack cycles with R/W held at read -- S decrements, nothing is
        // written -- and the vector. Only I and PC change (PM section 9.3).
        self.frame.reset_pending = false;
        self.frame.halted = false;
        self.frame.polled_i = None;
        let pc = self.pc;
        self.read(pc);
        self.read(pc);
        for _ in 0..3 {
            self.read(STACK | self.s as u16);
            self.s = self.s.wrapping_sub(1);
        }
        self.p |= I;
        let low = self.read(VECTOR_RESET) as u16;
        self.pc = ((self.read(VECTOR_RESET + 1) as u16) << 8) | low;
        self.frame.polled = Some(NO_POLL); // like an interrupt sequence, reset does not poll
        self.frame.deferred = None;
        7
    }
}
