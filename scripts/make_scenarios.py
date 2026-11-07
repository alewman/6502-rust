#!/usr/bin/env python3
"""Write the lockstep scenarios (rungs 1 and 4) into conformance/scenarios/.

Every program here is hand-assembled from the bytes below, so nothing
third-party is committed; the Dormann scenarios name the fetched binaries by
path and are written next to them (``--dormann DIR``), not committed.

    make_scenarios.py [--dormann DIR]

Rung 1: Klaus Dormann's functional and decimal tests, the first 300,000
boundaries of each on both parts, every bus access recorded.

Rung 4: the interrupt workout (CLI, SEI, PHP/PLP, BRK, taken branches with
and without a page crossing, IRQ and NMI handlers) under seeded random
requests: between steps, during a bus access, and during DMA stalls of reads
(`note_stall`, `dma_stalled`); resets; JAM then reset; SHA/SHX/SHY/TAS with
their cycle N stretched.
"""

from __future__ import annotations

import argparse
import json
import random
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "conformance" / "scenarios"


def workout() -> list[dict]:
    """The interrupt workout's memory: main loop at $02F0, handlers, vectors."""
    # Main loop at $02F0, so the branches at its start cross into page 3.
    main = bytes.fromhex(
        "58"        # 02F0 CLI
        "a9 00"     # 02F1 LDA #$00
        "18"        # 02F3 CLC
        "90 10"     # 02F4 BCC +$10 -> $0306 (taken, crosses into page 3)
        "ea ea ea ea ea ea ea ea ea ea ea ea ea ea ea ea"  # 02F6..0305 (skipped)
        "e6 10"     # 0306 INC $10
        "78"        # 0308 SEI
        "ea"        # 0309 NOP
        "58"        # 030A CLI
        "08"        # 030B PHP
        "78"        # 030C SEI
        "28"        # 030D PLP  (restores I clear, one boundary late)
        "69 01"     # 030E ADC #$01
        "d0 02"     # 0310 BNE +2 -> $0314 (taken, same page, unless A wrapped)
        "ea ea"     # 0312
        "f0 00"     # 0314 BEQ +0 (not taken unless zero)
        "08"        # 0316 PHP
        "58"        # 0317 CLI
        "28"        # 0318 PLP
        "00 ff"     # 0319 BRK (signature byte $FF) -> IRQ/BRK handler
        "a5 10"     # 031B LDA $10
        "c9 80"     # 031D CMP #$80
        "b0 03"     # 031F BCS +3 -> $0324
        "4c f0 02"  # 0321 JMP $02F0
        "a9 00"     # 0324 LDA #$00
        "85 10"     # 0326 STA $10
        "4c f0 02"  # 0328 JMP $02F0
    )
    irq = bytes.fromhex(
        "48"        # 0400 PHA
        "e6 20"     # 0401 INC $20
        "a5 20"     # 0403 LDA $20
        "29 07"     # 0405 AND #$07
        "d0 01"     # 0407 BNE +1
        "58"        # 0409 CLI  (nested interrupts now and then)
        "68"        # 040A PLA
        "40"        # 040B RTI
    )
    nmi = bytes.fromhex(
        "e6 21"     # 0500 INC $21
        "40"        # 0502 RTI
    )
    vectors = bytes.fromhex("0005" "f002" "0004")  # NMI $0500, RESET $02F0, IRQ $0400
    return [
        {"address": 0x02F0, "hex": main.hex()},
        {"address": 0x0400, "hex": irq.hex()},
        {"address": 0x0500, "hex": nmi.hex()},
        {"address": 0xFFFA, "hex": vectors.hex()},
    ]


