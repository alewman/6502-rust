//! Rung 3: Klaus Dormann's 6502 functional and decimal exercisers, run to
//! their end traps as the reference's `tests/test_dormann.py` runs them.
//!
//! Both are programs a real NMOS 6502 passes: every outcome ends in a `JMP *`
//! self-loop, and the loop's address says which.
//!
//!     dormann FUNCTIONAL_BIN DECIMAL_BIN

use sixfiveohtwo::{Bus, Frame, Mos6502};

/// The functional test's success trap (its listing, label `success`).
const FUNCTIONAL_SUCCESS: u16 = 0x3469;
/// The decimal test's ERROR byte: 1 while running and on failure, 0 on success.
const DECIMAL_ERROR: usize = 0x000B;
const BUDGET: u64 = 40_000_000;

struct Ram(Vec<u8>);

impl Bus for Ram {
    fn read(&mut self, address: u16, _: &mut Frame) -> u8 {
        self.0[address as usize]
    }
    fn write(&mut self, address: u16, value: u8, _: &mut Frame) {
        self.0[address as usize] = value;
    }
}

/// Run from `start` until the program jumps to itself; return the CPU and steps.
fn run_to_trap(image: &[u8], start: u16) -> (Mos6502<Ram>, u64) {
    let mut memory = image.to_vec();
    memory.resize(0x10000, 0);
    let mut cpu = Mos6502::new(Ram(memory));
    cpu.pc = start;
    let mut previous: i32 = -1;
    for steps in 0..BUDGET {
        let pc = cpu.pc;
        if pc as i32 == previous {
            return (cpu, steps);
        }
        previous = pc as i32;
        cpu.step();
    }
    panic!("no trap within {BUDGET} instructions; PC={:#06X}", cpu.pc);
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() != 2 {
        eprintln!("usage: dormann FUNCTIONAL_BIN DECIMAL_BIN");
        std::process::exit(2);
    }
    let functional = std::fs::read(&args[0]).expect("the functional test binary");
    let (cpu, steps) = run_to_trap(&functional, 0x0400);
    let functional_ok = cpu.pc == FUNCTIONAL_SUCCESS;
    println!(
        "functional: trap at ${:04X} after {steps} instructions ({})",
        cpu.pc,
        if functional_ok {
            "success"
        } else {
            "FAILED: see the test's listing"
        }
    );

    let mut decimal = std::fs::read(&args[1]).expect("the decimal test binary");
    decimal[DECIMAL_ERROR] = 0xA5; // the test must overwrite it: 1 while running, 0 on success
    let (cpu, steps) = run_to_trap(&decimal, 0x0400);
    let error = cpu.bus.0[DECIMAL_ERROR];
    println!(
        "decimal: trap at ${:04X} after {steps} instructions, ERROR = {error} ({})",
        cpu.pc,
        if error == 0 { "success" } else { "FAILED" }
    );
    std::process::exit(if functional_ok && error == 0 { 0 } else { 1 });
}
