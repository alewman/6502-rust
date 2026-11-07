//! CPU state, fetch, effective-address and stack helpers (`_core.py`).
//!
//! Every cycle of an NMOS 6502 is one bus access, a read or a write, and this
//! core makes each of them, dummy reads included, in the order the chip does.
//! The addressing-mode helpers below are therefore written cycle by cycle.
//! Sources are cited as in the reference's docs/start-here.md: PM is MOS
//! Technology's MCS6500 Microcomputer Family Programming Manual (6500-50A,
//! January 1976), HM its Hardware Manual (6500-10A, January 1976), whose
//! Appendix A lists the address and data bus for every cycle, and NMS is
//! groepaz's "No More Secrets: NMOS 6510 Unintended Opcodes" v0.99
//! (2024-12-24).

use crate::frame::Frame;

// Status register bits, P (PM section 3.8, "Flag Summary", p. 30).
pub const N: u8 = 0x80; // negative: bit 7 of the result
pub const V: u8 = 0x40; // overflow: two's complement overflow
pub const U: u8 = 0x20; // unused: no flip-flop; reads as 1 when P is pushed
pub const B: u8 = 0x10; // break: no flip-flop; 1 in the byte BRK and PHP push, 0 for IRQ/NMI
pub const D: u8 = 0x08; // decimal mode
pub const I: u8 = crate::frame::I; // IRQ disable (bit 2 on every part: the frame's)
pub const Z: u8 = 0x02; // zero
pub const C: u8 = 0x01; // carry

/// N and Z for every 8-bit result, so a handler sets both with one lookup.
pub(crate) const NZ: [u8; 256] = {
    let mut table = [0u8; 256];
    let mut value = 0;
    while value < 256 {
        table[value] = (value as u8 & N) | if value == 0 { Z } else { 0 };
        value += 1;
    }
    table
};

/// The stack is page one: S is the low byte (PM section 8.3, p. 112).
pub const STACK: u16 = 0x0100;

/// The host's side of the bus: one call per cycle, in the chip's order.
///
/// Each call also gets the CPU's [`Frame`], so a device can change an
/// interrupt request during the cycle it happens in (the poll then decides
/// whether that was in time), say that RDY is stretching this read
/// ([`Frame::note_stall`]), or set [`Frame::dma_stalled`].
pub trait Bus {
    fn read(&mut self, address: u16, frame: &mut Frame) -> u8;
    fn write(&mut self, address: u16, value: u8, frame: &mut Frame);
}

/// An NMOS 6502 instruction core whose memory and devices are a host's [`Bus`].
///
/// `step()` calls the bus once per cycle it returns, dummy reads and
/// read-modify-write dummy writes included, in the chip's order, so
/// memory-mapped devices see what they would see on a board. Registers are
/// plain fields: `a`, `x`, `y`, `s` (8-bit), `pc` (16-bit) and `p`, which
/// keeps bit 5 set and B clear. A new CPU has zeroed registers and I set;
/// [`Frame::request_reset`] runs the chip's start sequence.
pub struct Mos6502<T: Bus> {
    pub a: u8,
    pub x: u8,
    pub y: u8,
    pub s: u8,
    pub pc: u16,
    pub p: u8,
    pub bus: T,
    pub frame: Frame,
    /// The chip-dependent constant ANE ($8B) and LXA ($AB) OR into A before
    /// their AND: (A OR CONST) AND X AND #imm, and (A OR CONST) AND #imm (NMS
    /// pp. 60-64). It varies between chips and with temperature; $EE is the
    /// value SingleStepTests records. A host modelling a particular console
    /// sets them.
    pub ane_constant: u8,
    pub lxa_constant: u8,
    /// The D bit as the adder sees it: D where the decimal adjust is wired
    /// up, 0 on a part that has it disconnected (the RP2A03). ADC, SBC,
    /// USBC, RRA, ISC and ARR pass `p & decimal_mode` to the arithmetic.
    pub(crate) decimal_mode: u8,
    /// Cycles a step adds to its opcode's base count: a page crossed by an
    /// indexed read, a branch taken.
    pub(crate) extra: u32,
}

impl<T: Bus> Mos6502<T> {
    /// An NMOS 6502 on `bus`.
    pub fn new(bus: T) -> Self {
        Mos6502 {
            // A, X, Y and S hold "any random condition" until reset, which
            // sets only I and PC (PM section 9.2, p. 126; section 9.3, p.
            // 127); zero makes hosts and traces reproducible.
            a: 0,
            x: 0,
            y: 0,
            s: 0,
            pc: 0,
            // Bit 5 is kept set and bit 4 (B) clear: neither is a flip-flop,
            // and this is the value P reads as everywhere except on the stack.
            p: U | I,
            bus,
            frame: Frame::default(),
            ane_constant: 0xEE,
            lxa_constant: 0xEE,
            decimal_mode: D,
            extra: 0,
        }
    }

    /// The NMOS 6502 inside Nintendo's RP2A03 and RP2A07: decimal mode disconnected.
    ///
    /// The NES and Famicom CPU is this core with the decimal adjust cut out
    /// of the die ("cutting 5 pieces of polysilicon"; the instruction
    /// decoder is otherwise identical: the Breaks wiki,
    /// `BreakingNESWiki_DeepL/APU/core.md`, die-derived). D is still a flag
    /// that SED, CLD, PHP, PLP and RTI set, clear, push and pull, but ADC,
    /// SBC, USBC, RRA, ISC and ARR compute in binary with binary flags
    /// whatever it says. The APU, DMA and the rest of the chip are the
    /// host's, like every device.
    pub fn rp2a03(bus: T) -> Self {
        let mut cpu = Mos6502::new(bus);
        cpu.decimal_mode = 0;
        cpu
    }

