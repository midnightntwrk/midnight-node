# Running the fork tests against a network

The network's operators roll the fork out: they move validators to the 2.1.0 binary, upgrade
the indexer and submit the runtime upgrade. You run one stage of the suite after each of
their steps.

```
preflight -> baseline -> [wave 1] -> wave 1 -> ... -> [last wave, indexer] -> snapshot
          -> [set_code] -> watch -> post
```

The steps in brackets are the operators'. Run everything from the repository root:

```sh
R=tests/hardfork/ledger-8-to-9/network/run.sh
```

The node RPC must keep old runtime state (an archive node, or at least state from before the
fork): the fork block, the pre-fork contract data and the ledger-8 checks read old blocks.

Use the same machine and checkout for the whole fork. Each stage saves what later stages
compare against under `target/hardfork-8-to-9/network-<name>/state/`, so keep that directory
until you're done.

## Before the rollout

### 1. Agree on the versions

Check the defaults under [Versions and knobs](../README.md#versions-and-knobs) against what
the operators will deploy, and override any that differ. Make sure the indexer gets upgraded
to the build paired with the ledger-9 node before `set_code`, or it will stop at the fork
block.

### 2. Create four test wallets

Keep seeds out of the repository, in a file only you can read (mode 600; anything looser is
refused). Generate fresh ones: if another process spends from the same wallets, the transfer
checks get flaky. The toolkit takes seeds as command-line arguments, so they are visible in
`ps` and `docker inspect` while it runs: use a machine nobody else is logged into.

```sh
cp tests/hardfork/ledger-8-to-9/network/seeds.env.example ~/secrets/devnet-hf.seeds
chmod 600 ~/secrets/devnet-hf.seeds
for i in 1 2 3 4; do echo "SEED_$i=$(openssl rand -hex 32)"; done   # paste into the file
export SEEDS_FILE=~/secrets/devnet-hf.seeds
```

| Seed | Role |
|---|---|
| `SEED_1` | main sender; owns contract A and the dApps |
| `SEED_2` | second sender; funds contract B and sits on its committee |
| `SEED_3` | builds the ledger-8 transaction kept unsent until the fork; its NIGHT must not move until then, or the checks fail |
| `SEED_4` | receiver; the default ECDSA identity; optionally the cNIGHT-backed wallet |

### 3. Fund the wallets

Seeds 1-3 need NIGHT a few hours before the baseline, so DUST has time to build up. Get each
address from the ledger-8 toolkit, then use the faucet or send from a funded wallet:

```sh
set -a; . "$SEEDS_FILE"; set +a
docker run --rm ghcr.io/midnight-ntwrk/midnight-node-toolkit:1.0.400 \
    show-address --network devnet --seed "$SEED_1" --unshielded
```

For the cNIGHT check (`L9-DUST-3`), register one wallet with `scripts/cnight-generates-dust`
first and only then mint or move cNIGHT to it. The pallet only counts cNIGHT created after
the registration. Set `CNIGHT_SEED_INDEX` in the seeds file to that wallet.

### 4. Optional access

Without these, the checks that need them are skipped with a reason. Export them, or put them
in a copy of `env/<network>.env` and pass its path instead of the network name.

| Variable | Enables |
|---|---|
| `KUBE_CONTEXT`, `KUBE_NAMESPACE`, `KUBE_POD_REGEX` | validator logs and images |
| `EXTRA_RPC_NODES="name=https://... name2=https://..."` | cross-node agreement (check WAVE-4 of each WAVE-n table, `L9-AGREE-1..2`); with one RPC node nothing is compared |
| `EXPECTED_GENESIS_HASH` | a clear failure if the network was reset |
| `DEPLOYED_NODE_IMAGE`, `INDEXER_SOURCE_COMMIT` | the image the gdb check inspects, and the indexer source `SEC-PROV-3` checks |
| `PROOF_SERVER_L8`, `PROOF_SERVER_L9` | the network's proof servers instead of local containers |
| `SAFE_MODE_DRILL=1` and the governance key URIs | the SafeMode drill; it blocks every user transaction while it runs |
| `BRIDGE_TEST=1` and the bridge keys | a live cNIGHT -> NIGHT transfer |

Leave `PROOF_SERVER_L9` unset unless the network's proof server is a ledger-9 release. Older
ones can't prove ledger-9 DUST transactions.

### 5. Preflight, a day ahead

```sh
$R devnet preflight
```

Table PRE: the RPC, the indexer, the genesis, both toolkit images, the proof servers and the
client workspace. It also warms both toolkit caches, which takes hours the first time on a
long chain. Re-run it until every row passes.

## During the rollout

### 6. Baseline, before the first wave

```sh
$R devnet baseline
```

Deploys the contracts and dApps, records their state and the ledger parameters, and builds
the unsent ledger-8 transaction from seed 3, which the node must accept as valid now and reject
after the fork. Everything later compares against this.

### 7. A smoke after each wave

After each wave of validators moves to 2.1.0 (the runtime is still ledger 8):

```sh
$R devnet wave 1
$R devnet wave 2    # one per wave
```

If it fails, hold the next wave. Set `WAVE_EXPECTED_L9` to the number of RPC nodes that should
run 2.1.0 by now; WAVE-3 then fails when fewer do.

### 8. Snapshot, right before `set_code`

Once every validator runs 2.1.0 and the indexer is upgraded:

```sh
$R devnet snapshot
```

Take it as close to the upgrade as you can, and don't send anything from the test wallets
until the fork: after it, wallet NIGHT, the NIGHT pools (24B in total) and contract data must
read back unchanged.

### 9. Watch for the fork

```sh
watch -n 60 $R devnet watch
```

Each call prints one status line and writes no run directory. `watch_fork.sh` also runs on its
own with `NETWORK_ENV` (the env file's path) and `SEEDS_FILE` set; for scripts, it exits:

| Exit | Meaning |
|---|---|
| 0 | forked and finalizing; run `post` |
| 2 | still on ledger 8 |
| 3 | block production or finality stalled |
| 1 | a setup error (message on stderr), the RPC unreachable, or forked but the fork block can't be verified (the RPC node lacks old state) |

## After the fork

### 10. Post-fork checks, straight away

```sh
$R devnet post
```

This runs `snapdiff` first, before anything changes the state, then `l9`, `features`,
`clients-l9`, `security` and `safe-mode`. Each also runs on its own, e.g. `$R devnet l9`.

Results and the report end up under `target/hardfork-8-to-9/network-devnet/` (see
[Results](../README.md#results)). A stage with failing checks exits 1 after running all of
them. A stage that can't run exits 2 or higher, for example `wave` before `baseline`.

## If the network has already forked

Only `preflight`, `features`, `clients-l9`, `security` and `safe-mode` can run. `snapdiff`
needs the snapshot and `l9` needs the baseline: both exit 3 without them, and so does `post`,
which starts with `snapdiff`. Client checks on pre-fork dApps (C13, C15) skip with the reason.

```sh
$R devnet preflight
for s in features clients-l9 security safe-mode; do $R devnet "$s"; done
```
