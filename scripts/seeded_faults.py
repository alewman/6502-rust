#!/usr/bin/env python3
"""A gate that cannot fail proves nothing: build the crate with one fault at a
time and check that some rung catches each.

Each fault is one exact text replacement in a copy of ``src/``, built into its
own target directory; then the SingleStepTests sample (100 cases an opcode,
both sets), the Dormann programs and the rung-4 lockstep scenarios run
against it until one fails. A fault no rung catches is reported as MISSED.

    seeded_faults.py --vectors DIR --dormann DIR [--work DIR] [--only NAME]
"""

from __future__ import annotations

import argparse
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

# name: (file, exact text, replacement)
FAULTS = {
    "branch-stall-before": (
        "control.rs",
        "            early |= before;\n",
        "            let _ = before;\n",
    ),
    "request-every-change": (
        "frame.rs",
        "        if self.deferred.is_none() {\n            self.deferred = Some(self.requests);\n        }\n        self.requests = requests;",
        "        self.deferred = Some(self.requests);\n        self.requests = requests;",
    ),
    "cli-no-delay": (
        "control.rs",
        "        self.frame.polled_i = Some(self.p & I);\n        self.p &= !I;",
        "        self.p &= !I;",
    ),
    "no-nmi-hijack": (
        "control.rs",
        "if vector != VECTOR_NMI && self.frame.requests & NMI_LATCH != 0 {",
        "if false && vector != VECTOR_NMI && self.frame.requests & NMI_LATCH != 0 {",
    ),
    "dma-ignored": (
        "undocumented.rs",
        "let stored = if self.frame.dma_stalled { value } else { anded };",
        "let stored = anded;",
    ),
    "late-change-polled": (
        "frame.rs",
        "            frame.polled = Some(deferred);\n",
        "            let _ = deferred;\n",
    ),
    "decimal-z-from-bcd": (
        "alu.rs",
        "        if total >= 0x100 {\n            p |= C;\n        }\n        return (total as u8, p);",
        "        if total >= 0x100 {\n            p |= C;\n        }\n        if total & 0xFF == 0 { p |= Z; } else { p &= !Z; }\n        return (total as u8, p);",
    ),
    "page-cross-early-only": (
        "control.rs",
        "            self.frame.deferred = Some(early | deferred);",
        "            let _ = deferred;\n            self.frame.deferred = Some(early);",
    ),
    "note-stall-no-confirm": (
        "frame.rs",
        "        if self.deferred.is_some() {\n            self.access();\n        }",
        "",
    ),
    "brk-polls": (
        "control.rs",
        "        self.frame.deferred = Some(NO_POLL);\n    }",
        "    }",
    ),
}


def run(command: list[str]) -> bool:
    return subprocess.run(["nice", *command], capture_output=True, text=True).returncode == 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--vectors", type=Path, required=True)
    parser.add_argument("--dormann", type=Path, required=True)
    parser.add_argument("--work", type=Path, default=ROOT / "external" / "faults")
    parser.add_argument("--only")
    args = parser.parse_args()
    scenarios = sorted(str(p) for p in (ROOT / "conformance" / "scenarios").glob("*.json"))
    missed = 0
    for name, (file, old, new) in FAULTS.items():
        if args.only and name != args.only:
            continue
        tree = args.work / name
        if tree.exists():
            shutil.rmtree(tree)
        shutil.copytree(ROOT / "src", tree / "src")
        shutil.copy(ROOT / "Cargo.toml", tree / "Cargo.toml")
        shutil.copy(ROOT / "Cargo.lock", tree / "Cargo.lock")
        source = tree / "src" / file
        text = source.read_text()
        if text.count(old) != 1:
            print(f"{name}: the fault's text is not in {file} exactly once")
            return 2
        source.write_text(text.replace(old, new))
        target = args.work / "target"
        build = subprocess.run(
            ["nice", "cargo", "build", "--release", "-q", "--manifest-path", str(tree / "Cargo.toml"),
             "--target-dir", str(target)],
            capture_output=True, text=True,
        )
        if build.returncode:
            print(f"{name}: does not build\n{build.stderr[-800:]}")
            return 2
        binaries = target / "release"
        caught = None
        if not run([str(binaries / "sst"), str(args.vectors), "--limit", "100"]):
            caught = "SingleStepTests sample"
        elif not run([str(binaries / "dormann"),
                      str(args.dormann / "bin_files" / "6502_functional_test.bin"),
                      str(args.dormann / "6502_decimal_test.bin")]):
            caught = "Dormann"
        elif not run([sys.executable, str(ROOT / "scripts" / "lockstep.py"), *scenarios,
                      "--rust", str(binaries / "trace"), "--traces", str(args.work / "traces" / name)]):
            caught = "lockstep scenarios"
        print(f"{name}: {'caught by ' + caught if caught else 'MISSED'}", flush=True)
        missed += caught is None
    print(f"{len(FAULTS) - missed if not args.only else 1 - missed} caught, {missed} missed")
    return 1 if missed else 0


if __name__ == "__main__":
    raise SystemExit(main())
