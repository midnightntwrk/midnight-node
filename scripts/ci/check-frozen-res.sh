#!/usr/bin/env bash
# This file is part of midnight-node.
# Copyright (C) Midnight Foundation
# SPDX-License-Identifier: Apache-2.0
#
# Fail if a PR touches any file under the frozen res/ network-config folders.
#
# The per-network chain specs / config under res/mainnet, res/qanet,
# res/preprod and res/preview are deployed artifacts: nodes already on those
# networks were genesis'd against them, so they must never change in place. Any
# add / modify / delete / rename under these folders is a hard error.
#
# Usage: check-frozen-res.sh [base-ref]
#   base-ref defaults to origin/main (override for local runs or CI).

set -euo pipefail

BASE_REF="${1:-origin/main}"

# Folders that must never change. Anchored at the start of the path.
PROTECTED_RE='^res/(mainnet|qanet|preprod|preview)/'

# --no-renames so a rename *out of* a protected folder surfaces as a deletion of
# the protected path (and is therefore caught), instead of an R entry we'd have
# to special-case. The default diff filter already includes A/M/D, so additions
# of new files inside a protected folder are caught too.
changed="$(git diff --name-only --no-renames "${BASE_REF}...HEAD" | grep -E "$PROTECTED_RE" || true)"

if [ -n "$changed" ]; then
  echo "::error::Frozen res/ network config changed. These files are deployed artifacts and are only modifiable in rare cases:"
  echo "$changed" | sed 's/^/  - /'
  echo ""
  echo "If this change is genuinely required, it must be made deliberately and"
  echo "reviewed by the node team (and this check overridden intentionally)."
  exit 1
fi

echo "check-frozen-res: no changes under protected res/ folders. OK."
