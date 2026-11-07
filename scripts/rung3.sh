#!/usr/bin/env bash
# Rung 3: Klaus Dormann's functional test (success trap $3469) and decimal test
# (ERROR = 0), natively.
# Usage: scripts/rung3.sh [DORMANN_DIR]
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
DORMANN=${1:-"$ROOT/external/6502-python/tests/dormann"}
cargo build --release -q --manifest-path "$ROOT/Cargo.toml"
exec "$ROOT/target/release/dormann" "$DORMANN/bin_files/6502_functional_test.bin" "$DORMANN/6502_decimal_test.bin"
