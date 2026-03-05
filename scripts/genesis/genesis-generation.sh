#!/usr/bin/env bash
# This file is part of midnight-node.
# Copyright (C) Midnight Foundation
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color
BOLD='\033[1m'

# Get the root directory of the repository
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

# Available networks (excluding dev/undeployed which are for local development)
AVAILABLE_NETWORKS=("mainnet" "qanet" "devnet" "govnet" "node-dev-01" "preview")

# Default RNG seed (same as in Earthfile)
DEFAULT_RNG_SEED="0000000000000000000000000000000000000000000000000000000000000037"

# Function to print colored messages
print_header() {
    echo -e "\n${BOLD}${BLUE}═══════════════════════════════════════════════════════════════${NC}"
    echo -e "${BOLD}${BLUE}  $1${NC}"
    echo -e "${BOLD}${BLUE}═══════════════════════════════════════════════════════════════${NC}\n"
}

print_step() {
    echo -e "\n${BOLD}${CYAN}▶ $1${NC}\n"
}

print_info() {
    echo -e "${BLUE}ℹ ${NC}$1"
}

print_success() {
    echo -e "${GREEN}✓ ${NC}$1"
}

print_warning() {
    echo -e "${YELLOW}⚠ ${NC}$1"
}

print_error() {
    echo -e "${RED}✗ ${NC}$1"
}

print_file() {
    echo -e "  ${CYAN}→${NC} $1"
}

# Function to prompt for yes/no
confirm() {
    local prompt="$1"
    local default="${2:-n}"
    local response

    if [[ "$default" == "y" ]]; then
        echo -en "${BOLD}$prompt [Y/n]: ${NC}"
    else
        echo -en "${BOLD}$prompt [y/N]: ${NC}"
    fi

    read -r response
    response="${response:-$default}"

    [[ "$response" =~ ^[Yy]$ ]]
}

# Function to prompt for input with default
prompt_input() {
    local prompt="$1"
    local default="$2"
    local response

    if [[ -n "$default" ]]; then
        # Try to use zsh's vared for editable input (macOS has zsh by default)
        if command -v zsh &>/dev/null; then
            response=$(zsh -c "
                response='$default'
                vared -p $'${BOLD}$prompt${NC}: ' response
                echo \"\$response\"
            " 2>/dev/null) || {
                # Fallback if zsh fails
                echo -en "${BOLD}$prompt${NC} [${CYAN}${default}${NC}]: " >&2
                read -r response
                response="${response:-$default}"
            }
        else
            echo -en "${BOLD}$prompt${NC} [${CYAN}${default}${NC}]: " >&2
            read -r response
            response="${response:-$default}"
        fi
    else
        echo -en "${BOLD}$prompt${NC}: " >&2
        read -r response
    fi

    echo "$response"
}

# Function to get security parameter from pc-chain-config.json
get_security_parameter() {
    local network="$1"
    local config_file="$REPO_ROOT/res/$network/pc-chain-config.json"

    if [[ -f "$config_file" ]]; then
        # Extract security_parameter using grep and sed (portable)
        grep -o '"security_parameter"[[:space:]]*:[[:space:]]*[0-9]*' "$config_file" | grep -o '[0-9]*$'
    else
        echo ""
    fi
}

# Function to get cardano tip from cardano-tip.json
get_cardano_tip() {
    local network="$1"
    local config_file="$REPO_ROOT/res/$network/cardano-tip.json"

    if [[ -f "$config_file" ]]; then
        # Extract cardano_tip value using grep and sed (portable)
        grep -o '"cardano_tip"[[:space:]]*:[[:space:]]*"[^"]*"' "$config_file" | sed 's/.*"\(0x[^"]*\)".*/\1/'
    else
        echo ""
    fi
}

# Function to check if network uses cNIGHT config for DUST address registration
uses_cnight_config() {
    local network="$1"
    case "$network" in
        qanet|undeployed|devnet|govnet|node-dev-01)
            return 0  # true
            ;;
        *)
            return 1  # false
            ;;
    esac
}

# Function to check if network uses ICS config for treasury funding
uses_ics_config() {
    local network="$1"
    case "$network" in
        qanet|undeployed|devnet|govnet|node-dev-01)
            return 0  # true
            ;;
        *)
            return 1  # false
            ;;
    esac
}

