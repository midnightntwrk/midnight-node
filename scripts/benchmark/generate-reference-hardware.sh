#!/usr/bin/env bash
# This file is part of midnight-node.
# Copyright (C) Midnight Foundation
# SPDX-License-Identifier: Apache-2.0
#
# Regenerates node/src/midnight_reference_hardware.json from a fresh
# `midnight-node benchmark machine` run on the current host. Treat the host
# as the reference machine: measured scores become the minimums other
# operators are checked against at startup.
#
# Usage:
#   scripts/benchmark/generate-reference-hardware.sh [BINARY]
#
# BINARY defaults to ./target/release/midnight-node. Run on dedicated
# reference hardware with no competing workload — measurements are
# sensitive to noise.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
BINARY="${1:-$REPO_ROOT/target/release/midnight-node}"
OUTPUT="$REPO_ROOT/node/src/midnight_reference_hardware.json"

if [[ ! -x "$BINARY" ]]; then
    echo "error: midnight-node binary not found or not executable: $BINARY" >&2
    echo "       build with: cargo build --release" >&2
    exit 1
fi

TMP_BASE="$(mktemp -d)"
trap 'rm -rf "$TMP_BASE"' EXIT

echo "Running benchmark machine (this takes ~1 minute)..."
RAW_OUTPUT="$("$BINARY" benchmark machine --base-path "$TMP_BASE" --allow-benchmark-overhead 2>&1 || true)"
echo "$RAW_OUTPUT"

# Parse the table. Each metric row looks like:
#   | CPU    | BLAKE2-256 | 1.00 GiBs | 1000.00 MiBs | ... |
# Score column (3rd) is what we want as the new minimum.
parse_score() {
    local function_name="$1"
    echo "$RAW_OUTPUT" | awk -F '|' -v fn="$function_name" '
        $0 ~ fn {
            score = $4
            gsub(/^[ \t]+|[ \t]+$/, "", score)
            print score
            exit
        }
    '
}

# Convert "1.00 GiBs" / "374.59 MiBs" / "649.13 KiBs" to MiB/s (the unit
# sc_sysinfo::Throughput expects in JSON).
to_mibs() {
    local raw="$1"
    if [[ -z "$raw" ]]; then
        echo "error: missing score for required metric" >&2
        exit 1
    fi
    awk -v s="$raw" 'BEGIN {
        n = s + 0
        if (s ~ /GiBs/)      printf "%.6f\n", n * 1024
        else if (s ~ /MiBs/) printf "%.6f\n", n
        else if (s ~ /KiBs/) printf "%.9f\n", n / 1024
        else { print "error: unknown unit in \"" s "\"" > "/dev/stderr"; exit 1 }
    }'
}

BLAKE=$(to_mibs "$(parse_score 'BLAKE2-256 ')")
BLAKE_PARALLEL=$(to_mibs "$(parse_score 'BLAKE2-256-Parallel-8')")
SR25519=$(to_mibs "$(parse_score 'SR25519-Verify')")
MEMCOPY=$(to_mibs "$(parse_score ' Copy ')")
SEQ_WRITE=$(to_mibs "$(parse_score 'Seq Write')")
RND_WRITE=$(to_mibs "$(parse_score 'Rnd Write')")

cat >"$OUTPUT" <<EOF
[
	{
		"metric": "Blake2256",
		"minimum": $BLAKE
	},
	{
		"metric": {"Blake2256Parallel":{"num_cores":8}},
		"minimum": $BLAKE_PARALLEL,
		"validator_only": true
	},
	{
		"metric": "Sr25519Verify",
		"minimum": $SR25519
	},
	{
		"metric": "MemCopy",
		"minimum": $MEMCOPY
	},
	{
		"metric": "DiskSeqWrite",
		"minimum": $SEQ_WRITE
	},
	{
		"metric": "DiskRndWrite",
		"minimum": $RND_WRITE
	}
]
EOF

echo
echo "Wrote reference hardware profile to: $OUTPUT"
