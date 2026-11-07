#!/usr/bin/env bash
# Rung 2: SingleStepTests 65x02, "6502" on Mos6502 and "nes6502" on the RP2A03,
# 2,560,000 cases each (LIMIT=N for the first N cases of each opcode).
# Usage: scripts/rung2.sh [CORPUS_DIR]
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
CORPUS=${1:-"$ROOT/external/6502-python/tests/6502_test_vectors"}
cargo build --release -q --manifest-path "$ROOT/Cargo.toml"
exec "$ROOT/target/release/sst" "$CORPUS" ${LIMIT:+--limit "$LIMIT"} --jobs "${JOBS:-8}"
