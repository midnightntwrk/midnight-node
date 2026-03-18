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
AVAILABLE_NETWORKS=("mainnet" "qanet" "devnet" "govnet")

# Function to print colored messages
print_header() {
    echo -e "\n${BOLD}${BLUE}=================================================================${NC}"
    echo -e "${BOLD}${BLUE}  $1${NC}"
    echo -e "${BOLD}${BLUE}=================================================================${NC}\n"
}

print_step() {
    echo -e "\n${BOLD}${CYAN}>>> $1${NC}\n"
}

print_substep() {
    echo -e "${BOLD}${CYAN}  > $1${NC}"
}

print_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

print_success() {
    echo -e "${GREEN}[PASS]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

print_error() {
    echo -e "${RED}[FAIL]${NC} $1"
}

print_file() {
    echo -e "  ${CYAN}|${NC} $1"
}

print_diff() {
    echo -e "${RED}$1${NC}"
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
        mainnet|qanet|undeployed|devnet|govnet)
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
        mainnet|qanet|undeployed|devnet|govnet)
            return 0  # true
            ;;
        *)
            return 1  # false
            ;;
    esac
}

# Function to check if network uses reserve config for locked pool
uses_reserve_config() {
    local network="$1"
    case "$network" in
        mainnet|qanet|undeployed|devnet|govnet)
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

    echo -e "${BOLD}Configuration files for ${CYAN}$network${NC}:${NC}"
    echo ""

    local files=(
        "cnight-addresses.json"
        "ics-addresses.json"
        "ledger-parameters-config.json"
        "federated-authority-addresses.json"
        "permissioned-candidates-addresses.json"
        "pc-chain-config.json"
        "system-parameters-config.json"
        "registered-candidates-addresses.json"
        "chain-spec-raw.json"
    )

    for file in "${files[@]}"; do
        if [[ -f "$res_dir/$file" ]]; then
            print_file "$res_dir/$file"
        else
            echo -e "  ${RED}X${NC} $res_dir/$file ${RED}(missing)${NC}"
        fi
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

# Function to create temporary directory for verification
create_temp_dir() {
    local tmp_dir
    tmp_dir=$(mktemp -d -t genesis-verification-XXXXXX)
    echo "$tmp_dir"
}

# Function to compare two JSON files (ignoring whitespace differences)
compare_json_files() {
    local file1="$1"
    local file2="$2"
    local name="$3"

    if ! command -v jq &>/dev/null; then
        # Fall back to direct comparison if jq is not available
        if diff -q "$file1" "$file2" > /dev/null 2>&1; then
            print_success "$name matches"
            return 0
        else
            print_error "$name differs"
            echo ""
            echo "Differences found:"
            diff --color=always "$file1" "$file2" || true
            return 1
        fi
    fi

    # Use jq to normalize JSON before comparison
    local norm1 norm2
    norm1=$(jq -S '.' "$file1" 2>/dev/null) || {
        print_error "Failed to parse $file1 as JSON"
        return 1
    }
    norm2=$(jq -S '.' "$file2" 2>/dev/null) || {
        print_error "Failed to parse $file2 as JSON"
        return 1
    }

    if [[ "$norm1" == "$norm2" ]]; then
        print_success "$name matches"
        return 0
    else
        print_error "$name differs"
        echo ""
        echo "Differences found:"
        diff --color=always <(echo "$norm1") <(echo "$norm2") || true
        return 1
    fi
}

# ===========================================================================
# VERIFICATION STEP 0: Verify Cardano Tip is Finalized
# ===========================================================================
run_cardano_tip_finalization_check() {
    local network="$1"
    local db_connection="$2"
    local cardano_tip="$3"
    local node_binary="$4"

    print_step "Step 0: Verify Cardano Tip is Finalized"

    echo -e "${BOLD}This step verifies that the provided Cardano block hash has enough${NC}"
    echo -e "${BOLD}confirmations to be considered finalized (based on security_parameter).${NC}"
    echo ""

    cd "$REPO_ROOT"
    export CFG_PRESET="$network"
    export ALLOW_NON_SSL=true
    export DB_SYNC_POSTGRES_CONNECTION_STRING="$db_connection"

    local check_result
    if check_result=$("$node_binary" verify-cardano-tip-finalized --cardano-tip "$cardano_tip" 2>&1); then
        echo "$check_result"
        echo ""
        print_success "Step 0: Cardano tip is finalized!"
        return 0
    else
        echo "$check_result"
        echo ""
        print_error "Step 0: Cardano tip is NOT finalized!"
        return 1
    fi
}

