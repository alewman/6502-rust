//! Boundary traces in the reference's version 1 JSON Lines schema
//! (6502-python `docs/trace-schema.md`; `trace.py` and `debug.py`).
//!
//! A record carries the boundary kind, the cycles, the instruction's address
//! and bytes (no mnemonic: the reference disassembles the bytes itself), the
//! complete state before and after, and every bus access when recorded. The
//! reference's reader, `read_trace`, and `first_trace_divergence` compare a
//! trace written here with one its `DebugSession` wrote. Only writing is
//! transcribed; comparing is the reference's job.

use serde_json::{json, Map, Value};

use crate::core::I;
use crate::dispatch::{Mode, Op, OPCODES};
use crate::state::CpuState;

pub const TRACE_SCHEMA_VERSION: u32 = 1;

/// Kind of work performed by one `step()` call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoundaryKind {
    Instruction,
    /// A cycle spent stopped by JAM.
    JamIdle,
    Reset,
    Nmi,
    Irq,
}

impl BoundaryKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            BoundaryKind::Instruction => "instruction",
            BoundaryKind::JamIdle => "jam_idle",
            BoundaryKind::Reset => "reset",
            BoundaryKind::Nmi => "nmi",
            BoundaryKind::Irq => "irq",
        }
    }
}

/// What the next `step()` will do, decided exactly as `Mos6502::step()`
/// decides it (`debug.next_boundary`).
pub fn next_boundary(state: &CpuState) -> BoundaryKind {
    if state.reset_pending {
        return BoundaryKind::Reset;
    }
    if state.halted {
        return BoundaryKind::JamIdle;
    }
    if state.skipped_poll {
        return BoundaryKind::Instruction;
    }
    let nmi = state
        .polled_non_maskable_interrupt
        .unwrap_or(state.non_maskable_interrupt_pending);
    if nmi {
        return BoundaryKind::Nmi;
    }
    let irq = state
        .polled_maskable_interrupt
        .unwrap_or(state.maskable_interrupt_pending);
    let masked = state.polled_i.unwrap_or(state.p & I != 0);
    if irq && !masked {
        return BoundaryKind::Irq;
    }
    BoundaryKind::Instruction
}

/// How many bytes the instruction with this opcode occupies, as the
/// reference's disassembler decodes it (`disasm._LENGTHS` by its mode).
pub fn instruction_length(opcode: u8) -> usize {
    let (op, mode, _) = OPCODES[opcode as usize];
    match mode {
        Mode::Implied => match op {
            Op::Bpl | Op::Bmi | Op::Bvc | Op::Bvs | Op::Bcc | Op::Bcs | Op::Bne | Op::Beq => 2,
            Op::Jmp | Op::Jsr | Op::JmpInd => 3,
            _ => 1,
        },
        Mode::Imm | Mode::Zp | Mode::Zpx | Mode::Zpy | Mode::Izx | Mode::Izy | Mode::IzyW => 2,
        Mode::Abs | Mode::Abx | Mode::Aby | Mode::AbxW | Mode::AbyW => 3,
    }
}

/// A bus access as the trace records it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    Read(u16, u8),
    Write(u16, u8),
}

/// One processor boundary.
#[derive(Clone, Debug)]
pub struct StepRecord {
    pub sequence: u64,
    pub kind: BoundaryKind,
    pub cycles: u32,
    /// The instruction's address and bytes, for an instruction boundary.
    pub instruction: Option<(u16, Vec<u8>)>,
    pub before: CpuState,
    pub after: CpuState,
    pub accesses: Option<Vec<Access>>,
}

fn state_value(state: &CpuState) -> Value {
    json!({
        "a": state.a,
        "x": state.x,
        "y": state.y,
        "s": state.s,
        "pc": state.pc,
        "p": state.p,
        "halted": state.halted,
        "reset_pending": state.reset_pending,
        "maskable_interrupt_pending": state.maskable_interrupt_pending,
        "non_maskable_interrupt_pending": state.non_maskable_interrupt_pending,
        "polled_i": state.polled_i,
        "polled_maskable_interrupt": state.polled_maskable_interrupt,
        "polled_non_maskable_interrupt": state.polled_non_maskable_interrupt,
        "skipped_poll": state.skipped_poll,
    })
}

impl StepRecord {
    /// The versioned JSON representation (keys sorted, as the reference writes them).
    pub fn to_value(&self) -> Value {
        let mut record = Map::new();
        record.insert("version".into(), json!(TRACE_SCHEMA_VERSION));
        record.insert("sequence".into(), json!(self.sequence));
        record.insert("kind".into(), json!(self.kind.as_str()));
        record.insert("cycles".into(), json!(self.cycles));
        let instruction = match &self.instruction {
            None => Value::Null,
            Some((address, data)) => {
                let hex: String = data.iter().map(|b| format!("{b:02x}")).collect();
                json!({"address": address, "data": hex})
            }
        };
        record.insert("instruction".into(), instruction);
        record.insert("before".into(), state_value(&self.before));
        record.insert("after".into(), state_value(&self.after));
        if let Some(accesses) = &self.accesses {
            let list: Vec<Value> = accesses
                .iter()
                .map(|access| match *access {
                    Access::Read(address, value) => json!(["r", address, value]),
                    Access::Write(address, value) => json!(["w", address, value]),
                })
                .collect();
            record.insert("accesses".into(), Value::Array(list));
        }
        Value::Object(record)
    }

    /// One JSON Lines line, without the newline.
    pub fn to_line(&self) -> String {
        self.to_value().to_string()
    }
}
