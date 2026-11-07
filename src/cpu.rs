//! The public NMOS 6502 CPU (`cpu.py`): the frame's four hooks, the step, and
//! state capture.
//!
//! The host owns memory and every device: it gives the CPU a [`Bus`], calls
//! [`Mos6502::step`], and adds the returned cycle count to its own clock. See
//! the reference's docs/start-here.md, "The embedding contract".

use crate::core::{Bus, Mos6502};
use crate::dispatch::{Mode, OPCODES};
use crate::frame::{Frame, Part};
use crate::state::CpuState;

impl<T: Bus> Part for Mos6502<T> {
    fn frame(&self) -> &Frame {
        &self.frame
    }

    fn frame_mut(&mut self) -> &mut Frame {
        &mut self.frame
    }

    fn status(&self) -> u8 {
        self.p
    }

    fn accept_reset(&mut self) -> u32 {
        self.reset_sequence()
    }

    fn halted_cycle(&mut self) -> u32 {
        // JAM has stopped the processor: it reads $FFFF every cycle until reset.
        self.read(0xFFFF);
        1
    }

    fn enter_interrupt(&mut self, kind: u8) -> u32 {
        self.enter_interrupt_sequence(kind)
    }

    fn execute(&mut self) -> u32 {
        let pc = self.pc;
        let opcode = self.read(pc);
        self.pc = pc.wrapping_add(1);
        let (op, mode, cycles) = OPCODES[opcode as usize];
        if mode == Mode::Implied {
            self.dispatch_implied(op);
        } else {
            let address = self.address(mode);
            self.dispatch_operand(op, address);
        }
        let extra = self.extra;
        if extra != 0 {
            self.extra = 0;
            return cycles + extra;
        }
        cycles
    }
}

impl<T: Bus> Mos6502<T> {
    /// Execute one instruction, interrupt entry or reset; return its cycle
    /// count ([`Part::step`]).
    #[inline]
    pub fn step(&mut self) -> u32 {
        Part::step(self)
    }

    /// Whether JAM has stopped the CPU.
    pub fn halted(&self) -> bool {
        self.frame.halted
    }

    /// Return a snapshot of all CPU-owned state.
    ///
    /// No host reads, no side effects. Memory and devices are the host's and
    /// are not included.
    pub fn capture_state(&self) -> CpuState {
        let poll = self.frame.poll_fields();
        CpuState {
            a: self.a,
            x: self.x,
            y: self.y,
            s: self.s,
            pc: self.pc,
            p: self.p,
            halted: poll.halted,
            reset_pending: poll.reset_pending,
            maskable_interrupt_pending: poll.maskable_interrupt_pending,
            non_maskable_interrupt_pending: poll.non_maskable_interrupt_pending,
            polled_i: poll.polled_i,
            polled_maskable_interrupt: poll.polled_maskable_interrupt,
            polled_non_maskable_interrupt: poll.polled_non_maskable_interrupt,
            skipped_poll: poll.skipped_poll,
        }
    }

    /// Restore a captured CPU state without touching the host.
    ///
    /// Restoring the processor alone does not restore memory or devices.
    pub fn restore_state(&mut self, state: &CpuState) -> Result<(), String> {
        state.validate()?;
        self.a = state.a;
        self.x = state.x;
        self.y = state.y;
        self.s = state.s;
        self.pc = state.pc;
        self.p = state.p;
        self.frame.restore_poll(&state.poll_fields());
        self.extra = 0;
        Ok(())
    }
}