# Function to check if network uses reserve config
uses_reserve_config() {
    local network="$1"
    case "$network" in
        qanet|undeployed|devnet|govnet|node-dev-01)
            return 0  # true
            ;;
        *)
            return 1  # false
            ;;
    esac
}

# Function to show input files info
show_input_files() {
    local network="$1"
    local res_dir="$REPO_ROOT/res/$network"

    echo -e "${BOLD}Input configuration files for ${CYAN}$network${NC}:${NC}"
    echo ""

    local files=(
        "cnight-addresses.json"
        "ics-addresses.json"
        "reserve-addresses.json"
        "ledger-parameters-config.json"
        "federated-authority-addresses.json"
        "permissioned-candidates-addresses.json"
        "pc-chain-config.json"
        "system-parameters-config.json"
        "registered-candidates-addresses.json"
    )

    for file in "${files[@]}"; do
        if [[ -f "$res_dir/$file" ]]; then
            print_file "$res_dir/$file"
        else
            echo -e "  ${RED}✗${NC} $res_dir/$file ${RED}(missing)${NC}"
        fi
    done
    echo ""
}

# Function to show genesis files info
show_genesis_files() {
    local network="$1"
    local genesis_dir="$REPO_ROOT/res/genesis"

    echo -e "${BOLD}Genesis files for ${CYAN}$network${NC}:${NC}"
    echo ""

    local block_file="$genesis_dir/genesis_block_$network.mn"
    local state_file="$genesis_dir/genesis_state_$network.mn"

    if [[ -f "$block_file" ]]; then
        print_file "$block_file"
    else
        echo -e "  ${YELLOW}○${NC} $block_file ${YELLOW}(will be created)${NC}"
    fi

    if [[ -f "$state_file" ]]; then
        print_file "$state_file"
    else
        echo -e "  ${YELLOW}○${NC} $state_file ${YELLOW}(will be created)${NC}"
    fi
    echo ""
}

# Function to get the Earthly target name for a network
get_genesis_state_target() {
    local network="$1"
    echo "rebuild-genesis-state-$network"
}

# Function to print step summary
print_step_summary() {
    local step_name="$1"
    shift
    local config_items=("$@")

    echo ""
    echo -e "${BOLD}${GREEN}─────────────────────────────────────────────────────────────────${NC}"
    echo -e "${BOLD}${GREEN}  $step_name - Completed${NC}"
    echo -e "${BOLD}${GREEN}─────────────────────────────────────────────────────────────────${NC}"
    echo ""
    echo -e "${BOLD}Configuration used:${NC}"
    for item in "${config_items[@]}"; do
        echo -e "  $item"
    done
    echo ""
}

# Function to ensure node binary exists
ensure_node_binary() {
    local node_binary="$REPO_ROOT/target/release/midnight-node"
    if [[ ! -f "$node_binary" ]]; then
        print_warning "midnight-node binary not found at $node_binary"
        if confirm "Build the node in release mode?"; then
            print_info "Building midnight-node..."
            cd "$REPO_ROOT"
            ~/.cargo/bin/cargo build --release -p midnight-node
            print_success "Build completed!"
        else
            print_error "Cannot proceed without midnight-node binary."
            return 1
        fi
    fi
    echo "$node_binary"
}

# Function to run cNIGHT genesis generation (for DUST address registration)
run_cnight_genesis_generation() {
    local network="$1"
    local db_connection="$2"
    local cardano_tip="$3"
    local node_binary="$4"

    echo -e "${BOLD}Command to execute:${NC}"
    echo -e "  ${CYAN}CFG_PRESET=$network \\\\${NC}"
    echo -e "  ${CYAN}ALLOW_NON_SSL=true \\\\${NC}"
    echo -e "  ${CYAN}DB_SYNC_POSTGRES_CONNECTION_STRING=\"...\" \\\\${NC}"
    echo -e "  ${CYAN}$node_binary generate-c-night-genesis \\\\${NC}"
    echo -e "  ${CYAN}--cardano-tip $cardano_tip${NC}"
    echo ""

    print_info "Running cNIGHT genesis generation..."
    echo ""

    cd "$REPO_ROOT"
    export CFG_PRESET="$network"
    export ALLOW_NON_SSL=true
    export DB_SYNC_POSTGRES_CONNECTION_STRING="$db_connection"

    if "$node_binary" generate-c-night-genesis --cardano-tip "$cardano_tip"; then
        echo ""
        print_success "cNIGHT genesis generation completed!"
        echo ""
        echo "File created/updated:"
        print_file "$REPO_ROOT/res/$network/cnight-config.json"

        return 0
    else
        print_error "cNIGHT genesis generation failed!"
        return 1
    fi
}

