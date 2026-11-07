#!/usr/bin/env bash
# Rung 4: interrupts, resets, JAM and DMA stalls in lockstep with the reference
# (conformance/scenarios/, written by scripts/make_scenarios.py).
# Usage: scripts/rung4.sh [REFERENCE_PYTHON]
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
PYTHON=${1:-"$ROOT/external/6502-python/.venv/bin/python"}
exec python3 "$ROOT/scripts/lockstep.py" "$ROOT"/conformance/scenarios/*.json --python "$PYTHON" --jobs "${JOBS:-8}"
