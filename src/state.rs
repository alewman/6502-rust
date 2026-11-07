//! Values for capturing and restoring NMOS 6502 processor state (`state.py`).

use crate::frame::PollFields;

/// Complete CPU-owned state at an instruction boundary.
///
/// Registers, the interrupt requests the host has made and the core has not
/// yet taken, and the internal state that decides what the next `step()`
/// does. Host memory, devices, scheduling and cycle counters are deliberately
/// excluded: restoring this value restores the processor, not a machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CpuState {
    pub a: u8,
    pub x: u8,
    pub y: u8,
    pub s: u8,
    pub pc: u16,
    /// P as the core keeps it: bit 5 set and B (bit 4) clear, since neither
    /// is a flip-flop.
    pub p: u8,
    /// JAM has run; only a reset restarts the CPU.
    pub halted: bool,
    pub reset_pending: bool,
    /// The IRQ line, a level.
    pub maskable_interrupt_pending: bool,
    /// An NMI edge latched and not yet taken.
    pub non_maskable_interrupt_pending: bool,
    /// The I flag the last instruction's interrupt poll saw, when that
    /// differs in timing from P: set by CLI, SEI and PLP for exactly one
    /// boundary.
    pub polled_i: Option<bool>,
    /// The IRQ line and the NMI latch as the last poll saw them, at the end
    /// of the instruction's second-to-last cycle, when that is not what the
    /// two requests above say (a bus callback changed one in the last
    /// cycle); `None` when the poll agrees with them.
    pub polled_maskable_interrupt: Option<bool>,
    pub polled_non_maskable_interrupt: Option<bool>,
    /// The last step was BRK, an interrupt entry or reset, which do not
    /// poll: the next boundary takes nothing, and requests made since wait
    /// for the next instruction's poll.
    pub skipped_poll: bool,
}

impl Default for CpuState {
    fn default() -> Self {
        CpuState {
            a: 0,
            x: 0,
            y: 0,
            s: 0,
            pc: 0,
            p: 0x24,
            halted: false,
            reset_pending: false,
            maskable_interrupt_pending: false,
            non_maskable_interrupt_pending: false,
            polled_i: None,
            polled_maskable_interrupt: None,
            polled_non_maskable_interrupt: None,
            skipped_poll: false,
        }
    }
}

impl CpuState {
    /// The frame's fields of this state.
    pub fn poll_fields(&self) -> PollFields {
        PollFields {
            halted: self.halted,
            reset_pending: self.reset_pending,
            maskable_interrupt_pending: self.maskable_interrupt_pending,
            non_maskable_interrupt_pending: self.non_maskable_interrupt_pending,
            polled_i: self.polled_i,
            polled_maskable_interrupt: self.polled_maskable_interrupt,
            polled_non_maskable_interrupt: self.polled_non_maskable_interrupt,
            skipped_poll: self.skipped_poll,
        }
    }

    /// Check the invariant the reference's constructor enforces: P has bit 5
    /// set and B clear (the field widths are the types').
    pub fn validate(&self) -> Result<(), String> {
        if self.p & 0x30 != 0x20 {
            return Err("p must have bit 5 set and B (bit 4) clear".into());
        }
        Ok(())
    }
}