# Function to run ICS genesis generation (for treasury funding)
run_ics_genesis_generation() {
    local network="$1"
    local db_connection="$2"
    local cardano_tip="$3"
    local node_binary="$4"

    echo -e "${BOLD}Command to execute:${NC}"
    echo -e "  ${CYAN}CFG_PRESET=$network \\\\${NC}"
    echo -e "  ${CYAN}ALLOW_NON_SSL=true \\\\${NC}"
    echo -e "  ${CYAN}DB_SYNC_POSTGRES_CONNECTION_STRING=\"...\" \\\\${NC}"
    echo -e "  ${CYAN}$node_binary generate-ics-genesis \\\\${NC}"
    echo -e "  ${CYAN}--cardano-tip $cardano_tip${NC}"
    echo ""

    print_info "Running ICS genesis generation..."
    echo ""

    cd "$REPO_ROOT"
    export CFG_PRESET="$network"
    export ALLOW_NON_SSL=true
    export DB_SYNC_POSTGRES_CONNECTION_STRING="$db_connection"

    if "$node_binary" generate-ics-genesis --cardano-tip "$cardano_tip"; then
        echo ""
        print_success "ICS genesis generation completed!"
        echo ""
        echo "File created/updated:"
        print_file "$REPO_ROOT/res/$network/ics-config.json"

        return 0
    else
        print_error "ICS genesis generation failed!"
        return 1
    fi
}

# Function to run reserve genesis generation
run_reserve_genesis_generation() {
    local network="$1"
    local db_connection="$2"
    local cardano_tip="$3"
    local node_binary="$4"

    echo -e "${BOLD}Command to execute:${NC}"
    echo -e "  ${CYAN}CFG_PRESET=$network \\\\${NC}"
    echo -e "  ${CYAN}ALLOW_NON_SSL=true \\\\${NC}"
    echo -e "  ${CYAN}DB_SYNC_POSTGRES_CONNECTION_STRING=\"...\" \\\\${NC}"
    echo -e "  ${CYAN}$node_binary generate-reserve-genesis \\\\${NC}"
    echo -e "  ${CYAN}--cardano-tip $cardano_tip${NC}"
    echo ""

    print_info "Running reserve genesis generation..."
    echo ""

    cd "$REPO_ROOT"
    export CFG_PRESET="$network"
    export ALLOW_NON_SSL=true
    export DB_SYNC_POSTGRES_CONNECTION_STRING="$db_connection"

    if "$node_binary" generate-reserve-genesis --cardano-tip "$cardano_tip"; then
        echo ""
        print_success "Reserve genesis generation completed!"
        echo ""
        echo "File created/updated:"
        print_file "$REPO_ROOT/res/$network/reserve-config.json"

        return 0
    else
        print_error "Reserve genesis generation failed!"
        return 1
    fi
}

# Function to run ledger state generation
run_ledger_state_generation() {
    local network="$1"
    local rng_seed="$2"

    # Use the network-specific Earthly target (e.g., +rebuild-genesis-state-qanet)
    local earthly_target
    earthly_target=$(get_genesis_state_target "$network")

    # For mainnet, RNG_SEED is not used (no faucet wallets), so don't pass it
    local earthly_cmd
    if [[ "$network" == "mainnet" ]]; then
        echo -e "${BOLD}Command to execute:${NC}"
        echo -e "  ${CYAN}earthly -P +$earthly_target${NC}"
        earthly_cmd=(earthly -P "+$earthly_target")
    else
        echo -e "${BOLD}Command to execute:${NC}"
        echo -e "  ${CYAN}earthly -P +$earthly_target --RNG_SEED=$rng_seed${NC}"
        earthly_cmd=(earthly -P "+$earthly_target" "--RNG_SEED=$rng_seed")
    fi
    echo ""

    print_info "Running Earthly target..."
    echo ""

    cd "$REPO_ROOT"
    if "${earthly_cmd[@]}"; then
        echo ""
        print_success "Ledger state generation completed!"
        echo ""
        echo "Files created:"
        print_file "$REPO_ROOT/res/genesis/genesis_block_$network.mn"
        print_file "$REPO_ROOT/res/genesis/genesis_state_$network.mn"

        if [[ "$network" == "mainnet" ]]; then
            print_step_summary "Step 2: Ledger State Generation" \
                "Network: $network" \
                "RNG Seed: (not used - no faucet wallets)"
        else
            print_step_summary "Step 2: Ledger State Generation" \
                "Network: $network" \
                "RNG Seed: $rng_seed"
        fi

        return 0
    else
        print_error "Ledger state generation failed!"
        return 1
    fi
}

