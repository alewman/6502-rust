//! The frame of a 65xx-family core: interrupt requests, the poll and `step()`
//! (transcribed from `sixfiveohtwo/frame.py`).
//!
//! [`crate::Mos6502`] is this frame plus the NMOS instruction set. A core for
//! another member of the family (a 65C816, as `65816-python` builds on the
//! Python frame) implements [`Part`] itself and so inherits none of the NMOS
//! handlers; it gets what is chip-independent and easy to get subtly wrong:
//!
//! - the interrupt request API and the poll: requests are sampled at the end
//!   of an instruction's second-to-last cycle, so a bus callback that changes
//!   one during the last cycle is seen one instruction later (NESdev, "CPU
//!   interrupts", "Detailed interrupt behavior"); CLI, SEI and PLP change I
//!   after their poll (`polled_i`); BRK, interrupt entries and reset do not
//!   poll ([`NO_POLL`]);
//! - `step()`'s skeleton: a pending reset, a halted CPU, the interrupt the
//!   boundary takes, or one instruction.
//!
//! The subclassing surface, as in docs/api-stability.md of the reference:
//!
//! - implement [`Part`]'s four hooks, `accept_reset`, `halted_cycle`,
//!   `enter_interrupt(kind)` and `execute`, each returning the cycles it
//!   took; `kind` is [`NMI_LATCH`] or [`IRQ_LINE`];
//! - call [`Frame::access`] before every bus access (the Python frame routes
//!   each access through its confirming wrappers instead: see below);
//! - keep the interrupt-disable flag in bit 2 of P and return it from
//!   [`Part::status`];
//! - an extra request (an ABORT input) is a further bit of `requests` set
//!   through [`Frame::request`], taken by overriding
//!   [`Part::boundary_interrupt`];
//! - a read a host stretches with RDY low (`note_stall`) leaves the requests
//!   it found in `stalled` and `stalled_before`, for a branch sequence that
//!   polls them, which resets both to `None`.
//!
//! Differences forced by Rust: the Python frame swaps its bus callables for
//! confirming wrappers when a request changes, so the next access confirms it;
//! here the core calls [`Frame::access`] before each access, which does the
//! same check. A bus callback cannot reach the CPU while the CPU is calling
//! it, so the bus receives `&mut Frame` with every access and makes its
//! requests (and `note_stall`, and the NMOS part's `dma_stalled`) there.

/// The IRQ line, a level, as a bit of the requests.
pub const IRQ_LINE: u8 = 0x01;
/// The NMI latch: an edge remembered until it is taken.
pub const NMI_LATCH: u8 = 0x02;
/// In `polled` only: the last sequence (BRK, an interrupt entry, reset) did
/// not poll, so nothing is taken until an instruction has run and polled.
pub const NO_POLL: u8 = 0x04;
/// The interrupt-disable flag, bit 2 of P on every part of the family.
pub const I: u8 = 0x04;

/// The interrupt inputs and the poll's memory, shared by every 65xx part.
///
/// The fields are the subclassing surface (the Python frame's underscored
/// attributes); a host uses the methods.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Frame {
    pub reset_pending: bool,
    /// `IRQ_LINE | NMI_LATCH`, as the host has set them.
    pub requests: u8,
    /// The chip polls its interrupt detectors in an instruction's last
    /// cycle, so what counts is the requests as they stood at the end of the
    /// second-to-last cycle (NESdev, "CPU interrupts", "Detailed interrupt
    /// behavior"). Usually that is the requests as they stand, and `polled`
    /// is `None`. It holds what the poll saw instead, for the next boundary
    /// only, when that differs: a bus callback changed a request in the last
    /// cycle, too late (`deferred`, see [`Frame::request`]), or the step was
    /// BRK, an interrupt entry or reset, which do not poll ([`NO_POLL`]). A
    /// request the host makes between steps is taken at the next boundary,
    /// unless that step did not poll.
    pub polled: Option<u8>,
    pub deferred: Option<u8>,
    /// The requests as a host's last two `note_stall` calls found them, for a
    /// branch's poll: `None` when the read has not been stalled.
    pub stalled: Option<u8>,
    pub stalled_before: Option<u8>,
    /// CLI, SEI and PLP change I after the chip has polled for interrupts,
    /// so the next boundary decides on the old I. `None`: poll P as it is.
    pub polled_i: Option<u8>,
    /// The CPU has stopped (JAM on the NMOS part); `halted_cycle` decides
    /// what a step does until reset.
    pub halted: bool,
    /// The NMOS part's DMA input (`MOS6502.dma_stalled` in the reference):
    /// set by a host whose bus callback stretched the current read cycle
    /// with RDY low, cleared by it on the next read that is not. SHA, SHX,
    /// SHY and TAS look at it on their write. Kept here because the bus
    /// callback reaches the CPU only through the frame.
    pub dma_stalled: bool,
}

