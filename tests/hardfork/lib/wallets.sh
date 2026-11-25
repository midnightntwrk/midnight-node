#!/usr/bin/env bash
# This file is part of midnight-node.
# Copyright (C) Midnight Foundation
# SPDX-License-Identifier: Apache-2.0
# Licensed under the Apache License, Version 2.0 (the "License");
# You may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#	http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

# The test wallets, by role. A script picks a set, then uses the role names:
#
#   use_wallets genesis    the wallets of the seeds file (on local-env: funded at genesis)
#   use_wallets fresh      wallets that never transacted before; group 7 funds them
#
#   SENDER    sends, deploys, is the dApps' maintenance authority
#   RECEIVER  receives, is the second player and the second committee member
#   HOLDER    holds the unsent old-ledger transaction until after the fork
#   SPARE     an extra destination
#   CROSSER   sends a transaction every few seconds across the fork block (genesis set only)
#
# For each role: <ROLE>_SEED, <ROLE>_ADDRESS (unshielded) and <ROLE>_SHIELDED_ADDRESS.

WALLET_ROLES=(SENDER RECEIVER HOLDER SPARE CROSSER)
declare -A GENESIS_SEED=(
    [SENDER]="$SENDER_SEED" [RECEIVER]="$RECEIVER_SEED" [HOLDER]="$HOLDER_SEED"
    [SPARE]="$SPARE_SEED" [CROSSER]="$CROSSER_SEED"
)

# An address is printed by the toolkit once and kept, so an address printed before the fork
# is the one used after it.
wallet_address() {  # <seed> <unshielded|shielded|user-address|coin-public|dust>
    local file address
    file="$STATE_DIR/addresses/$(printf '%s' "$1" | sha256_of | cut -c1-16).$2"
    if [ ! -s "$file" ]; then
        mkdir -p "$(dirname "$file")"
        address=$(tk_address "$1" "--$2")
        [ -n "$address" ] || { echo "the toolkit printed no $2 address" >&2; return 1; }
        echo "$address" > "$file"
    fi
    cat "$file"
}

# Deterministic, so a re-run finds the wallets it funded.
fresh_seed() {  # <role>
    printf 'hard fork %s, fresh %s, %s' "$FORK_NAME" "$1" "${GENESIS_SEED[$1]}" | sha256_of
}

use_wallets() {  # <genesis|fresh>
    local role seed
    WALLET_SET="$1"
    for role in "${WALLET_ROLES[@]}"; do
        if [ "$WALLET_SET" = fresh ]; then seed=$(fresh_seed "$role"); else seed="${GENESIS_SEED[$role]}"; fi
        printf -v "${role}_SEED" '%s' "$seed"
        printf -v "${role}_ADDRESS" '%s' "$(wallet_address "$seed" unshielded)"
        printf -v "${role}_SHIELDED_ADDRESS" '%s' "$(wallet_address "$seed" shielded)"
    done
    export CLIENT_SEED="$SENDER_SEED"
}

# --- Funding a wallet from another one --------------------------------------------------
NIGHT=1000000   # atomic units in one NIGHT

night_utxo_count() {  # <wallet.json>
    jq -r --arg night "$NIGHT_TOKEN_TYPE" '[.utxos[] | select(.token_type == $night)] | length' "$1"
}

# Three UTXOs, so the wallet can pay for several transactions in a row.
fund_with_night() {  # <from seed> <to address> <atomic units per UTXO> <log>
    tk_tx generate-txs single-tx --source-seed "$1" --unshielded-amount "$3" \
        --destination-address "$2" --destination-address "$2" --destination-address "$2" \
        -d "$NODE_WS" > "$4" 2>&1
}

# A wallet whose NIGHT has generated no DUST yet cannot pay for its own registration.
register_for_dust() {  # <wallet seed> <fee payer seed> <log>
    tk_tx generate-txs register-dust-address --wallet-seed "$1" --funding-seed "$2" -d "$NODE_WS" > "$3" 2>&1
}