# ===========================================================================
# VERIFICATION STEP 1: Regenerate and compare genesis config files
# ===========================================================================
run_config_regeneration_verification() {
    local network="$1"
    local db_connection="$2"
    local cardano_tip="$3"
    local security_param="$4"
    local node_binary="$5"
    local tmp_dir="$6"

    print_step "Step 1: Regenerate and Compare Genesis Config Files"

    echo -e "${BOLD}This step regenerates all genesis config files and compares them${NC}"
    echo -e "${BOLD}with the files under res/$network/.${NC}"
    echo ""

    local all_passed=true
    local res_dir="$REPO_ROOT/res/$network"

    cd "$REPO_ROOT"
    export CFG_PRESET="$network"
    export CARDANO_SECURITY_PARAMETER="$security_param"
    export ALLOW_NON_SSL=true
    export DB_SYNC_POSTGRES_CONNECTION_STRING="$db_connection"

    # 1a. Regenerate cnight-config.json
    if uses_cnight_config "$network"; then
        if confirm "  Verify cnight-config.json?" "y"; then
            print_substep "Regenerating cnight-config.json..."
            local tmp_cnight="$tmp_dir/cnight-config.json"

            if "$node_binary" generate-c-night-genesis --cardano-tip "$cardano_tip" --output "$tmp_cnight" 2>/dev/null; then
                if ! compare_json_files "$tmp_cnight" "$res_dir/cnight-config.json" "cnight-config.json"; then
                    all_passed=false
                fi
            else
                print_error "Failed to regenerate cnight-config.json"
                all_passed=false
            fi
        else
            print_info "Skipping cnight-config.json verification."
        fi
    fi

    # 1b. Regenerate ics-config.json
    if uses_ics_config "$network"; then
        if confirm "  Verify ics-config.json?" "y"; then
            print_substep "Regenerating ics-config.json..."
            local tmp_ics="$tmp_dir/ics-config.json"

            if "$node_binary" generate-ics-genesis --cardano-tip "$cardano_tip" --output "$tmp_ics" 2>/dev/null; then
                if ! compare_json_files "$tmp_ics" "$res_dir/ics-config.json" "ics-config.json"; then
                    all_passed=false
                fi
            else
                print_error "Failed to regenerate ics-config.json"
                all_passed=false
            fi
        else
            print_info "Skipping ics-config.json verification."
        fi
    fi

    # 1c. Regenerate federated-authority-config.json
    if confirm "  Verify federated-authority-config.json?" "y"; then
        print_substep "Regenerating federated-authority-config.json..."
        local tmp_fa="$tmp_dir/federated-authority-config.json"

        if "$node_binary" generate-federated-authority-genesis --cardano-tip "$cardano_tip" --output "$tmp_fa" 2>/dev/null; then
            if ! compare_json_files "$tmp_fa" "$res_dir/federated-authority-config.json" "federated-authority-config.json"; then
                all_passed=false
            fi
        else
            print_error "Failed to regenerate federated-authority-config.json"
            all_passed=false
        fi
    else
        print_info "Skipping federated-authority-config.json verification."
    fi

    # 1d. Regenerate permissioned-candidates-config.json
    if confirm "  Verify permissioned-candidates-config.json?" "y"; then
        print_substep "Regenerating permissioned-candidates-config.json..."
        local tmp_pc="$tmp_dir/permissioned-candidates-config.json"

        if "$node_binary" generate-permissioned-candidates-genesis --cardano-tip "$cardano_tip" --output "$tmp_pc" 2>/dev/null; then
            if ! compare_json_files "$tmp_pc" "$res_dir/permissioned-candidates-config.json" "permissioned-candidates-config.json"; then
                all_passed=false
            fi
        else
            print_error "Failed to regenerate permissioned-candidates-config.json"
            all_passed=false
        fi
    else
        print_info "Skipping permissioned-candidates-config.json verification."
    fi

    # 1e. Regenerate reserve-config.json
    if uses_reserve_config "$network"; then
        if confirm "  Verify reserve-config.json?" "y"; then
            print_substep "Regenerating reserve-config.json..."
            local tmp_reserve="$tmp_dir/reserve-config.json"

            if "$node_binary" generate-reserve-genesis --cardano-tip "$cardano_tip" --output "$tmp_reserve" 2>/dev/null; then
                if ! compare_json_files "$tmp_reserve" "$res_dir/reserve-config.json" "reserve-config.json"; then
                    all_passed=false
                fi
            else
                print_error "Failed to regenerate reserve-config.json"
                all_passed=false
            fi
        else
            print_info "Skipping reserve-config.json verification."
        fi
    fi

    echo ""
    if [[ "$all_passed" == "true" ]]; then
        print_success "Step 1: All config files match!"
        return 0
    else
        print_error "Step 1: Some config files differ. See differences above."
        return 1
    fi
}

