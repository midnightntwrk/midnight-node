#node #genesis #tooling
# Genesis verification tool

Added a comprehensive genesis verification tool for validating chain specifications before network launch.

## New CLI Commands

### Genesis Verification
- `verify-ledger-state-genesis` - Verifies genesis state from chain-spec-raw.json (DustState, supply invariant, parameters)
- `verify-cardano-tip-finalized` - Verifies a Cardano block has enough confirmations based on security_parameter
- `verify-auth-script` - Verifies all upgradable contracts use the expected authorization script
- `verify-federated-authority-auth-script` - Verifies federated authority contract auth scripts
- `verify-ics-auth-script` - Verifies ICS validator contract auth scripts
- `verify-permissioned-candidates-auth-script` - Verifies permissioned candidates contract auth scripts

## Interactive Verification Script

New interactive script `scripts/genesis/genesis-verification.sh` that performs 5 verification steps:
- Step 0: Cardano tip finalization check
- Step 1: Config file regeneration and comparison
- Step 2: LedgerState verification (DustState, supply invariant, parameters)
- Step 3: Dparameter verification
- Step 4: Authorization script verification for upgradable contracts

## Additional Changes

- Both genesis scripts now prefill the Cardano tip prompt from `res/<network>/cardano-tip.json` if available
- Reorganized genesis code into `node/src/genesis/creation/` and `node/src/genesis/verification/` modules
- Added comprehensive documentation in `docs/genesis/verification.md`

PR: https://github.com/midnightntwrk/midnight-node/pull/654
Ticket: https://shielded.atlassian.net/browse/PM-20831
