#!/usr/bin/env bash
# This file is part of midnight-node.
# Copyright (C) Midnight Foundation
# SPDX-License-Identifier: Apache-2.0
#
# Fail if a PR touches any frozen res/ network artifact for a live network.
#
# The per-network chain specs / config under res/mainnet, res/qanet,
# res/preprod and res/preview - and the deployed genesis ledger state/block at
# res/genesis/genesis_{state,block}_{mainnet,qanet,preprod,preview}.mn - are
# deployed artifacts: nodes already on those networks were genesis'd against
# them, so they must never change in place. Any add / modify / delete / rename
# of these paths is a hard error.
#
# Usage: check-frozen-res.sh [base-ref]
#   base-ref defaults to origin/main (override for local runs or CI).

set -euo pipefail

BASE_REF="${1:-origin/main}"

# Live networks whose deployed artifacts must never change in place.
NETWORKS='mainnet|qanet|preprod|preview'

# Paths that must never change. Anchored at the start of the path:
#   res/<network>/...                          - per-network chain spec + config
#   res/genesis/genesis_{state,block}_<net>.mn - deployed genesis ledger state/block
PROTECTED_RE="^res/(${NETWORKS})/|^res/genesis/genesis_(state|block)_(${NETWORKS})\.mn\$"

# --no-renames so a rename *out of* a protected path surfaces as a deletion of
# the protected path (and is therefore caught), instead of an R entry we'd have
# to special-case. The default diff filter already includes A/M/D, so additions
# of new protected files are caught too.
#
# Compute the diff on its own line so a failure - e.g. BASE_REF not present in
# the checkout - aborts the script via set -e (fail closed), instead of being
# masked by the grep pipeline's exit status below.
diff_output="$(git diff --name-only --no-renames "${BASE_REF}...HEAD")"

# grep exits 1 when nothing matches; that is the OK case here, so tolerate only
# that. The diff itself has already succeeded above.
changed="$(printf '%s\n' "$diff_output" | grep -E "$PROTECTED_RE" || true)"

if [ -n "$changed" ]; then
  echo "::error::Frozen res/ network config changed. These files are deployed artifacts and are only modifiable in rare cases:"
  echo "$changed" | sed 's/^/  - /'
  echo ""
  echo "If this change is genuinely required, it must be made deliberately and"
  echo "reviewed by the node team (and this check overridden intentionally)."
  exit 1
fi

echo "check-frozen-res: no changes under protected res/ folders. OK."