# ===========================================================================
# VERIFICATION STEP 2: Inspect LedgerState
# ===========================================================================
run_ledger_state_verification() {
    local network="$1"
    local node_binary="$2"
    local tmp_dir="$3"

    print_step "Step 2: Verify LedgerState from Chain Spec"

    echo -e "${BOLD}This step inspects the genesis_state in chain-spec-raw.json${NC}"
    echo -e "${BOLD}and verifies its contents.${NC}"
    echo ""

    local all_passed=true
    local res_dir="$REPO_ROOT/res/$network"
    local chain_spec="$res_dir/chain-spec-raw.json"

    if [[ ! -f "$chain_spec" ]]; then
        print_error "chain-spec-raw.json not found at $chain_spec"
        return 1
    fi

    # Run the verify-ledger-state-genesis command from the node
    print_substep "Extracting and verifying genesis state..."

    local inspect_result
    if ! inspect_result=$("$node_binary" verify-ledger-state-genesis \
        --chain-spec "$chain_spec" \
        --cnight-config "$res_dir/cnight-config.json" \
        --ledger-parameters-config "$res_dir/ledger-parameters-config.json" \
        --cardano-tip-config "$res_dir/cardano-tip.json" \
        --network "$network" \
        2>&1); then
        print_error "Failed to verify genesis state"
        echo "$inspect_result"
        return 1
    fi

    echo "$inspect_result"

    # Parse the inspection results
    # The command outputs structured results that we can parse

    # 2a. Check DustState
    if echo "$inspect_result" | grep -q "DUST_STATE_OK"; then
        print_success "2a. DustState matches cnight-config.json system_tx"
    else
        print_error "2a. DustState does not match cnight-config.json system_tx"
        all_passed=false
    fi

    # 2b. Check empty state for mainnet
    if [[ "$network" == "mainnet" ]]; then
        if echo "$inspect_result" | grep -q "EMPTY_STATE_OK"; then
            print_success "2b. Empty state verified (utxo, zswap, contract are empty)"
        else
            print_error "2b. State is not empty - faucets may have been funded!"
            all_passed=false
        fi
    else
        print_info "2b. Empty state check skipped (not mainnet)"
    fi

    # 2c. Check total NIGHT amount invariance
    if echo "$inspect_result" | grep -q "SUPPLY_INVARIANT_OK"; then
        print_success "2c. Total NIGHT supply is 24B (treasury + reserve_pool = MAX_SUPPLY)"
    else
        print_error "2c. Total NIGHT supply invariant violated!"
        all_passed=false
    fi

    # 2d. Check LedgerParameters
    if echo "$inspect_result" | grep -q "LEDGER_PARAMETERS_OK"; then
        print_success "2d. LedgerParameters match ledger-parameters-config.json"
    else
        print_error "2d. LedgerParameters do not match ledger-parameters-config.json"
        all_passed=false
    fi

    echo ""
    if [[ "$all_passed" == "true" ]]; then
        print_success "Step 2: LedgerState verification passed!"
        return 0
    else
        print_error "Step 2: LedgerState verification failed. See errors above."
        return 1
    fi
}