    // -- the bus ---------------------------------------------------------

    #[inline]
    pub(crate) fn read(&mut self, address: u16) -> u8 {
        self.frame.access();
        self.bus.read(address, &mut self.frame)
    }

    #[inline]
    pub(crate) fn write(&mut self, address: u16, value: u8) {
        self.frame.access();
        self.bus.write(address, value, &mut self.frame);
    }

    // -- fetch -------------------------------------------------------------

    #[inline]
    pub(crate) fn fetch(&mut self) -> u8 {
        let pc = self.pc;
        self.pc = pc.wrapping_add(1);
        self.read(pc)
    }

    pub(crate) fn fetch_word(&mut self) -> u16 {
        let low = self.fetch() as u16; // the 6502 is little-endian: low byte first
        ((self.fetch() as u16) << 8) | low
    }

    /// The dummy read of every one-byte instruction's second cycle (HM p. A-2).
    pub(crate) fn read_pc(&mut self) {
        self.read(self.pc);
    }

    // -- effective addresses, cycle by cycle (HM Appendix A) ---------------
    // Each returns the address the instruction's own read, write or
    // read-modify-write uses, after making the addressing cycles' accesses.
    // Immediate is included: its "address" is the operand's own place in the
    // instruction stream, so every read handler is one line.

    pub(crate) fn ea_imm(&mut self) -> u16 {
        let pc = self.pc;
        self.pc = pc.wrapping_add(1);
        pc
    }

    pub(crate) fn ea_zp(&mut self) -> u16 {
        self.fetch() as u16
    }

    pub(crate) fn ea_zpx(&mut self) -> u16 {
        let base = self.fetch();
        self.read(base as u16); // the unindexed address is read and discarded (HM p. A-4)
        base.wrapping_add(self.x) as u16 // page zero wraps: never carries to page one
    }

    pub(crate) fn ea_zpy(&mut self) -> u16 {
        let base = self.fetch();
        self.read(base as u16);
        base.wrapping_add(self.y) as u16
    }

    pub(crate) fn ea_abs(&mut self) -> u16 {
        self.fetch_word()
    }

    fn indexed(&mut self, base: u16, index: u8, write: bool) -> u16 {
        // The low byte is added first and the address bus shows base-page:low
        // for one cycle; a read ends there unless a page was crossed, which
        // costs a cycle while the carry reaches the high byte (HM p. A-4).
        // Writes and read-modify-writes always spend that cycle (HM pp. A-6, A-9).
        let address = base.wrapping_add(index as u16);
        if write || (address ^ base) & 0xFF00 != 0 {
            self.read((base & 0xFF00) | (address & 0xFF));
            if !write {
                self.extra = 1;
            }
        }
        address
    }

    pub(crate) fn ea_abx(&mut self) -> u16 {
        let base = self.fetch_word();
        self.indexed(base, self.x, false)
    }

    pub(crate) fn ea_aby(&mut self) -> u16 {
        let base = self.fetch_word();
        self.indexed(base, self.y, false)
    }

    pub(crate) fn ea_abx_w(&mut self) -> u16 {
        let base = self.fetch_word();
        self.indexed(base, self.x, true)
    }

    pub(crate) fn ea_aby_w(&mut self) -> u16 {
        let base = self.fetch_word();
        self.indexed(base, self.y, true)
    }

    pub(crate) fn ea_izx(&mut self) -> u16 {
        // (zp,X): the pointer lives in page zero at zp+X, both bytes wrapping
        // within page zero (HM p. A-3).
        let base = self.fetch();
        self.read(base as u16);
        let pointer = base.wrapping_add(self.x);
        let low = self.read(pointer as u16) as u16;
        ((self.read(pointer.wrapping_add(1) as u16) as u16) << 8) | low
    }

    fn izy_base(&mut self) -> u16 {
        let pointer = self.fetch();
        let low = self.read(pointer as u16) as u16;
        ((self.read(pointer.wrapping_add(1) as u16) as u16) << 8) | low
    }

    pub(crate) fn ea_izy(&mut self) -> u16 {
        let base = self.izy_base();
        self.indexed(base, self.y, false) // (zp),Y: HM p. A-5
    }

    pub(crate) fn ea_izy_w(&mut self) -> u16 {
        let base = self.izy_base();
        self.indexed(base, self.y, true)
    }

    // -- stack (S points at the next free byte: PM section 8.3, p. 112) ----

    pub(crate) fn push(&mut self, value: u8) {
        let s = self.s;
        self.write(STACK | s as u16, value);
        self.s = s.wrapping_sub(1);
    }

    pub(crate) fn pull(&mut self) -> u8 {
        let s = self.s.wrapping_add(1);
        self.s = s;
        self.read(STACK | s as u16)
    }

    /// The dummy read of the stack before a pull, or during JSR (HM p. A-10).
    pub(crate) fn read_stack(&mut self) {
        self.read(STACK | self.s as u16);
    }
}
