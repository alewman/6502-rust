# Lockstep scenarios

A scenario is a program, a host and a number of boundaries, run on both cores:
`target/release/trace` here and `scripts/reference_trace.py` on 6502-python
(its own `DebugSession`, access tracking on, written with its own
`write_trace`). `scripts/lockstep.py` runs both and compares the two traces
with the reference's `first_trace_divergence`; a scenario passes when it
prints `traces are identical`.

`scenarios/` holds rung 4's, written by `scripts/make_scenarios.py` from the
hand-assembled programs in it (nothing third-party). Rung 1's, which name the
fetched Dormann binaries by path, are written to `external/scenarios/`.

## Format

```json
{
  "part": "6502",
  "memory": [{"address": 512, "hex": "e8e802"}, {"address": 0, "file": "image.bin"}],
  "registers": {"pc": 1024},
  "reset": true,
  "steps": 40,
  "events": [{"step": 20, "action": "reset"}, {"access": 77, "action": "irq"}],
  "stalls": [{"access": 90, "halted": 3, "during": [{"cycle": 2, "action": "nmi"}]}],
  "accesses": true
}
```

| Key | Meaning |
| --- | --- |
| `part` | `"6502"` (`Mos6502`, the reference's `MOS6502`) or `"rp2a03"` (decimal mode disconnected). Default `"6502"`. |
| `memory` | 64 KB, zero except these blocks: `hex` bytes, or a `file` (relative to the scenario). |
| `registers` | Any of `a x y s pc p` set before the first step. |
| `reset` | Request a reset before the first step. |
| `steps` | Boundaries to run and record. |
| `events` | Host actions: `irq`, `irq_clear`, `nmi`, `nmi_clear`, `reset`. With `step: n`, made before boundary `n` (from 0). With `access: k`, made inside the bus callback of access `k` (counted from 0 over the whole run, reads and writes), before the access completes; several at one access run in the order listed. |
| `stalls` | Reads stretched by RDY (a DMA): the callback of access `k` calls `note_stall()`, then for each of the `halted` cycles makes that cycle's `during` actions and calls `note_stall()` again, then sets `dma_stalled`; every other read clears it, as famicom-python's board does. |
| `accesses` | Record every bus access in the trace (default true). |