impl Frame {
    /// Make an access: confirm a request change made since the last one.
    ///
    /// The first change since the last access keeps the requests from before
    /// it in `deferred` (see [`Frame::request`]). If an access comes in the
    /// same step, the change was before the last cycle and the poll sees it;
    /// if the step ends first, the poll saw `deferred`.
    #[inline]
    pub fn access(&mut self) {
        self.deferred = None;
    }

    /// Change the requests, and arrange to learn whether a bus access follows.
    pub fn request(&mut self, requests: u8) {
        if self.deferred.is_none() {
            self.deferred = Some(self.requests);
        }
        self.requests = requests;
    }

    /// Say, from inside a read callback, that RDY is holding this read (a DMA).
    ///
    /// Call it as the stall begins and again after each halted cycle, then
    /// make the access. The halted chip repeats the cycle and its interrupt
    /// detectors keep sampling, so a request raised during the stall is
    /// polled at the end of the read it stretched, whichever cycle that is,
    /// even the last (perfect6502 with RDY held low). A taken branch's
    /// next-opcode read, which does not poll, takes requests raised by the
    /// start of the stall's last halted cycle, not during it (perfect6502;
    /// AccuracyCoin "Interrupt flag latency" test A).
    ///
    /// The frame only records what the requests were at each call, in
    /// `stalled` and `stalled_before`; a part's branch sequence reads them
    /// (the NMOS one does) and resets both to `None`.
    pub fn note_stall(&mut self) {
        self.stalled_before = self.stalled;
        self.stalled = Some(self.requests);
        if self.deferred.is_some() {
            self.access();
        }
    }

    /// The interrupt this boundary takes: [`NMI_LATCH`], [`IRQ_LINE`], or 0 for none.
    pub fn boundary_interrupt(&mut self, p: u8) -> u8 {
        let polled = match self.polled.take() {
            None => self.requests,
            Some(polled) => polled,
        };
        if polled & NMI_LATCH != 0 {
            return NMI_LATCH;
        }
        match self.polled_i.take() {
            None => {
                if polled & IRQ_LINE != 0 && p & I == 0 {
                    return IRQ_LINE;
                }
            }
            Some(polled_i) => {
                if polled & IRQ_LINE != 0 && polled_i == 0 {
                    return IRQ_LINE;
                }
            }
        }
        0
    }

    // -- interrupt and reset inputs ------------------------------------

    /// Run the start sequence at the next boundary.
    ///
    /// The start sequence begins when RESET is released (PM section 9.2), so
    /// one request is one reset, whenever the host's board releases the
    /// line. Reset has priority over everything, and restarts a halted CPU.
    pub fn request_reset(&mut self) {
        self.reset_pending = true;
    }

    /// Cancel a requested reset that has not yet reached a boundary.
    pub fn clear_reset(&mut self) {
        self.reset_pending = false;
    }

    /// Whether the IRQ line is asserted.
    pub fn maskable_interrupt_pending(&self) -> bool {
        self.requests & IRQ_LINE != 0
    }

    /// Assert IRQ, a level: it stays asserted until
    /// [`Frame::clear_maskable_interrupt`].
    ///
    /// Taking the interrupt does not clear it; on a board the device holds
    /// IRQ low until its handler acknowledges it, and so must the host.
    /// While it is asserted and I is clear, every poll takes the interrupt.
    pub fn request_maskable_interrupt(&mut self) {
        self.request(self.requests | IRQ_LINE);
        if let Some(polled) = self.polled {
            if polled & NO_POLL == 0 {
                self.polled = Some(polled | IRQ_LINE);
            }
        }
    }

    /// Release IRQ.
    pub fn clear_maskable_interrupt(&mut self) {
        self.request(self.requests & !IRQ_LINE);
        if let Some(polled) = self.polled {
            self.polled = Some(polled & !IRQ_LINE);
        }
    }

    /// Whether an NMI edge has been latched and not yet taken.
    pub fn non_maskable_interrupt_pending(&self) -> bool {
        self.requests & NMI_LATCH != 0
    }

    /// Latch an NMI edge; the next poll takes it, whatever I says.
    ///
    /// NMI is edge-triggered: call this on the line's falling edge. Requests
    /// made while one is pending coalesce into one NMI.
    pub fn request_non_maskable_interrupt(&mut self) {
        self.request(self.requests | NMI_LATCH);
        if let Some(polled) = self.polled {
            if polled & NO_POLL == 0 {
                self.polled = Some(polled | NMI_LATCH);
            }
        }
    }

    /// Cancel a latched NMI that has not yet been taken, even one already polled.
    pub fn clear_non_maskable_interrupt(&mut self) {
        self.request(self.requests & !NMI_LATCH);
        if let Some(polled) = self.polled {
            self.polled = Some(polled & !NMI_LATCH);
        }
    }

