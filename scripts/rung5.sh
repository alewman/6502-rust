#!/usr/bin/env bash
# Rung 5: the frame (the 65xx-family base) carries a part that is not the
# NMOS 6502: tests/frame_part.rs.
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
exec cargo test --release -q --manifest-path "$ROOT/Cargo.toml" --test frame_part
