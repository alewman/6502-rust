# Provenance

This crate is a transcription of
[6502-python](https://github.com/alewman/6502-python) at `741419a` (MIT,
same author), so its core inherits that repository's verdicts
(6502-python `docs/provenance.md`, audit wave 1, 2026-11-13: every part
**clean**, no GPL or LGPL code and no emulator source behind the core).
This file covers only what this repository adds.

| Part | From | Licence | Verdict |
| --- | --- | --- | --- |
| `src/*.rs` (core, frame, trace) | 6502-python, module by module, with its hardware comments | MIT (same family) | clean (inherited) |
| `src/bin/sst.rs`, `src/bin/dormann.rs`, `src/bin/trace.rs` | own runners for the reference's oracles | MIT | clean |
| `conformance/scenarios/*.json`, `scripts/make_scenarios.py` | hand-assembled programs written here | MIT | clean |
| `scripts/*.py`, `scripts/*.sh` (lockstep, seeded faults, rungs, fetch) | own work; run 6502-python's own reader and fetchers | MIT | clean |
| `tests/frame_part.rs` (a toy part on the frame) | own work | MIT | clean |

Fixtures: SingleStepTests 65x02 (MIT) and Klaus Dormann's / Bruce Clark's
tests (GPL-3.0, run as programs) are fetched by the reference's scripts into
the gitignored `external/`; nothing third-party is committed, in the tree or
in history (two commits, `76767e4` and `71747eb`).

Counts: 5 clean, 0 cite, 0 rewrite, 0 unclear. Owed, not a breach: SPDX
headers on each source file (charter rule 2).

`provenance_check.py` was not run: there is no permissive emulator source
behind this crate to compare against.