def fuzz(seed: int, steps: int, accesses_per_step: float = 3.5) -> dict:
    """The workout under seeded random requests, stalls and resets."""
    rng = random.Random(seed)
    span = int(steps * accesses_per_step)
    events: list[dict] = []
    for access in range(span):
        roll = rng.random()
        if roll < 0.012:
            events.append({"access": access, "action": "irq"})
        elif roll < 0.024:
            events.append({"access": access, "action": "irq_clear"})
        elif roll < 0.030:
            events.append({"access": access, "action": "nmi"})
        elif roll < 0.031:
            events.append({"access": access, "action": "nmi_clear"})
        elif roll < 0.037:
            # Two changes in one cycle: the poll keeps the requests from
            # before the first (Frame.request), so a pulse is invisible.
            first, second = rng.choice((("irq", "irq_clear"), ("irq_clear", "irq"), ("nmi", "nmi_clear")))
            events.append({"access": access, "action": first})
            events.append({"access": access, "action": second})
    for step in range(steps):
        roll = rng.random()
        if roll < 0.01:
            events.append({"step": step, "action": "irq"})
        elif roll < 0.02:
            events.append({"step": step, "action": "irq_clear"})
        elif roll < 0.024:
            events.append({"step": step, "action": "nmi"})
        elif roll < 0.0245:
            events.append({"step": step, "action": "reset"})
    stalls = []
    for access in range(span):
        if rng.random() < 0.02:
            halted = rng.randint(1, 4)
            during = [
                {"cycle": rng.randint(1, halted), "action": rng.choice(("irq", "irq_clear", "nmi"))}
                for _ in range(rng.randint(0, 2))
            ]
            stalls.append({"access": access, "halted": halted, "during": during})
    return {
        "memory": workout(),
        "reset": True,
        "steps": steps,
        "events": events,
        "stalls": stalls,
    }


def jam_then_reset() -> dict:
    """JAM stops the CPU; a reset some boundaries later restarts it."""
    program = bytes.fromhex("e8 e8 02 e8")  # INX INX JAM INX
    return {
        "memory": [
            {"address": 0x0200, "hex": program.hex()},
            {"address": 0xFFFC, "hex": "0002"},
        ],
        "reset": True,
        "steps": 40,
        "events": [{"step": 20, "action": "reset"}, {"step": 25, "action": "nmi"}],
    }


def shx_under_dma() -> dict:
    """SHA, SHX, SHY and TAS, each run with and without its cycle N stretched."""
    # Each store at $0500 (no crossing) and $05F0+$20 (crossing), registers set first.
    program = bytes.fromhex(
        "a9 f5 a2 af a0 00"  # LDA #$F5 LDX #$AF LDY #$00
        "9c 00 05"  # SHY $0500,X
        "9e 00 05"  # SHX $0500,Y
        "9f 00 05"  # SHA $0500,Y
        "9b 00 05"  # TAS $0500,Y
        "a0 20"     # LDY #$20
        "9e f0 05"  # SHX $05F0,Y (crosses)
        "9f f0 05"  # SHA $05F0,Y (crosses)
        "a9 00 85 10 a9 05 85 11"  # ($10) = $0500
        "93 10"     # SHA ($10),Y
        "4c 00 02"  # JMP $0200
    )
    # Stall the cycle N of every other store: accesses counted from the reset.
    stalls = [{"access": n, "halted": 2} for n in range(9, 400, 7)]
    return {
        "memory": [
            {"address": 0x0200, "hex": program.hex()},
            {"address": 0xFFFC, "hex": "0002"},
        ],
        "reset": True,
        "steps": 120,
        "stalls": stalls,
    }


def dormann(directory: Path) -> dict[str, dict]:
    functional = directory / "bin_files" / "6502_functional_test.bin"
    decimal = directory / "6502_decimal_test.bin"
    scenarios = {}
    for part in ("6502", "rp2a03"):
        for name, path in (("functional", functional), ("decimal", decimal)):
            scenarios[f"dormann-{name}-{part}"] = {
                "part": part,
                "memory": [{"address": 0, "file": str(path.resolve())}],
                "registers": {"pc": 0x0400},
                "steps": 300_000,
            }
    return scenarios


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--dormann", type=Path, help="the fetched Dormann binaries' directory")
    args = parser.parse_args()
    OUT.mkdir(parents=True, exist_ok=True)
    scenarios = {
        "jam-then-reset": jam_then_reset(),
        "shx-under-dma": shx_under_dma(),
    }
    for seed in range(1, 9):
        scenarios[f"interrupts-{seed}"] = fuzz(seed, 20_000)
    scenarios["interrupts-rp2a03"] = {**fuzz(99, 20_000), "part": "rp2a03"}
    for name, scenario in scenarios.items():
        (OUT / f"{name}.json").write_text(json.dumps(scenario, separators=(",", ":")) + "\n")
    if args.dormann:
        external = ROOT / "external" / "scenarios"
        external.mkdir(parents=True, exist_ok=True)
        for name, scenario in dormann(args.dormann).items():
            (external / f"{name}.json").write_text(json.dumps(scenario) + "\n")
    print(f"wrote {len(scenarios)} scenarios to {OUT}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
