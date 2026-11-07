# Handoff brief: 6502-rust

## The request

Aubrey, 2026-11-07, after a2600-python's seven milestones: build the Rust
version of the Atari 2600 board, then its fast fork. The 2600 needs a Rust
6502 core first, and none exists. Decisions given: both `6502-rust` and
`a2600-rust` are **public**; the core **includes the 65xx-family base class**
(6502-python's `CPUFrame`) so a Rust 65C816 core can later build on it as
`65816-python` builds on the Python one.

## What it is

A Rust NMOS 6502 core transcribed from
[6502-python](https://github.com/alewman/6502-python) at **`741419a`** (main,
the merge of PR #9) and proven equivalent to it at every processor boundary:
boundary kind, cycles, every bus access in order, and all of `CPUState`. The
same revision is what a2600-python pins (tag `a2600-rust-pin-1`), so
`a2600-rust` can sit on this crate.

The crate is `sixfiveohtwo` (the reference's import name; Cargo does not
allow a package name that starts with a digit).

## What to transcribe

From `src/sixfiveohtwo/` at `741419a`, module for module, with the same
names, the same explicit dispatch and the hardware comments carried over:

- `frame.py`: `CPUFrame`, the base class: the two bus callables, the
  interrupt requests and their poll (sampled at the end of an instruction's
  second-to-last cycle; CLI/SEI/PLP change I after the poll; BRK, interrupt
  entries and reset do not poll), and `step()`'s skeleton. In Rust: a frame
  struct and a trait carrying the four hooks (`accept_reset`, `halted`,
  `enter_interrupt`, `execute`), so another core supplies its own
  instruction set and inherits none of the NMOS handlers.
- `_core.py`, `_alu.py`, `_control.py`, `_loads.py`, `_shifts.py`,
  `_undocumented.py`, `_dispatch.py`, `cpu.py`: `MOS6502` and `RP2A03` (the
  NES CPU: decimal mode disconnected).
- The host hooks the NES board relies on (the famicom-python session's
  request, 2026-11-07): `dma_stalled` and `note_stall` (SHA/SHX/SHY/TAS drop
  their AND when a DMA stretched the read; a stalled read leaves the
  requests it found for a branch's poll), and one bus callback per cycle,
  dummy accesses included.
- `state.py` (`CPUState`) and `trace.py` (the trace schema, version 1,
  `docs/trace-schema.md`), so both cores write the same JSON Lines.

Not transcribed (Python-side tooling): `debug.py`, `console.py`,
`disasm.py`; a trace record's `instruction` may be `null` (the schema allows
it), and comparison ignores it.

## The ladder

Each rung has a script in `scripts/` and passes only when its output says so.

1. **Trace lockstep, short**: the same programs on both cores, traces
   compared by 6502-python's own `first_trace_divergence`. Identical.
2. **SingleStepTests 65x02** at `2f6980a`: the "6502" set on `MOS6502` and
   the "nes6502" set on `RP2A03`, 2,560,000 cases each: registers, RAM and
   every bus cycle's address, value and direction. All pass.
3. **Klaus Dormann's functional test** (success trap `$3469` after
   30,646,177 instructions) **and the decimal test** (`ERROR` = 0 at
   `$044B`), natively, binaries by SHA-256 as 6502-python fetches them.
4. **Interrupts and stalls in lockstep**: IRQ/NMI timing, the poll cases of
   `tests/test_interrupt_polling.py` and `tests/test_dma_stall.py` as traced
   scenarios, diffed against the reference.
5. **The base class carries a second core**: a minimal test core on the
   frame alone (its own `execute`) steps, takes interrupts and polls as the
   frame specifies, with no NMOS code linked in.

CI reproduces rungs 1, 2 (a sample), 3 and 5 on every push, plus `cargo fmt
--check`, `cargo clippy -D warnings` and `cargo test`. Keep it small (main
and pull requests, concurrency cancel).

## Working rules

- Do not modify 6502-python. If the reference looks wrong, stop and show the
  divergence; the vectors and Dormann are the authority, not either core.
- Transcribe; do not redesign. Where Rust forces a difference, say so in the
  module header.
- Never copy GPL code; nothing third-party is committed (vectors and test
  binaries are fetched by script).
- `nice`, at most about 16 processes on the shared machine.
- Commit trailer: `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
  Push when Aubrey asks; the repository is public once created.
