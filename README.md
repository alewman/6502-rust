# 6502-rust

A Rust NMOS 6502 core transcribed from
[6502-python](https://github.com/alewman/6502-python) and proven equivalent
to it at every processor boundary: boundary kind, cycles, every bus access in
order, and all of `CPUState`, not merely "passes the functional test at the
end". It is the CPU of the Rust Atari 2600 board (`a2600-rust`), and carries
what a Rust NES board needs (the RP2A03, the DMA and stall hooks).

The crate is `sixfiveohtwo`, the reference's import name (Cargo does not
allow a package name that starts with a digit).

```rust
use sixfiveohtwo::{Bus, Frame, Mos6502};

struct Ram(Vec<u8>);
impl Bus for Ram {
    fn read(&mut self, address: u16, _: &mut Frame) -> u8 { self.0[address as usize] }
    fn write(&mut self, address: u16, value: u8, _: &mut Frame) { self.0[address as usize] = value; }
}

let mut cpu = Mos6502::new(Ram(vec![0; 0x10000]));
cpu.frame.request_reset();
let cycles = cpu.step(); // 7: the start sequence, one bus call per cycle
```

Every cycle is one bus call, dummy reads and read-modify-write dummy writes
included, in the chip's order. Each call also gets the CPU's `Frame`, so a
device can raise or drop IRQ and NMI during the cycle it happens in, say that
RDY is stretching a read (`note_stall`), or set `dma_stalled` (SHA, SHX, SHY
and TAS drop their AND after a stretched cycle N).

## Layout

Each `src/*.rs` file is the transcription of one `sixfiveohtwo/*.py` module,
with the same handler names, the same 256-entry dispatch table (generated
from the reference's) and the hardware comments carried over: `frame`
(`CPUFrame`), `core`, `alu`, `control`, `loads`, `shifts`, `undocumented`,
`dispatch`, `cpu`, `state`, `trace`. Where Rust forced a difference, the
module header says so; the one that matters is the bus (a callback cannot
reach the CPU while the CPU is calling it, so the bus gets `&mut Frame`).

**The 65xx-family base.** `frame::Frame` and the `frame::Part` trait are the
reference's `CPUFrame`: the interrupt requests, the poll (sampled at the end
of an instruction's second-to-last cycle; CLI, SEI and PLP change I after it;
BRK, interrupt entries and reset do not poll) and `step()`'s skeleton. A core
for another part (a 65C816, as `65816-python` builds on the Python frame)
implements `Part`'s four hooks and inherits none of the NMOS handlers;
`tests/frame_part.rs` does exactly that with a toy part.

Not transcribed: the reference's debugger, command console and disassembler
(Python-side tooling). A trace written here carries each instruction's
address and bytes, and the reference's reader disassembles them.

## Certification

| What | Version |
| --- | --- |
| 6502-python (the reference) | main at `741419a` (the merge of PR #9: tooling hooks; with the DMA and stall hooks of PR #7). The revision a2600-python pins (tag `a2600-rust-pin-1`) |
| Trace schema | version 1 (6502-python `docs/trace-schema.md`) |
| SingleStepTests 65x02 | revision `2f6980a2d95757486c7bee24355c360e40e2a224` |
| Klaus Dormann's tests | functional `6502_functional_test.bin` SHA-256 `fa12bfc7…01953fd`; decimal (amb5l's ca65 port, assembled with cc65 2.19) SHA-256 `b179ca4c…6ca43f` |

Ladder status at this commit, each rung reproduced with the script named on
Linux x86_64, rustc 1.93.1, PyPy 7.3.20 (Python 3.11):

| Rung | What | Result | Script |
| --- | --- | --- | --- |
| 1 | Dormann's functional and decimal tests in lockstep with the reference, first 300,000 boundaries of each, on both parts, every bus access compared | 4 of 4 scenarios `traces are identical` | `rung1.sh` |
| 2 | SingleStepTests: "6502" on `Mos6502`, "nes6502" on the RP2A03: registers, P (B excluded, as the reference does), RAM, cycles and every bus cycle | `6502: 2560000 passed, 0 failed`; `nes6502: 2560000 passed, 0 failed` | `rung2.sh` |
| 3 | Dormann natively | functional: trap at `$3469` after 30,646,177 instructions (the reference's count); decimal: trap at `$044B`, ERROR = 0 | `rung3.sh` |
| 4 | Interrupts, resets, JAM and DMA stalls in lockstep: the hand-assembled interrupt workout under seeded random IRQ/NMI requests between steps and inside bus cycles (two changes in one cycle included), stalled reads with requests in their halted cycles, resets; JAM then reset; SHA/SHX/SHY/TAS with cycle N stretched | 11 of 11 scenarios `traces are identical` (one workout: 1,353 IRQs, 706 NMIs, 11 resets in 20,000 boundaries) | `rung4.sh` |
| 5 | The frame carries a part that is not the NMOS 6502 | 9 tests pass | `rung5.sh` |

**Seeded faults** (`scripts/seeded_faults.py`): ten one-line faults in the
poll, the branch's poll, the stall hooks, CLI's delay, NMI hijacking, BRK's
skipped poll, the DMA AND and decimal flags, each built and run through the
rungs: **10 caught, 0 missed**. Nine are caught only by the lockstep
scenarios (SingleStepTests runs one instruction with no interrupts); the
first run missed one (a request changed twice in one cycle), and rung 4 now
makes such pairs.

CI (`.github/workflows/ci.yml`) runs `cargo fmt --check`, `cargo clippy -D
warnings` and `cargo test` (rung 5), and rungs 1, 3 and 4 with rung 2 on a
sample of 100 cases an opcode, on every push to main and every pull request.

### Reproducing

```text
PYTHON=pypy3 scripts/fetch_6502python.sh       # 6502-python at 741419a into external/, with a venv
external/6502-python/.venv/bin/python external/6502-python/scripts/fetch_test_vectors.py
external/6502-python/.venv/bin/python external/6502-python/scripts/fetch_dormann_tests.py   # decimal needs cc65's ca65
scripts/rung1.sh
scripts/rung2.sh
scripts/rung3.sh
scripts/rung4.sh
scripts/rung5.sh
scripts/seeded_faults.py --vectors external/6502-python/tests/6502_test_vectors \
    --dormann external/6502-python/tests/dormann
```

`docs/handoff-brief.md` is the brief this was built from.

## Licence

MIT (see `LICENSE`). Nothing third-party is committed: the vectors and the
Dormann binaries are fetched by the reference's own scripts.