    // -- state -----------------------------------------------------------

    /// The frame's part of a state snapshot.
    pub fn poll_fields(&self) -> PollFields {
        let polled = self.polled.unwrap_or(self.requests);
        PollFields {
            halted: self.halted,
            reset_pending: self.reset_pending,
            maskable_interrupt_pending: self.requests & IRQ_LINE != 0,
            non_maskable_interrupt_pending: self.requests & NMI_LATCH != 0,
            polled_i: self.polled_i.map(|i| i != 0),
            polled_maskable_interrupt: differs(polled, self.requests, IRQ_LINE),
            polled_non_maskable_interrupt: differs(polled, self.requests, NMI_LATCH),
            skipped_poll: polled & NO_POLL != 0,
        }
    }

    /// Restore the fields [`Frame::poll_fields`] captured.
    pub fn restore_poll(&mut self, state: &PollFields) {
        self.halted = state.halted;
        self.reset_pending = state.reset_pending;
        self.requests = (if state.maskable_interrupt_pending {
            IRQ_LINE
        } else {
            0
        }) | (if state.non_maskable_interrupt_pending {
            NMI_LATCH
        } else {
            0
        });
        let mut polled = self.requests;
        for (bit, seen) in [
            (IRQ_LINE, state.polled_maskable_interrupt),
            (NMI_LATCH, state.polled_non_maskable_interrupt),
        ] {
            if let Some(seen) = seen {
                polled = if seen { polled | bit } else { polled & !bit };
            }
        }
        if state.skipped_poll {
            polled |= NO_POLL;
        }
        self.polled = if polled == self.requests {
            None
        } else {
            Some(polled)
        };
        self.access();
        self.polled_i = state.polled_i.map(|i| if i { I } else { 0 });
    }
}

/// What the poll saw of `bit` when that is not what the host has requested, else `None`.
fn differs(polled: u8, requests: u8, bit: u8) -> Option<bool> {
    let seen = polled & bit != 0;
    if seen == (requests & bit != 0) {
        None
    } else {
        Some(seen)
    }
}

/// The frame's fields of a state snapshot (`CPUFrame._poll_fields`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PollFields {
    pub halted: bool,
    pub reset_pending: bool,
    pub maskable_interrupt_pending: bool,
    pub non_maskable_interrupt_pending: bool,
    pub polled_i: Option<bool>,
    pub polled_maskable_interrupt: Option<bool>,
    pub polled_non_maskable_interrupt: Option<bool>,
    pub skipped_poll: bool,
}

/// A 65xx-family part on the frame: its four sequences, and the step that
/// chooses between them (`CPUFrame.step`).
pub trait Part {
    fn frame(&self) -> &Frame;
    fn frame_mut(&mut self) -> &mut Frame;
    /// P, with the interrupt-disable flag in bit 2.
    fn status(&self) -> u8;

    // The part's own sequences; each returns the cycles it took.
    fn accept_reset(&mut self) -> u32;
    fn halted_cycle(&mut self) -> u32;
    fn enter_interrupt(&mut self, kind: u8) -> u32;
    fn execute(&mut self) -> u32;

    /// The interrupt this boundary takes; a part with another input (ABORT)
    /// overrides it.
    fn boundary_interrupt(&mut self) -> u8 {
        let p = self.status();
        self.frame_mut().boundary_interrupt(p)
    }

    /// Execute one instruction, interrupt entry or reset; return its cycle count.
    ///
    /// At an instruction boundary: a pending reset runs the start sequence; a
    /// halted CPU does what its part does while stopped; an NMI the last
    /// instruction's poll saw is taken; so is an IRQ it saw, if the I flag
    /// that poll saw is clear; otherwise one instruction runs.
    ///
    /// The poll sees the requests as they stood at the end of the
    /// instruction's second-to-last cycle, so one a bus callback makes
    /// during the last cycle waits for the next instruction's poll. One the
    /// host makes between steps is taken at the next boundary. BRK,
    /// interrupt entries and reset do not poll: one instruction runs after
    /// them before anything requested since is taken.
    fn step(&mut self) -> u32 {
        let frame = self.frame();
        if frame.reset_pending {
            return self.accept_reset();
        }
        if frame.halted {
            return self.halted_cycle();
        }
        // Nothing requested, nothing remembered: no interrupt to decide (the
        // usual boundary, so it skips the call).
        if frame.requests != 0 || frame.polled.is_some() || frame.polled_i.is_some() {
            let taken = self.boundary_interrupt();
            if taken != 0 {
                return self.enter_interrupt(taken);
            }
        }
        let cycles = self.execute();
        let frame = self.frame_mut();
        if let Some(deferred) = frame.deferred.take() {
            // A bus callback changed a request in the last cycle, after the poll.
            frame.polled = Some(deferred);
        }
        cycles
    }
}
