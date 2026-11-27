# Hard fork test suite

Tests a ledger hard fork, ledger X to ledger X+1, the way the QA hard fork runbook describes
it (`releases/release-process/hardfork-runbook.md` in the QA repository). The runbook has ten
groups under "What to test"; this suite has one script per group, and each check sits under
the runbook bullet it covers.

The same checks run against a local chain the suite forks itself and against a deployed
network that operators fork.

## The groups

| # | Group | Question it answers | Script |
|---|---|---|---|
| 1 | Release artefacts verification | Are the builds about to be deployed the ones that were tested? | `groups/g01_release_artefacts.sh` |
| 2 | Old-ledger features before the fork | Does every major feature work on the chain that is about to fork? | `groups/g02_old_ledger_features.sh` |
| 3 | Smoke test after each binary wave | Old and new binaries side by side: is the chain still healthy and usable? | `groups/g03_wave_smoke.sh` |
| 4 | Fork block integrity | Was anything lost, did the chain pause, did a transaction get stuck at the fork block? | `groups/g04_fork_block_integrity.sh` |
| 5 | Ledger state migration | Did the migration change exactly what its design says, and can users recover what was reset? | `groups/g05_state_migration.sh` |
| 6 | Pre-fork state still works after the fork | Can users still spend, call and finish what they had before the fork? | `groups/g06_pre_fork_state_after_fork.sh` |
| 7 | New-ledger features after the fork | Does everything from group 2 work again from fresh state, and does every new feature work? | `groups/g07_new_ledger_features.sh` |
| 8 | Outdated and invalid inputs are refused | Does each case end in a clear error, with no lost funds, no hang and no damaged chain? | `groups/g08_refusals.sh` |
| 9 | Indexer, wallets and SDKs across the fork | Do the services and client libraries follow the chain through the fork and read both sides? | `groups/g09_clients_across_fork.sh` |
| 10 | Failure and recovery drills | Does the network survive nodes dying, lagging and being wiped, and does the safety net work? | `groups/g10_failure_and_recovery.sh` |

Group 7's regression matrix is group 2's feature checks run again: both source
`groups/ledger_features.sh`. Group 2 runs them on the old ledger with the wallets of the
seeds file; group 7 on the new ledger with wallets that never transacted before and newly
deployed contracts and dApps.

## Reading the code

- **A check** is one function, `check_transfer_shielded`, that ends with one verdict: `pass`,
  `fail`, `warn`, `skip` or `known`. Its ID is the group plus its name: `G2-transfer-shielded`.
- **A group script** reads top to bottom: the scene (which side of the fork, which wallets),
  the check functions, then the list of checks under their runbook bullets.
- **Wallets have roles**, not numbers: `SENDER`, `RECEIVER`, `HOLDER` (keeps the unsent
  old-ledger transaction), `SPARE`, `CROSSER` (transacts across the fork block). See
  `lib/wallets.sh`.
- **Contracts have names**: `store-a`, `store-b`, `counter`, `bboard`, `micro-dao`, `vault`,
  `game-draw`, `game-half-revealed`, `game-waiting`. Group 7's are `fresh-<name>`.
- **"old" and "new"** are the two sides of the fork everywhere: `use_ledger_side old`.

```
run.sh                 run.sh local [step...]   |   run.sh <network> <stage>
groups/                one script per runbook group, and ledger_features.sh
forks/<fork>/          everything that differs from one hard fork to the next
lib/                   node, toolkit, indexer, proof server, wallets, results
clients/               the wallet SDK and Midnight.js checks, as an application would use them
dapps/                 counter, bboard, micro-dao, vault, rps (the games)
local/                 the operators on local-env: start, rollout, wipe a validator, wipe the indexer
network/               the stages for a deployed network: setup, drop-out, fork watcher
report/                the report generator
```

## Running it on local-env

Needs Docker, bash 4 or later, node, npm, jq, python3 and a logged-in `gh`. About 100 minutes.

