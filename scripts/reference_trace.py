#!/usr/bin/env python3
"""Run a scenario on 6502-python and write its boundary trace (the reference's half).

The same scenario format as ``src/bin/trace.rs`` (conformance/README.md),
run on the reference core through its own ``DebugSession`` with access
tracking, written with its own ``write_trace``. Run it with the reference's
interpreter (``external/6502-python/.venv/bin/python``, from
``scripts/fetch_6502python.sh``).

    reference_trace.py SCENARIO.json [--out TRACE.jsonl]
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

from sixfiveohtwo import MOS6502, RP2A03, DebugSession
from sixfiveohtwo.trace import write_trace


def act(cpu: MOS6502, action: str) -> None:
    {
        "irq": cpu.request_maskable_interrupt,
        "irq_clear": cpu.clear_maskable_interrupt,
        "nmi": cpu.request_non_maskable_interrupt,
        "nmi_clear": cpu.clear_non_maskable_interrupt,
        "reset": cpu.request_reset,
    }[action]()


class Host:
    """Flat memory, the scenario's in-cycle requests and its DMA stalls."""

    def __init__(self, memory: bytearray, during: dict, stalls: dict) -> None:
        self.memory = memory
        self.during = during
        self.stalls = stalls
        self.count = 0
        self.cpu: MOS6502 | None = None

    def read_byte(self, address: int) -> int:
        index = self.count
        self.count += 1
        cpu = self.cpu
        for action in self.during.get(index, ()):
            act(cpu, action)
        stall = self.stalls.get(index)
        if stall is not None:
            # RDY holds this read: note_stall as the stall begins and after
            # each halted cycle, the halted cycles' requests between.
            cpu.note_stall()
            for cycle in range(1, stall["halted"] + 1):
                for event in stall.get("during", ()):
                    if event["cycle"] == cycle:
                        act(cpu, event["action"])
                cpu.note_stall()
            cpu.dma_stalled = True
        else:
            cpu.dma_stalled = False
        return self.memory[address]

    def write_byte(self, address: int, value: int) -> None:
        index = self.count
        self.count += 1
        for action in self.during.get(index, ()):
            act(self.cpu, action)
        self.memory[address] = value


def records(path: Path):
    scenario = json.loads(path.read_text())
    memory = bytearray(0x10000)
    for block in scenario["memory"]:
        if "hex" in block:
            data = bytes.fromhex(block["hex"])
        else:
            file = Path(block["file"])
            data = (file if file.is_absolute() else path.parent / file).read_bytes()
        for offset, byte in enumerate(data):
            memory[(block["address"] + offset) & 0xFFFF] = byte
    during: dict[int, list[str]] = {}
    between: dict[int, list[str]] = {}
    for event in scenario.get("events", ()):
        if "step" in event:
            between.setdefault(event["step"], []).append(event["action"])
        else:
            during.setdefault(event["access"], []).append(event["action"])
    stalls = {stall["access"]: stall for stall in scenario.get("stalls", ())}
    host = Host(memory, during, stalls)
    part = {"6502": MOS6502, "rp2a03": RP2A03}[scenario.get("part", "6502")]
    cpu = part(host.read_byte, host.write_byte)
    host.cpu = cpu
    for name, value in scenario.get("registers", {}).items():
        setattr(cpu, name, value)
    if scenario.get("reset"):
        cpu.request_reset()
    session = DebugSession(
        cpu, peek_byte=memory.__getitem__, track_accesses=scenario.get("accesses", True)
    )
    for sequence in range(scenario["steps"]):
        for action in between.get(sequence, ()):
            act(cpu, action)
        yield session.step()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("scenario", type=Path)
    parser.add_argument("--out", type=Path)
    args = parser.parse_args()
    stream = args.out.open("w") if args.out else sys.stdout
    with stream:
        write_trace(records(args.scenario), stream)
    return 0


if __name__ == "__main__":
    sys.exit(main())
