#!/usr/bin/env bash
set -euo pipefail

: "${CARGO:=cargo +nightly}"
: "${FUZZ_TARGET:=parser}"
: "${FUZZ_RUNS:=10000}"

if ! command -v cargo-fuzz >/dev/null 2>&1; then
    echo "cargo-fuzz not found. Install with 'cargo install cargo-fuzz'." >&2
    exit 1
fi

exec ${CARGO} fuzz run "${FUZZ_TARGET}" -- -runs="${FUZZ_RUNS}"
