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

# The two dApps that hold funds: rock-paper-scissors games and the vault.
#
# A game (dapps/rps.compact) holds a NIGHT stake from each player until it is settled or
# cancelled. The sender hosts, the receiver is the guest; each player's key, move and salt
# are private state. Group 2 leaves games unfinished on purpose and group 6 finishes them.
# The vault (dapps/vault.compact) mints a token for itself and pays it out.
#
# These return 0 or 1 and print what went wrong; the check decides the verdict.

GAME_STAKE=1000          # atomic units of NIGHT each player puts in
ROCK=0; PAPER=1; SCISSORS=2
# Fixed test keys, not secrets.
HOST_SECRET_KEY=$(printf 'a1%.0s' {1..32})
GUEST_SECRET_KEY=$(printf 'b2%.0s' {1..32})
export GAME_STAKE

contract_label() { echo "${CONTRACT_PREFIX:-}$1"; }   # group 7's fresh contracts are fresh-<name>

player_seed() {  # <host|guest>
    if [ "$1" = host ]; then echo "$SENDER_SEED"; else echo "$RECEIVER_SEED"; fi
}

player_private_state_file() {  # <game> <player>
    DAPP_PS="$1_$2" dapp_ps_file rps
}

# Written once per player and game; toolkit-js carries it from call to call.
game_choose_move() {  # <game> <host|guest> <move>
    local secret_key=$HOST_SECRET_KEY
    [ "$2" = host ] || secret_key=$GUEST_SECRET_KEY
    jq -n --arg key "$secret_key" --argjson move "$3" --arg salt "$(openssl rand -hex 32)" \
        '{secretKey: $key, move: $move, salt: $salt}' > "$(player_private_state_file "$1" "$2")"
}

game_deploy() {  # <game> -> address
    local address
    address=$(dapp_deploy rps "$SENDER_SEED") || { echo "deploy: $address; $(dapp_err rps deploy)"; return 1; }
    registry_add "$(contract_label "game-$1")" dapp "$address" "$SIDE"
    echo "$address"
}

game_address() {  # <game>
    registry_address "$(contract_label "game-$1")"
}

game_call() {  # <game> <host|guest> <circuit> [arguments...]
    local game=$1 player=$2 circuit=$3
    shift 3
    DAPP_PS="${game}_$player" dapp_call rps "$(player_seed "$player")" "$(game_address "$game")" "$circuit" "$@" && return 0
    echo "$circuit by the $player: $(dapp_err rps "$circuit")"
    return 1
}

# The toolkit spends a whole UTXO into a call and adds no change, so the stake needs a NIGHT
# UTXO of exactly that amount: a transfer to the player's own address makes one.
game_pay_stake() {  # <game> <host|guest> <host_game|join_game>
    local game=$1 player=$2 circuit=$3 seed utxo
    seed=$(player_seed "$player")
    utxo=$(exact_night_utxo "$seed" "$(wallet_address "$seed" unshielded)" "$GAME_STAKE" "$EV/stake_${game}_$player") \
        || { echo "$circuit: $utxo"; return 1; }
    DAPP_INPUT_UTXO="$utxo" game_call "$game" "$player" "$circuit" "$(wallet_address "$seed" user-address)"
}

# Both players stake and commit to their moves.
game_start() {  # <game> <host move> <guest move>
    game_choose_move "$1" host "$2"
    game_choose_move "$1" guest "$3"
    game_pay_stake "$1" host host_game && game_pay_stake "$1" guest join_game
}

# --- Funds ----------------------------------------------------------------------------
contract_balances() {  # <address> -> {"<token type>": "<amount>"}, zero balances left out
    local out="$EV/balances_${1:0:12}"
    client_run "$out" mjs_check.mjs balances --address "$1" || { echo "balances unreadable: $(client_error "$out")"; return 1; }
    jq -S -c .balances "$out.json"
}

contract_must_hold_night() {  # <address> <expected atomic units>
    local balances held
    balances=$(contract_balances "$1") || { echo "$balances"; return 1; }
    held=$(jq -r --arg night "$NIGHT_TOKEN_TYPE" '.[$night] // "0"' <<< "$balances")
    [ "$held" = "$2" ] || { echo "the contract holds $held NIGHT, expected $2 ($balances)"; return 1; }
}

unshielded_night_of() {  # <seed> <evidence name>
    wallet_snapshot "$1" "$EV/$2.json" || { echo "wallet unreadable ($EV/$2.json.err)"; return 1; }
    wallet_night_unshielded "$EV/$2.json"
}

token_held_by() {  # <seed> <token type> <evidence name>
    wallet_snapshot "$1" "$EV/$3.json" || { echo "wallet unreadable ($EV/$3.json.err)"; return 1; }
    jq -r --arg token "$2" '[.utxos[] | select(.token_type == $token) | .value] | add // 0' "$EV/$3.json"
}

# Exactly the expected gain on local-env; on a network other wallets may also send to ours.
gained_exactly() {  # <before> <after> <expected gain>
    python3 - "$1" "$2" "$3" "$TARGET" <<'PY'
import sys
before, after, expected = (int(v) for v in sys.argv[1:4])
on_network = sys.argv[4] == "network"
gain = after - before
sys.exit(0 if gain == expected or (on_network and gain >= expected) else 1)
PY
}

# --- The vault ------------------------------------------------------------------------
vault_address() { registry_address "$(contract_label vault)"; }
vault_token() { recalled "$(state_key "$(contract_label vault)" TOKEN)"; }

vault_call() {  # <mint|pay_out> <arguments...>
    dapp_call vault "$SENDER_SEED" "$(vault_address)" "$@" && return 0
    echo "$1: $(dapp_err vault "$1")"
    return 1
}

vault_must_hold() {  # <expected amount of its own token>
    local balances held
    balances=$(contract_balances "$(vault_address)") || { echo "$balances"; return 1; }
    held=$(jq -r --arg token "$(vault_token)" '.[$token] // "0"' <<< "$balances")
    [ "$held" = "$1" ] || { echo "the vault holds $held of its token, expected $1 ($balances)"; return 1; }
}