# ===========================================================================
# VERIFICATION STEP 3: Check Dparameter
# ===========================================================================
run_dparameter_verification() {
    local network="$1"

    print_step "Step 3: Verify Dparameter Configuration"

    echo -e "${BOLD}This step verifies system-parameters-config.json${NC}"
    echo -e "${BOLD}matches permissioned-candidates-config.json.${NC}"
    echo ""

    local all_passed=true
    local res_dir="$REPO_ROOT/res/$network"
    local system_params="$res_dir/system-parameters-config.json"
    local perm_candidates="$res_dir/permissioned-candidates-config.json"

    if [[ ! -f "$system_params" ]]; then
        print_error "system-parameters-config.json not found"
        return 1
    fi

    if [[ ! -f "$perm_candidates" ]]; then
        print_error "permissioned-candidates-config.json not found"
        return 1
    fi

    # Check num_registered_candidates == 0
    local num_registered
    num_registered=$(jq -r '.d_parameter.num_registered_candidates // "null"' "$system_params" 2>/dev/null)

    if [[ "$num_registered" == "0" ]]; then
        print_success "3a. num_registered_candidates == 0"
    else
        print_error "3a. num_registered_candidates should be 0, but is: $num_registered"
        all_passed=false
    fi

    # Check num_permissioned_candidates matches count of initial_permissioned_candidates
    local num_permissioned
    num_permissioned=$(jq -r '.d_parameter.num_permissioned_candidates // "null"' "$system_params" 2>/dev/null)

    local actual_count
    actual_count=$(jq '.initial_permissioned_candidates | length' "$perm_candidates" 2>/dev/null)

    if [[ "$num_permissioned" == "$actual_count" ]]; then
        print_success "3b. num_permissioned_candidates ($num_permissioned) matches initial_permissioned_candidates count"
    else
        print_error "3b. num_permissioned_candidates ($num_permissioned) does not match initial_permissioned_candidates count ($actual_count)"
        all_passed=false
    fi

    echo ""
    if [[ "$all_passed" == "true" ]]; then
        print_success "Step 3: Dparameter verification passed!"
        return 0
    else
        print_error "Step 3: Dparameter verification failed. See errors above."
        return 1
    fi
}

# ===========================================================================
# VERIFICATION STEP 4: Verify Authorization Scripts for Upgradable Contracts
# ===========================================================================
run_auth_script_verification() {
    local network="$1"
    local db_connection="$2"
    local cardano_tip="$3"
    local node_binary="$4"

    print_step "Step 4: Verify Authorization Scripts for Upgradable Contracts"

    echo -e "${BOLD}This step verifies that all upgradable contracts (Federated Authority,${NC}"
    echo -e "${BOLD}ICS, Permissioned Candidates) use the expected authorization script.${NC}"
    echo ""
    echo -e "${BOLD}For each contract, it checks:${NC}"
    echo -e "  1. The compiled_code hash matches the policy_id"
    echo -e "  2. The two_stage_policy_id is embedded in the compiled_code"
    echo -e "  3. The authorization script observed on Cardano matches the expected value"
    echo ""

    cd "$REPO_ROOT"
    export CFG_PRESET="$network"
    export ALLOW_NON_SSL=true
    export DB_SYNC_POSTGRES_CONNECTION_STRING="$db_connection"

    local check_result
    if check_result=$("$node_binary" verify-auth-script --cardano-tip "$cardano_tip" 2>&1); then
        echo "$check_result"
        echo ""
        print_success "Step 4: All authorization script checks passed!"
        return 0
    else
        echo "$check_result"
        echo ""
        print_error "Step 4: Some authorization script checks failed!"
        return 1
    fi
}

