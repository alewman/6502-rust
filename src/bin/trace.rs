//! Run a scenario and write its boundary trace (rungs 1 and 4).
//!
//!     trace SCENARIO.json [--out TRACE.jsonl]
//!
//! The scenario format (`conformance/README.md`) is read the same way by
//! `scripts/reference_trace.py`, which runs it on 6502-python; the two
//! traces are compared with the reference's `first_trace_divergence`.

use std::collections::HashMap;
use std::io::{BufWriter, Write};

use serde::Deserialize;
use sixfiveohtwo::trace::{instruction_length, next_boundary, Access, BoundaryKind, StepRecord};
use sixfiveohtwo::{Bus, Frame, Mos6502};

#[derive(Deserialize)]
struct Block {
    address: u16,
    #[serde(default)]
    hex: Option<String>,
    #[serde(default)]
    file: Option<String>,
}

#[derive(Deserialize, Default)]
struct Registers {
    a: Option<u8>,
    x: Option<u8>,
    y: Option<u8>,
    s: Option<u8>,
    pc: Option<u16>,
    p: Option<u8>,
}

#[derive(Deserialize, Clone)]
struct Event {
    #[serde(default)]
    step: Option<u64>,
    #[serde(default)]
    access: Option<u64>,
    action: String,
}

#[derive(Deserialize, Clone)]
struct Stall {
    access: u64,
    halted: u32,
    #[serde(default)]
    during: Vec<HaltedEvent>,
}

#[derive(Deserialize, Clone)]
struct HaltedEvent {
    cycle: u32,
    action: String,
}

#[derive(Deserialize)]
struct Scenario {
    #[serde(default = "default_part")]
    part: String,
    memory: Vec<Block>,
    #[serde(default)]
    registers: Registers,
    #[serde(default)]
    reset: bool,
    steps: u64,
    #[serde(default)]
    events: Vec<Event>,
    #[serde(default)]
    stalls: Vec<Stall>,
    #[serde(default = "default_true")]
    accesses: bool,
}

fn default_part() -> String {
    "6502".into()
}

fn default_true() -> bool {
    true
}

fn act(frame: &mut Frame, action: &str) {
    match action {
        "irq" => frame.request_maskable_interrupt(),
        "irq_clear" => frame.clear_maskable_interrupt(),
        "nmi" => frame.request_non_maskable_interrupt(),
        "nmi_clear" => frame.clear_non_maskable_interrupt(),
        "reset" => frame.request_reset(),
        other => panic!("unknown action {other}"),
    }
}

struct Host {
    memory: Vec<u8>,
    count: u64,
    log: Option<Vec<Access>>,
    during: HashMap<u64, Vec<String>>,
    stalls: HashMap<u64, Stall>,
}

impl Bus for Host {
    fn read(&mut self, address: u16, frame: &mut Frame) -> u8 {
        let index = self.count;
        self.count += 1;
        if let Some(actions) = self.during.get(&index) {
            for action in actions {
                act(frame, action);
            }
        }
        if let Some(stall) = self.stalls.get(&index) {
            // RDY holds this read: note_stall as the stall begins and after
            // each halted cycle, the halted cycles' requests between.
            frame.note_stall();
            for cycle in 1..=stall.halted {
                for event in stall.during.iter().filter(|e| e.cycle == cycle) {
                    act(frame, &event.action);
                }
                frame.note_stall();
            }
            frame.dma_stalled = true;
        } else {
            frame.dma_stalled = false;
        }
        let value = self.memory[address as usize];
        if let Some(log) = &mut self.log {
            log.push(Access::Read(address, value));
        }
        value
    }

    fn write(&mut self, address: u16, value: u8, frame: &mut Frame) {
        let index = self.count;
        self.count += 1;
        if let Some(actions) = self.during.get(&index) {
            for action in actions {
                act(frame, action);
            }
        }
        if let Some(log) = &mut self.log {
            log.push(Access::Write(address, value));
        }
        self.memory[address as usize] = value;
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args
        .next()
        .expect("usage: trace SCENARIO.json [--out TRACE.jsonl]");
    let mut out: Box<dyn Write> = Box::new(std::io::stdout().lock());
    while let Some(arg) = args.next() {
        if arg == "--out" {
            let file = std::fs::File::create(args.next().expect("--out PATH")).expect("create");
            out = Box::new(file);
        }
    }
    let mut out = BufWriter::new(out);
    let base = std::path::Path::new(&path).parent().unwrap().to_path_buf();
    let scenario: Scenario =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("parse");

    let mut memory = vec![0u8; 0x10000];
    for block in &scenario.memory {
        let bytes = match (&block.hex, &block.file) {
            (Some(hex), None) => (0..hex.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).expect("hex"))
                .collect::<Vec<u8>>(),
            (None, Some(file)) => {
                let file = std::path::Path::new(file);
                let file = if file.is_absolute() {
                    file.to_path_buf()
                } else {
                    base.join(file)
                };
                std::fs::read(&file).unwrap_or_else(|e| panic!("{}: {e}", file.display()))
            }
            _ => panic!("a memory block has hex or file"),
        };
        for (offset, byte) in bytes.iter().enumerate() {
            memory[(block.address as usize + offset) & 0xFFFF] = *byte;
        }
    }
    let mut during: HashMap<u64, Vec<String>> = HashMap::new();
    let mut between: HashMap<u64, Vec<String>> = HashMap::new();
    for event in &scenario.events {
        match (event.step, event.access) {
            (Some(step), None) => between.entry(step).or_default().push(event.action.clone()),
            (None, Some(access)) => during.entry(access).or_default().push(event.action.clone()),
            _ => panic!("an event has step or access"),
        }
    }
    let stalls = scenario
        .stalls
        .iter()
        .map(|s| (s.access, s.clone()))
        .collect();
    let host = Host {
        memory,
        count: 0,
        log: if scenario.accesses {
            Some(Vec::new())
        } else {
            None
        },
        during,
        stalls,
    };
    let mut cpu = match scenario.part.as_str() {
        "6502" => Mos6502::new(host),
        "rp2a03" => Mos6502::rp2a03(host),
        other => panic!("unknown part {other}"),
    };
    let r = &scenario.registers;
    cpu.a = r.a.unwrap_or(cpu.a);
    cpu.x = r.x.unwrap_or(cpu.x);
    cpu.y = r.y.unwrap_or(cpu.y);
    cpu.s = r.s.unwrap_or(cpu.s);
    cpu.pc = r.pc.unwrap_or(cpu.pc);
    cpu.p = r.p.unwrap_or(cpu.p);
    if scenario.reset {
        cpu.frame.request_reset();
    }

    for sequence in 0..scenario.steps {
        if let Some(actions) = between.get(&sequence) {
            for action in actions.clone() {
                act(&mut cpu.frame, &action);
            }
        }
        let before = cpu.capture_state();
        let kind = next_boundary(&before);
        let instruction = if kind == BoundaryKind::Instruction {
            let pc = before.pc;
            let length = instruction_length(cpu.bus.memory[pc as usize]);
            let data = (0..length)
                .map(|n| cpu.bus.memory[pc.wrapping_add(n as u16) as usize])
                .collect();
            Some((pc, data))
        } else {
            None
        };
        if let Some(log) = &mut cpu.bus.log {
            log.clear();
        }
        let cycles = cpu.step();
        let after = cpu.capture_state();
        let record = StepRecord {
            sequence,
            kind,
            cycles,
            instruction,
            before,
            after,
            accesses: cpu.bus.log.clone(),
        };
        writeln!(out, "{}", record.to_line()).expect("write");
    }
}
