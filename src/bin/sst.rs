//! Rung 2: the SingleStepTests 65x02 corpus, the "6502" set on `Mos6502` and
//! the "nes6502" set on the RP2A03, every case checked as the reference's
//! `tests/single_step.py` checks it: registers, P (B excluded: the corpus
//! records whatever B its generator chose, and B is not a flip-flop), every
//! RAM byte the case names, the cycle count, and the bus access by access.
//!
//!     sst CORPUS_DIR [SET ...] [--limit N] [--jobs N]
//!
//! `CORPUS_DIR` holds `6502/v1/*.json` and `nes6502/v1/*.json`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use serde::Deserialize;
use sixfiveohtwo::{Bus, Frame, Mos6502};

#[derive(Deserialize)]
struct Snapshot {
    pc: u16,
    s: u8,
    a: u8,
    x: u8,
    y: u8,
    p: u8,
    ram: Vec<(u16, u8)>,
}

#[derive(Deserialize)]
struct Case {
    name: String,
    initial: Snapshot,
    #[serde(rename = "final")]
    end: Snapshot,
    cycles: Vec<(u16, u8, String)>,
}

struct Ram {
    memory: Vec<u8>,
    accesses: Vec<(u16, u8, bool)>,
}

impl Bus for Ram {
    fn read(&mut self, address: u16, _: &mut Frame) -> u8 {
        let value = self.memory[address as usize];
        self.accesses.push((address, value, false));
        value
    }
    fn write(&mut self, address: u16, value: u8, _: &mut Frame) {
        self.accesses.push((address, value, true));
        self.memory[address as usize] = value;
    }
}

/// Run one case; return what disagrees (empty when it passes).
fn run_case(case: &Case, cpu: &mut Mos6502<Ram>) -> Vec<String> {
    let initial = &case.initial;
    for &(address, value) in &initial.ram {
        cpu.bus.memory[address as usize] = value;
    }
    cpu.bus.accesses.clear();
    cpu.frame = Frame::default();
    cpu.a = initial.a;
    cpu.x = initial.x;
    cpu.y = initial.y;
    cpu.s = initial.s;
    cpu.pc = initial.pc;
    cpu.p = (initial.p | 0x20) & !0x10;

    let cycles = cpu.step();

    let end = &case.end;
    let mut wrong = Vec::new();
    for (name, got, expected) in [
        ("a", cpu.a as u16, end.a as u16),
        ("x", cpu.x as u16, end.x as u16),
        ("y", cpu.y as u16, end.y as u16),
        ("s", cpu.s as u16, end.s as u16),
        ("pc", cpu.pc, end.pc),
    ] {
        if got != expected {
            wrong.push(format!("{name} is {got:#x}, expected {expected:#x}"));
        }
    }
    if (cpu.p ^ end.p) & !0x10 != 0 {
        wrong.push(format!(
            "p (B excluded) is {:#x}, expected {:#x}",
            cpu.p, end.p
        ));
    }
    for &(address, value) in &end.ram {
        let got = cpu.bus.memory[address as usize];
        if got != value {
            wrong.push(format!(
                "ram[{address:#06x}] is {got:#x}, expected {value:#x}"
            ));
        }
    }
    if cycles as usize != case.cycles.len() {
        wrong.push(format!(
            "cycles is {cycles}, expected {}",
            case.cycles.len()
        ));
    }
    let expected: Vec<(u16, u8, bool)> = case
        .cycles
        .iter()
        .map(|(address, value, kind)| (*address, *value, kind == "write"))
        .collect();
    if cpu.bus.accesses != expected {
        wrong.push(format!(
            "bus is {:?}, expected {:?}",
            cpu.bus.accesses, expected
        ));
    }
    // Leave memory clean for the next case: zero every byte this one named.
    for &(address, _) in initial.ram.iter().chain(end.ram.iter()) {
        cpu.bus.memory[address as usize] = 0;
    }
    for &(address, _, _) in &cpu.bus.accesses {
        cpu.bus.memory[address as usize] = 0;
    }
    wrong
}

fn run_file(path: &Path, nes: bool, limit: Option<usize>) -> (usize, usize, Vec<String>) {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let cases: Vec<Case> =
        serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let ram = Ram {
        memory: vec![0; 0x10000],
        accesses: Vec::with_capacity(16),
    };
    let mut cpu = if nes {
        Mos6502::rp2a03(ram)
    } else {
        Mos6502::new(ram)
    };
    let (mut passed, mut failed, mut first) = (0, 0, Vec::new());
    for case in cases.iter().take(limit.unwrap_or(usize::MAX)) {
        let wrong = run_case(case, &mut cpu);
        if wrong.is_empty() {
            passed += 1;
        } else {
            failed += 1;
            if first.len() < 3 {
                first.push(format!("{}: {}", case.name, wrong.join("; ")));
            }
        }
    }
    (passed, failed, first)
}

fn main() {
    let mut args = std::env::args().skip(1);
    let mut corpus: Option<PathBuf> = None;
    let mut sets = Vec::new();
    let mut limit = None;
    let mut jobs = 8usize;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--limit" => limit = Some(args.next().expect("--limit N").parse().expect("a number")),
            "--jobs" => jobs = args.next().expect("--jobs N").parse().expect("a number"),
            _ if corpus.is_none() => corpus = Some(PathBuf::from(arg)),
            _ => sets.push(arg),
        }
    }
    let corpus = corpus.expect("usage: sst CORPUS_DIR [SET ...] [--limit N] [--jobs N]");
    if sets.is_empty() {
        sets = vec!["6502".into(), "nes6502".into()];
    }
    let mut all_passed = true;
    for set in &sets {
        let nes = match set.as_str() {
            "6502" => false,
            "nes6502" => true,
            other => panic!("unknown set {other}: 6502 or nes6502"),
        };
        if !corpus.join(set).join("v1").join("a9.json").is_file() {
            eprintln!(
                "{}: no {set} corpus; fetch it with the reference's scripts/fetch_test_vectors.py",
                corpus.display()
            );
            std::process::exit(2);
        }
        let files: Vec<PathBuf> = (0..256)
            .map(|opcode| {
                corpus
                    .join(set)
                    .join("v1")
                    .join(format!("{opcode:02x}.json"))
            })
            .collect();
        let next = AtomicUsize::new(0);
        let totals = Mutex::new((0usize, 0usize, Vec::<String>::new()));
        std::thread::scope(|scope| {
            for _ in 0..jobs.max(1) {
                scope.spawn(|| loop {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    if index >= files.len() {
                        break;
                    }
                    let (passed, failed, first) = run_file(&files[index], nes, limit);
                    let mut totals = totals.lock().unwrap();
                    totals.0 += passed;
                    totals.1 += failed;
                    totals.2.extend(first);
                });
            }
        });
        let (passed, failed, first) = totals.into_inner().unwrap();
        for line in first.iter().take(20) {
            println!("  {line}");
        }
        println!(
            "{set}: {passed} passed, {failed} failed / {} cases",
            passed + failed
        );
        all_passed &= failed == 0;
    }
    std::process::exit(if all_passed { 0 } else { 1 });
}