# Function to run smart contract genesis config generation
run_genesis_config_generation() {
    local network="$1"
    local db_connection="$2"
    local cardano_tip="$3"
    local security_param="$4"
    local node_binary="$5"

    echo -e "${BOLD}Command to execute:${NC}"
    echo -e "  ${CYAN}CFG_PRESET=$network \\\\${NC}"
    echo -e "  ${CYAN}CARDANO_SECURITY_PARAMETER=$security_param \\\\${NC}"
    echo -e "  ${CYAN}ALLOW_NON_SSL=true \\\\${NC}"
    echo -e "  ${CYAN}DB_SYNC_POSTGRES_CONNECTION_STRING=\"...\" \\\\${NC}"
    echo -e "  ${CYAN}$node_binary generate-genesis-config \\\\${NC}"
    echo -e "  ${CYAN}--cardano-tip $cardano_tip${NC}"
    echo ""

    print_info "Running genesis config generation..."
    echo ""

    cd "$REPO_ROOT"
    export CFG_PRESET="$network"
    export CARDANO_SECURITY_PARAMETER="$security_param"
    export ALLOW_NON_SSL=true
    export DB_SYNC_POSTGRES_CONNECTION_STRING="$db_connection"

    if "$node_binary" generate-genesis-config --cardano-tip "$cardano_tip"; then
        echo ""
        print_success "Genesis config generation completed!"
        echo ""
        echo "Files created/updated:"
        print_file "$REPO_ROOT/res/$network/cnight-config.json"
        print_file "$REPO_ROOT/res/$network/federated-authority-config.json"
        print_file "$REPO_ROOT/res/$network/permissioned-candidates-config.json"

        print_step_summary "Step 1: Genesis Config Generation" \
            "Network: $network" \
            "Security Parameter: $security_param" \
            "Cardano Tip: $cardano_tip"

        return 0
    else
        print_error "Genesis config generation failed!"
        return 1
    fi
}

# Function to run partial genesis config generation (federated-authority and permissioned-candidates only)
# Used when cnight-config.json was already generated in Step 1a
run_partial_genesis_config_generation() {
    local network="$1"
    local db_connection="$2"
    local cardano_tip="$3"
    local security_param="$4"
    local node_binary="$5"

    echo -e "${BOLD}Commands to execute:${NC}"
    echo -e "  ${CYAN}1. $node_binary generate-federated-authority-genesis --cardano-tip $cardano_tip${NC}"
    echo -e "  ${CYAN}2. $node_binary generate-permissioned-candidates-genesis --cardano-tip $cardano_tip${NC}"
    echo ""

    print_info "Running federated authority genesis generation..."
    echo ""

    cd "$REPO_ROOT"
    export CFG_PRESET="$network"
    export CARDANO_SECURITY_PARAMETER="$security_param"
    export ALLOW_NON_SSL=true
    export DB_SYNC_POSTGRES_CONNECTION_STRING="$db_connection"

    if ! "$node_binary" generate-federated-authority-genesis --cardano-tip "$cardano_tip"; then
        print_error "Federated authority genesis generation failed!"
        return 1
    fi
    print_success "Federated authority genesis generation completed!"
    echo ""

    print_info "Running permissioned candidates genesis generation..."
    echo ""

    if ! "$node_binary" generate-permissioned-candidates-genesis --cardano-tip "$cardano_tip"; then
        print_error "Permissioned candidates genesis generation failed!"
        return 1
    fi

    echo ""
    print_success "Partial genesis config generation completed!"
    echo ""
    echo "Files created/updated:"
    print_file "$REPO_ROOT/res/$network/federated-authority-config.json"
    print_file "$REPO_ROOT/res/$network/permissioned-candidates-config.json"

    print_step_summary "Step 1: Genesis Config Generation (partial)" \
        "Network: $network" \
        "Security Parameter: $security_param" \
        "Cardano Tip: $cardano_tip" \
        "Note: ics-config.json was preserved from a previous run"

    return 0
}

