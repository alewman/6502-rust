#!/usr/bin/env bash
# Rung 1: Klaus Dormann's functional and decimal tests in lockstep with the
# reference, the first 300,000 boundaries of each on both parts, every access.
# Usage: scripts/rung1.sh [DORMANN_DIR] [REFERENCE_PYTHON]
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
DORMANN=${1:-"$ROOT/external/6502-python/tests/dormann"}
PYTHON=${2:-"$ROOT/external/6502-python/.venv/bin/python"}
python3 "$ROOT/scripts/make_scenarios.py" --dormann "$DORMANN" > /dev/null
exec python3 "$ROOT/scripts/lockstep.py" "$ROOT"/external/scenarios/dormann-*.json --python "$PYTHON" --jobs "${JOBS:-4}"
