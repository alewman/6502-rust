#!/usr/bin/env bash
# The reference, 6502-python at the pinned commit, into external/ with a venv.
# Usage: scripts/fetch_6502python.sh [DEST] [SOURCE]
#   SOURCE: a git URL or a local clone (default: GitHub). PYTHON picks the
#   interpreter for the venv (PyPy runs the long rungs several times faster).
set -euo pipefail
# 828b619 = 741419a after 6502-python's history rewrite of 2026-11-21 (same tree).
COMMIT=828b61940415eed39678fa1f7d310608201e54ef
ROOT=$(cd "$(dirname "$0")/.." && pwd)
DEST=${1:-"$ROOT/external/6502-python"}
SOURCE=${2:-https://github.com/alewman/6502-python}
PYTHON=${PYTHON:-python3}
if [ ! -d "$DEST/.git" ]; then
    git clone -q "$SOURCE" "$DEST"
fi
git -C "$DEST" fetch -q origin "$COMMIT" 2>/dev/null || true
git -C "$DEST" checkout -q "$COMMIT"
if [ ! -x "$DEST/.venv/bin/python" ]; then
    "$PYTHON" -m venv "$DEST/.venv"
fi
"$DEST/.venv/bin/python" -m pip install -q -e "$DEST"
echo "6502-python $(git -C "$DEST" rev-parse --short HEAD) in $DEST"