# Function to run chainspec generation
run_chainspec_generation() {
    local network="$1"
    local deterministic="${2:-false}"

    # Check for sha256 hashing tool (needed for chain-spec-hash.json)
    if ! command -v sha256sum &>/dev/null && ! command -v shasum &>/dev/null; then
        print_error "Neither 'sha256sum' nor 'shasum' is installed."
        print_info "Please install one of them:"
        echo -e "  ${CYAN}Linux:${NC}   sudo apt install coreutils    ${CYAN}# provides sha256sum${NC}"
        echo -e "  ${CYAN}macOS:${NC}   brew install coreutils         ${CYAN}# provides sha256sum${NC}"
        echo ""
        return 1
    fi

    echo -e "${BOLD}Input files used:${NC}"
    print_file "$REPO_ROOT/res/$network/pc-chain-config.json"
    print_file "$REPO_ROOT/res/$network/system-parameters-config.json"
    print_file "$REPO_ROOT/res/$network/registered-candidates-addresses.json"
    print_file "$REPO_ROOT/res/$network/permissioned-candidates-config.json"
    print_file "$REPO_ROOT/res/$network/federated-authority-config.json"
    print_file "$REPO_ROOT/res/$network/cnight-config.json"
    print_file "$REPO_ROOT/res/$network/ics-config.json"
    print_file "$REPO_ROOT/res/genesis/genesis_block_$network.mn"
    print_file "$REPO_ROOT/res/genesis/genesis_state_$network.mn"
    echo ""

    local earthly_cmd=(earthly -P +rebuild-chainspec "--NETWORK=$network")
    if [[ "$deterministic" == "true" ]]; then
        earthly_cmd+=("--DETERMINISTIC=true")
    fi

    echo -e "${BOLD}Command to execute:${NC}"
    echo -e "  ${CYAN}${earthly_cmd[*]}${NC}"
    echo ""

    if [[ "$deterministic" == "true" ]]; then
        print_info "Using srtool for deterministic WASM build (this may take longer)..."
    else
        print_info "Running Earthly target..."
    fi
    echo ""

    cd "$REPO_ROOT"
    if "${earthly_cmd[@]}"; then
        echo ""
        print_success "Chain specification generation completed!"
        echo ""

        # Generate hash of chain-spec-raw.json
        local raw_spec_file="$REPO_ROOT/res/$network/chain-spec-raw.json"
        local hash_file="$REPO_ROOT/res/$network/chain-spec-hash.json"
        if [[ -f "$raw_spec_file" ]]; then
            print_info "Generating hash of chain-spec-raw.json..."
            local spec_hash
            if command -v sha256sum &>/dev/null; then
                spec_hash=$(sha256sum "$raw_spec_file" | awk '{print $1}')
            elif command -v shasum &>/dev/null; then
                spec_hash=$(shasum -a 256 "$raw_spec_file" | awk '{print $1}')
            else
                print_error "Neither sha256sum nor shasum found. Cannot generate hash."
                return 0
            fi
            echo "{\"hash\": \"$spec_hash\"}" > "$hash_file"
            print_success "Hash generated: $spec_hash"
            echo ""
        fi

        echo "Files created:"
        print_file "$REPO_ROOT/res/$network/chain-spec.json"
        print_file "$REPO_ROOT/res/$network/chain-spec-abridged.json"
        print_file "$REPO_ROOT/res/$network/chain-spec-raw.json"
        print_file "$REPO_ROOT/res/$network/chain-spec-hash.json"
        if [[ "$deterministic" == "true" ]]; then
            print_file "$REPO_ROOT/res/$network/srtool-digest.json"
        fi

        local summary_note=""
        if [[ "$deterministic" == "true" ]]; then
            summary_note="Deterministic build: Yes (srtool)"
        fi
        print_step_summary "Step 3: Chain Spec Generation" \
            "Network: $network" \
            "$summary_note"

        return 0
    else
        print_error "Chain specification generation failed!"
        return 1
    fi
}