```
tests/hardfork/run.sh local              # everything, on a fresh chain
tests/hardfork/run.sh local g7 g8        # some steps, on the chain that is already up
```

Steps: `start g1 g2 rollout g4 g5 g6 g7 g8 g9 g10`. On local-env the suite also plays the
operators: `start` brings up a chain on the old release, `rollout` swaps the binaries in two
waves (group 3 after each), records the state (group 4), starts the crosser's wallet, submits
the runtime upgrade and drops validators out (group 10). `LAGGARD=3` keeps validator 3 on
the old binary through the fork instead of stopping one.

Everything is written under `target/hardfork-<fork>/local/`.

## Running it on a network

The operators swap the binaries and submit the runtime upgrade; the suite only checks, one
stage after each of their steps. `network/run.sh` lists the stages in order.

```
export SEEDS_FILE=~/secrets/qanet-seeds.env        # from network/seeds.env.example
tests/hardfork/run.sh qanet preflight
tests/hardfork/run.sh qanet g2
tests/hardfork/run.sh qanet g3 1
tests/hardfork/run.sh qanet g4-record
tests/hardfork/run.sh qanet g4-crosser             # second terminal, a minute before the upgrade
tests/hardfork/run.sh qanet after-fork
```

Use one machine and one checkout for the whole fork and keep `target/hardfork-<fork>/network-<name>/`:
later stages compare against what earlier ones recorded there.

## Results

| Where | What |
|---|---|
| `results/G2.tsv` | one row per check of a group: status, runbook bullet, what it checks, detail, seconds |
| `results/SUMMARY.md` | every group, rebuilt after each script |
| `report/REPORT.md`, `report/report.html` | the report: verdict, coverage against the runbook, failures, known issues, what did not run |
| `evidence/<group>/` | the raw output of every check |
| `state/` | what one stage recorded for the later ones |
| `runs/<time>/` | the log of each step of a run |

Statuses: **PASS**; **FAIL**; **WARN**, it ran and saw something unexpected that is not a
defect of the fork; **SKIP**, it did not run, with the reason; **KNOWN**, a limitation filed
upstream, and only when the evidence matches that issue.

The suite fails closed: a check that ends without a verdict is a FAIL, a script that stops
half-way records a `script-did-not-finish` FAIL, and a check that cannot read its data fails.
Running a check again replaces its earlier row. A runbook bullet nothing tests yet keeps a
SKIP row that says so, so the gap stays visible.

## The next fork

Copy `forks/ledger-8-to-9/` to `forks/ledger-9-to-10/` and remove the old directory (or set
`FORK_DIR`). Then, file by file:

| File | What to do |
|---|---|
| `versions.sh` | the release matrix: one setting per version, and the node's error numbers group 8 expects |
| `required-fixes.tsv` | the fixes the releases must carry |
| `known_issues.sh` | the filed limitations a check may report as KNOWN; start empty |
| `g01_release_artefacts.sh` | this release's dependency pins and the regressions of its security fixes |
| `g02_old_ledger_features.sh` | what the new tooling must do, and refuse, on the old chain |
| `g05_state_migration.sh` | the migration's checks; delete the file if the fork migrates no state |
| `g07_new_ledger_features.sh` | one check per new feature, the new parameters, and a `not_covered` row per gap |
| `rollout_local.sh` | what the releases need on local-env; delete what no longer applies |
| `clients/package.json` | the client releases after the fork; the scripts that use them are in `clients/` at the top and take the two ledgers from `versions.sh` |
| `clients-legacy/` | the wallet SDK from before the fork, for the wallet that never upgraded |
| `dapps/` | the dApps of the new features |
| `networks/` | one settings file per network |

A fork's file has the name of the group it adds to. Nothing under `groups/`, `lib/`,
`clients/`, `local/` or `network/` should need a change, unless a client release changes its
API; if a check there turns out to be about one fork only,
move it to the fork.
