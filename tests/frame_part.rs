//! Rung 5: the frame carries a part that is not the NMOS 6502.
//!
//! `Toy` implements [`Part`] on the frame alone, as a 65C816 core would: its
//! own registers, its own `execute` (an "instruction" is a number of bus
//! cycles), its own sequences, and nothing of `Mos6502`. The tests check
//! that the frame's step and poll behave as the reference's `CPUFrame`
//! documents for any part.

use sixfiveohtwo::frame::{Frame, Part, I, IRQ_LINE, NMI_LATCH, NO_POLL};

/// What the toy did at each step, and when the test's events fire.
struct Toy {
    frame: Frame,
    p: u8,
    /// Cycles each instruction takes, all bus reads.
    length: u32,
    /// The access (counted from 0) during which `event` runs.
    event_at: Option<(u64, fn(&mut Frame))>,
    accesses: u64,
    log: Vec<&'static str>,
    halt_after: Option<u32>,
    instructions: u32,
}

impl Toy {
    fn new(length: u32) -> Self {
        Toy {
            frame: Frame::default(),
            p: 0x20,
            length,
            event_at: None,
            accesses: 0,
            log: Vec::new(),
            halt_after: None,
            instructions: 0,
        }
    }

    /// One bus cycle, made the way the frame asks: `access()` first.
    fn cycle(&mut self) {
        self.frame.access();
        if let Some((at, event)) = self.event_at {
            if at == self.accesses {
                event(&mut self.frame);
            }
        }
        self.accesses += 1;
    }
}

impl Part for Toy {
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
        self.log.push("reset");
        self.frame.reset_pending = false;
        self.frame.halted = false;
        self.frame.polled = Some(NO_POLL);
        self.frame.deferred = None;
        3
    }
    fn halted_cycle(&mut self) -> u32 {
        self.log.push("halted");
        self.cycle();
        1
    }
    fn enter_interrupt(&mut self, kind: u8) -> u32 {
        self.log.push(if kind == NMI_LATCH { "nmi" } else { "irq" });
        if kind == NMI_LATCH {
            self.frame.requests &= !NMI_LATCH;
        }
        self.p |= I;
        self.frame.polled = Some(NO_POLL);
        self.frame.deferred = None;
        5
    }
    fn execute(&mut self) -> u32 {
        self.log.push("instruction");
        for _ in 0..self.length {
            self.cycle();
        }
        self.instructions += 1;
        if Some(self.instructions) == self.halt_after {
            self.frame.halted = true;
        }
        self.length
    }
}

#[test]
fn a_request_between_steps_is_taken_at_the_next_boundary() {
    let mut toy = Toy::new(3);
    toy.step();
    toy.frame.request_maskable_interrupt();
    assert_eq!(toy.step(), 5);
    assert_eq!(toy.log, ["instruction", "irq"]);
}

#[test]
fn a_request_in_the_last_cycle_waits_one_instruction() {
    // Access 2 is the last cycle of the first three-cycle instruction.
    let mut toy = Toy::new(3);
    toy.event_at = Some((2, |frame| frame.request_maskable_interrupt()));
    toy.step();
    toy.step();
    toy.step();
    assert_eq!(toy.log, ["instruction", "instruction", "irq"]);
}

#[test]
fn a_request_before_the_last_cycle_is_polled() {
    let mut toy = Toy::new(3);
    toy.event_at = Some((1, |frame| frame.request_maskable_interrupt()));
    toy.step();
    toy.step();
    assert_eq!(toy.log, ["instruction", "irq"]);
}

#[test]
fn a_pulse_within_one_cycle_is_never_seen() {
    // Raised and dropped during the last cycle: the poll keeps the requests
    // from before the first change.
    let mut toy = Toy::new(3);
    toy.event_at = Some((2, |frame| {
        frame.request_maskable_interrupt();
        frame.clear_maskable_interrupt();
    }));
    toy.step();
    toy.step();
    toy.step();
    assert_eq!(toy.log, ["instruction", "instruction", "instruction"]);
}

#[test]
fn reset_comes_first_and_restarts_a_halted_part() {
    let mut toy = Toy::new(2);
    toy.halt_after = Some(1);
    toy.step();
    toy.step();
    toy.frame.request_non_maskable_interrupt();
    toy.frame.request_reset();
    toy.step();
    assert_eq!(toy.log, ["instruction", "halted", "reset"]);
    // Reset does not poll: one instruction runs before the NMI is taken.
    toy.step();
    toy.step();
    assert_eq!(toy.log[3..], ["instruction", "nmi"]);
}

#[test]
fn nmi_wins_over_irq_and_ignores_i() {
    let mut toy = Toy::new(2);
    toy.p |= I;
    toy.frame.request_maskable_interrupt();
    toy.frame.request_non_maskable_interrupt();
    toy.step();
    assert_eq!(toy.log, ["nmi"]);
    // I is set and the entry did not poll: the IRQ stays masked.
    toy.step();
    toy.step();
    assert_eq!(toy.log, ["nmi", "instruction", "instruction"]);
}

#[test]
fn polled_i_decides_one_boundary() {
    // A part's CLI: I cleared after the poll saw it set.
    let mut toy = Toy::new(2);
    toy.p |= I;
    toy.frame.request_maskable_interrupt();
    toy.step();
    toy.frame.polled_i = Some(I);
    toy.p &= !I;
    toy.step();
    toy.step();
    assert_eq!(toy.log, ["instruction", "instruction", "irq"]);
}

/// A part with an extra input, taken by overriding the boundary's choice.
struct Aborting {
    toy: Toy,
    abort: bool,
}

const ABORT: u8 = 0x08;

impl Part for Aborting {
    fn frame(&self) -> &Frame {
        &self.toy.frame
    }
    fn frame_mut(&mut self) -> &mut Frame {
        &mut self.toy.frame
    }
    fn status(&self) -> u8 {
        self.toy.p
    }
    fn accept_reset(&mut self) -> u32 {
        self.toy.accept_reset()
    }
    fn halted_cycle(&mut self) -> u32 {
        self.toy.halted_cycle()
    }
    fn enter_interrupt(&mut self, kind: u8) -> u32 {
        if kind == ABORT {
            self.toy.log.push("abort");
            self.abort = false;
            self.toy.frame.requests &= !ABORT;
            return 4;
        }
        self.toy.enter_interrupt(kind)
    }
    fn execute(&mut self) -> u32 {
        self.toy.execute()
    }
    fn boundary_interrupt(&mut self) -> u8 {
        if self.toy.frame.requests & ABORT != 0 {
            return ABORT;
        }
        let p = self.status();
        self.frame_mut().boundary_interrupt(p)
    }
}

#[test]
fn a_part_adds_an_input_through_the_requests() {
    let mut part = Aborting {
        toy: Toy::new(2),
        abort: false,
    };
    part.step();
    let requests = part.toy.frame.requests | ABORT | IRQ_LINE;
    part.toy.frame.request(requests);
    part.abort = true;
    part.step();
    part.step();
    assert_eq!(part.toy.log, ["instruction", "abort", "irq"]);
    assert!(!part.abort);
}

#[test]
fn note_stall_keeps_the_last_two_snapshots() {
    let mut frame = Frame::default();
    frame.note_stall();
    frame.request_maskable_interrupt();
    frame.note_stall();
    assert_eq!(frame.stalled_before, Some(0));
    assert_eq!(frame.stalled, Some(IRQ_LINE));
    assert_eq!(frame.deferred, None, "a stall confirms the change");
}