# Main script
main() {
    print_header "Midnight Genesis Verification Tool"

    echo "This tool verifies the chain specification for a network."
    echo "It performs the following checks:"
    echo ""
    echo -e "  0. ${BOLD}Cardano Tip Finalization${NC} - Verifies the Cardano tip has enough confirmations"
    echo -e "  1. ${BOLD}Config File Regeneration${NC} - Regenerates config files and compares with existing"
    echo -e "  2. ${BOLD}LedgerState Verification${NC} - Verifies genesis_state contents from chain-spec-raw.json"
    echo -e "     a. DustState matches cnight-config.json system_tx"
    echo -e "     b. Empty state for mainnet (no faucet funding)"
    echo -e "     c. Total NIGHT supply invariance (24B)"
    echo -e "     d. LedgerParameters match config"
    echo -e "  3. ${BOLD}Dparameter Verification${NC} - Verifies system-parameters-config.json consistency"
    echo -e "  4. ${BOLD}Auth Script Verification${NC} - Verifies upgradable contracts share the same auth script"
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

    # Get security parameter from pc-chain-config.json
    local security_param
    security_param=$(get_security_parameter "$network")
    if [[ -z "$security_param" ]]; then
        security_param="432"
        print_warning "Could not read security_parameter from pc-chain-config.json, using default: $security_param"
    fi

    # Collect inputs
    print_step "Configuration"

    echo -e "${BOLD}These inputs are needed for verification:${NC}"
    echo ""

    local db_connection
    db_connection=$(prompt_input "DB Sync PostgreSQL connection string" "postgres://postgres:postgres@localhost:5432/cexplorer")
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

    # Show configuration summary
    echo -e "${BOLD}Configuration Summary:${NC}"
    echo -e "  Network:              ${CYAN}$network${NC}"
    echo -e "  Security Parameter:   ${CYAN}$security_param${NC}"
    echo -e "  Cardano Tip:          ${CYAN}$cardano_tip${NC}"
    echo ""

    # Ensure node binary exists
    local node_binary
    node_binary=$(ensure_node_binary) || exit 1

    # Create temporary directory
    local tmp_dir
    tmp_dir=$(create_temp_dir)
    print_info "Using temporary directory: $tmp_dir"
    echo ""

    # Track verification results: "pass", "fail", or "skip"
    local step0_result="fail"
    local step1_result="fail"
    local step2_result="fail"
    local step3_result="fail"
    local step4_result="fail"
    local overall_passed=true

    # =========================================================================
    # STEP 0: Cardano Tip Finalization Check
    # =========================================================================
    if confirm "Run Step 0 (Cardano Tip Finalization Check)?" "y"; then
        if run_cardano_tip_finalization_check "$network" "$db_connection" "$cardano_tip" "$node_binary"; then
            step0_result="pass"
        else
            overall_passed=false
            if ! confirm "Cardano tip is not finalized. Continue anyway?" "n"; then
                print_error "Verification aborted. Please provide a finalized Cardano tip."
                rm -rf "$tmp_dir"
                exit 1
            fi
            print_warning "Continuing with unfinalized Cardano tip (results may be unreliable)."
        fi
    else
        step0_result="skip"
        print_info "Skipping Step 0."
    fi

    # =========================================================================
    # STEP 1: Config File Regeneration and Comparison
    # =========================================================================
    if confirm "Run Step 1 (Config Files Verification)?" "y"; then
        if run_config_regeneration_verification "$network" "$db_connection" "$cardano_tip" "$security_param" "$node_binary" "$tmp_dir"; then
            step1_result="pass"
        else
            overall_passed=false
            if ! confirm "Continue despite Step 1 failures?" "n"; then
                print_error "Verification aborted."
                rm -rf "$tmp_dir"
                exit 1
            fi
        fi
    else
        step1_result="skip"
        print_info "Skipping Step 1."
    fi

    # =========================================================================
    # STEP 2: LedgerState Verification
    # =========================================================================
    if confirm "Run Step 2 (LedgerState Verification)?" "y"; then
        if run_ledger_state_verification "$network" "$node_binary" "$tmp_dir"; then
            step2_result="pass"
        else
            overall_passed=false
            if ! confirm "Continue despite Step 2 failures?" "n"; then
                print_error "Verification aborted."
                rm -rf "$tmp_dir"
                exit 1
            fi
        fi
    else
        step2_result="skip"
        print_info "Skipping Step 2."
    fi

    # =========================================================================
    # STEP 3: Dparameter Verification
    # =========================================================================
    if confirm "Run Step 3 (Dparameter Verification)?" "y"; then
        if run_dparameter_verification "$network"; then
            step3_result="pass"
        else
            overall_passed=false
        fi
    else
        step3_result="skip"
        print_info "Skipping Step 3."
    fi

    # =========================================================================
    # STEP 4: Auth Script Verification
    # =========================================================================
    if confirm "Run Step 4 (Auth Script Verification)?" "y"; then
        if run_auth_script_verification "$network" "$db_connection" "$cardano_tip" "$node_binary"; then
            step4_result="pass"
        else
            overall_passed=false
        fi
    else
        step4_result="skip"
        print_info "Skipping Step 4."
    fi

    # =========================================================================
    # Final Summary
    # =========================================================================
    print_header "Verification Summary"

    echo -e "Results for ${BOLD}$network${NC}:"
    echo ""

    local steps=("0:Cardano Tip Finalization" "1:Config File Regeneration" "2:LedgerState Verification" "3:Dparameter Verification" "4:Auth Script Verification")
    local results=("$step0_result" "$step1_result" "$step2_result" "$step3_result" "$step4_result")

    for i in "${!steps[@]}"; do
        local label="${steps[$i]}"
        local result="${results[$i]}"
        case "$result" in
            pass) echo -e "  ${GREEN}[PASS]${NC} Step $label" ;;
            skip) echo -e "  ${YELLOW}[SKIP]${NC} Step $label" ;;
            *)    echo -e "  ${RED}[FAIL]${NC} Step $label" ;;
        esac
    done

    echo ""

    # Cleanup
    rm -rf "$tmp_dir"
    print_info "Cleaned up temporary directory."

    if [[ "$overall_passed" == "true" ]]; then
        print_success "All verification checks passed!"
        exit 0
    else
        print_error "Some verification checks failed. See details above."
        exit 1
    fi
}

# Run main function
main "$@"