# Main script
main() {
    print_header "Midnight Genesis Generation Tool"

    echo "This tool will guide you through the chain specification generation process."
    echo "It consists of three main steps:"
    echo ""
    echo -e "  1. ${BOLD}Genesis Config Generation${NC} - Generates config files from smart contract addresses"
    echo -e "  2. ${BOLD}Ledger State Generation${NC} - Creates initial ledger state (genesis_block, genesis_state)"
    echo -e "  3. ${BOLD}Chain Spec Generation${NC} - Creates the final chain specification files"
    echo ""

    # Select network
    print_step "Select Network"

    echo -e "${BOLD}Available networks:${NC}"
    echo ""
    PS3=$'\n'"Select network (1-${#AVAILABLE_NETWORKS[@]}): "
    select network in "${AVAILABLE_NETWORKS[@]}"; do
        if [[ -n "$network" ]]; then
            break
        fi
        echo "Invalid selection. Please try again."
    done
    print_success "Selected network: $network"
    echo ""

    # Show input files
    show_input_files "$network"

    # Get security parameter from pc-chain-config.json (used later in step 2)
    local security_param
    security_param=$(get_security_parameter "$network")
    if [[ -z "$security_param" ]]; then
        security_param="432"
        print_warning "Could not read security_parameter from pc-chain-config.json, using default: $security_param"
    fi

    # Collect common inputs upfront
    print_step "Configuration"

    echo -e "${BOLD}These inputs are needed for genesis generation steps:${NC}"
    echo ""

    local db_connection
    db_connection=$(prompt_input "DB Sync PostgreSQL connection string" "postgres://cardano@localhost:54322/cexplorer")
    echo ""

    # Get default cardano tip from cardano-tip.json if available
    local default_cardano_tip
    default_cardano_tip=$(get_cardano_tip "$network")
    if [[ -n "$default_cardano_tip" ]]; then
        print_info "Found cardano tip in res/$network/cardano-tip.json"
    fi

    local cardano_tip
    cardano_tip=$(prompt_input "Cardano block hash (tip)" "$default_cardano_tip")
    if [[ -z "$cardano_tip" ]]; then
        print_error "Cardano tip is required!"
        exit 1
    fi
    echo ""

    # RNG seed is only needed for networks that fund faucet wallets (not mainnet)
    local rng_seed="$DEFAULT_RNG_SEED"
    if [[ "$network" != "mainnet" ]]; then
        rng_seed=$(prompt_input "RNG seed for ledger state" "$DEFAULT_RNG_SEED")
        echo ""
    fi

    # Show configuration summary
    echo -e "${BOLD}Configuration Summary:${NC}"
    echo -e "  Network:              ${CYAN}$network${NC}"
    echo -e "  Security Parameter:   ${CYAN}$security_param${NC}"
    echo -e "  Cardano Tip:          ${CYAN}$cardano_tip${NC}"
    if [[ "$network" != "mainnet" ]]; then
        echo -e "  RNG Seed:             ${CYAN}$rng_seed${NC}"
    else
        echo -e "  RNG Seed:             ${CYAN}(not used - no faucet wallets)${NC}"
    fi
    echo ""

    # Track completed steps and config generation
    local step1_completed=false
    local step2_completed=false
    local step3_completed=false
    local cnight_config_generated=false
    local ics_config_generated=false

    # Ensure node binary exists (needed for genesis config generation)
    local node_binary
    node_binary=$(ensure_node_binary) || exit 1

    # =========================================================================
    # STEP 1: Genesis Config Generation
    # =========================================================================
    print_step "Step 1: Smart Contract Genesis Configuration Generation"

    echo -e "${BOLD}This step generates genesis config files from smart contract addresses.${NC}"
    echo ""
    echo "Input files:"
    print_file "$REPO_ROOT/res/$network/federated-authority-addresses.json -> federated-authority-config.json"
    print_file "$REPO_ROOT/res/$network/permissioned-candidates-addresses.json -> permissioned-candidates-config.json"
    print_file "$REPO_ROOT/res/$network/cnight-addresses.json -> cnight-config.json"
    print_file "$REPO_ROOT/res/$network/ics-addresses.json -> ics-config.json (for treasury funding)"
    print_file "$REPO_ROOT/res/$network/reserve-addresses.json -> reserve-config.json"
    echo ""

    if confirm "Run Step 1 (Genesis Config Generation)?" "y"; then
        echo ""

        run_genesis_config_generation "$network" "$db_connection" "$cardano_tip" "$security_param" "$node_binary"
        local result=$?
        if [[ $result -eq 0 ]]; then
            step1_completed=true
            cnight_config_generated=true
            ics_config_generated=true
        elif [[ $result -eq 1 ]]; then
            print_error "Step 1 failed. Exiting."
            exit 1
        fi

        # Generate reserve config if the network uses it
        if uses_reserve_config "$network"; then
            echo ""
            run_reserve_genesis_generation "$network" "$db_connection" "$cardano_tip" "$node_binary"
            local reserve_result=$?
            if [[ $reserve_result -ne 0 ]]; then
                print_error "Reserve genesis generation failed."
            fi
        fi
    else
        print_info "Skipping Step 1."
    fi

    # =========================================================================
    # STEP 2: Ledger State Generation
    # =========================================================================
    print_step "Step 2: Ledger State Generation"

    echo -e "${BOLD}This step generates the initial ledger state using Earthly.${NC}"
    echo ""

    echo "Input files used for ledger state generation:"
    print_file "$REPO_ROOT/res/$network/ledger-parameters-config.json"
    if uses_cnight_config "$network"; then
        print_file "$REPO_ROOT/res/$network/cnight-config.json"
    fi
    if uses_ics_config "$network"; then
        print_file "$REPO_ROOT/res/$network/ics-config.json"
    fi
    if uses_reserve_config "$network"; then
        print_file "$REPO_ROOT/res/$network/reserve-config.json"
    fi
    echo ""

    show_genesis_files "$network"

    # Check for existing genesis files
    if [[ -f "$REPO_ROOT/res/genesis/genesis_block_$network.mn" ]] && \
       [[ -f "$REPO_ROOT/res/genesis/genesis_state_$network.mn" ]]; then
        print_info "Existing genesis files found for $network."
        echo ""
    fi

    if confirm "Run Step 2 (Ledger State Generation)?" "n"; then
        echo ""

        local can_proceed=true

        # Check if this network uses cNIGHT config (for DUST address registration)
        if uses_cnight_config "$network"; then
            # Check if cnight-config.json exists (should have been generated in Step 1)
            if [[ ! -f "$REPO_ROOT/res/$network/cnight-config.json" ]]; then
                echo -e "${YELLOW}Note:${NC} Network ${CYAN}$network${NC} uses cNIGHT config for DUST address registration."
                echo "This requires cnight-config.json to be generated from smart contract data."
                echo ""
                print_warning "cnight-config.json not found. It must be generated first."
                echo ""
                run_cnight_genesis_generation "$network" "$db_connection" "$cardano_tip" "$node_binary"
                local result=$?
                if [[ $result -ne 0 ]]; then
                    print_error "cNIGHT genesis generation failed."
                    can_proceed=false
                else
                    cnight_config_generated=true
                fi
                echo ""
            fi
        fi

        # Check if this network uses ICS config (for treasury funding)
        if uses_ics_config "$network"; then
            # Check if ics-config.json exists (should have been generated in Step 1)
            if [[ ! -f "$REPO_ROOT/res/$network/ics-config.json" ]]; then
                echo -e "${YELLOW}Note:${NC} Network ${CYAN}$network${NC} uses ICS config for treasury funding."
                echo "This requires ics-config.json to be generated from smart contract data."
                echo ""
                print_warning "ics-config.json not found. It must be generated first."
                echo ""
                run_ics_genesis_generation "$network" "$db_connection" "$cardano_tip" "$node_binary"
                local result=$?
                if [[ $result -ne 0 ]]; then
                    print_error "ICS genesis generation failed."
                    can_proceed=false
                else
                    ics_config_generated=true
                fi
                echo ""
            fi
        fi

        # Only run ledger state generation if we have all required files
        if [[ "$can_proceed" == "true" ]]; then
            # Check that required config files exist
            local missing_files=false
            if uses_cnight_config "$network" && [[ ! -f "$REPO_ROOT/res/$network/cnight-config.json" ]]; then
                print_error "cnight-config.json is required but missing."
                missing_files=true
            fi
            if uses_ics_config "$network" && [[ ! -f "$REPO_ROOT/res/$network/ics-config.json" ]]; then
                print_error "ics-config.json is required but missing."
                missing_files=true
            fi

            if [[ "$missing_files" == "true" ]]; then
                print_error "Cannot proceed with ledger state generation due to missing config files."
            else
                run_ledger_state_generation "$network" "$rng_seed"
                local result=$?
                if [[ $result -eq 0 ]]; then
                    step2_completed=true
                elif [[ $result -eq 1 ]]; then
                    print_error "Step 2 failed. Exiting."
                    exit 1
                fi
            fi
        else
            print_error "Cannot proceed with ledger state generation due to config generation failures."
        fi
    else
        print_info "Skipping Step 2."
    fi

    # =========================================================================
    # STEP 3: Chain Spec Generation
    # =========================================================================
    print_step "Step 3: Chain Specification Generation"

    echo -e "${BOLD}This step generates the final chain specification files.${NC}"
    echo ""

    if confirm "Run Step 3 (Chain Spec Generation)?" "y"; then
        echo ""
        local use_deterministic="false"
        echo -e "${BOLD}Deterministic builds use srtool to create reproducible runtime WASM.${NC}"
        echo -e "This ensures the same WASM hash across different build environments."
        echo -e "${YELLOW}Note: Deterministic builds take longer but are recommended for production networks.${NC}"
        echo ""
        if confirm "Use deterministic build (srtool)?" "n"; then
            use_deterministic="true"
        fi
        echo ""
        run_chainspec_generation "$network" "$use_deterministic"
        local result=$?
        if [[ $result -eq 0 ]]; then
            step3_completed=true
        elif [[ $result -eq 1 ]]; then
            print_error "Step 3 failed. Exiting."
            exit 1
        fi
    else
        print_info "Skipping Step 3."
    fi

    # =========================================================================
    # Final Summary
    # =========================================================================
    print_header "Generation Complete!"

    echo -e "Summary for ${BOLD}$network${NC}:"
    echo ""

    # Show config files generated in Step 1
    if [[ "$cnight_config_generated" == "true" ]] || [[ "$ics_config_generated" == "true" ]]; then
        echo -e "  ${GREEN}✓${NC} Config files generated for ledger state:"
        if [[ "$cnight_config_generated" == "true" ]]; then
            print_file "$REPO_ROOT/res/$network/cnight-config.json"
        fi
        if [[ "$ics_config_generated" == "true" ]]; then
            print_file "$REPO_ROOT/res/$network/ics-config.json"
        fi
        echo ""
    fi

    if [[ "$step1_completed" == "true" ]]; then
        echo -e "  ${GREEN}✓${NC} Step 1: Genesis Config Generation"
        print_file "$REPO_ROOT/res/$network/cnight-config.json"
        print_file "$REPO_ROOT/res/$network/ics-config.json"
        print_file "$REPO_ROOT/res/$network/reserve-config.json"
        print_file "$REPO_ROOT/res/$network/federated-authority-config.json"
        print_file "$REPO_ROOT/res/$network/permissioned-candidates-config.json"
    else
        echo -e "  ${YELLOW}○${NC} Step 1: Genesis Config Generation (skipped)"
    fi
    echo ""

    if [[ "$step2_completed" == "true" ]]; then
        echo -e "  ${GREEN}✓${NC} Step 2: Ledger State Generation"
        print_file "$REPO_ROOT/res/genesis/genesis_block_$network.mn"
        print_file "$REPO_ROOT/res/genesis/genesis_state_$network.mn"
    else
        echo -e "  ${YELLOW}○${NC} Step 2: Ledger State Generation (skipped)"
    fi
    echo ""

    if [[ "$step3_completed" == "true" ]]; then
        echo -e "  ${GREEN}✓${NC} Step 3: Chain Spec Generation"
        print_file "$REPO_ROOT/res/$network/chain-spec.json"
        print_file "$REPO_ROOT/res/$network/chain-spec-abridged.json"
        print_file "$REPO_ROOT/res/$network/chain-spec-raw.json"
        print_file "$REPO_ROOT/res/$network/chain-spec-hash.json"
    else
        echo -e "  ${YELLOW}○${NC} Step 3: Chain Spec Generation (skipped)"
    fi
    echo ""

    if [[ "$cnight_config_generated" == "true" ]] || [[ "$ics_config_generated" == "true" ]] || [[ "$step1_completed" == "true" ]] || [[ "$step2_completed" == "true" ]] || [[ "$step3_completed" == "true" ]]; then
        print_success "All selected steps completed successfully!"
    else
        print_warning "No steps were executed."
    fi
}

# Run main function
main "$@"
