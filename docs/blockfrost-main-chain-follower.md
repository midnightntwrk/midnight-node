# Blockfrost main chain follower

The node reads Cardano main chain data through one of two backends. By default it queries a
**cardano-db-sync** PostgreSQL database, which means operating `cardano-node`, `cardano-db-sync`
and PostgreSQL alongside the node.

Setting `blockfrost_endpoint` switches all six main chain follower data sources to a
**Blockfrost-compatible HTTP API** instead. No Cardano node, no db-sync, no database.

Leave `blockfrost_endpoint` unset and nothing changes: the db-sync path is the default and is
unaffected.

## Configuration

| TOML key | Environment variable | Notes |
|----------|---------------------|-------|
| `blockfrost_endpoint` | `BLOCKFROST_ENDPOINT` | Base URL of a Blockfrost-compatible API. Must be `http` or `https`; validated at startup. |
| `blockfrost_project_id` | `BLOCKFROST_PROJECT_ID` | Secret. Required by hosted Blockfrost, usually unset for a self-hosted backend. ASCII alphanumeric. |

Config validation requires **either** `db_sync_postgres_connection_string` **or**
`blockfrost_endpoint`. Setting neither is a startup error.

### Hosted Blockfrost

```sh
BLOCKFROST_ENDPOINT=https://cardano-preview.blockfrost.io/api/v0
BLOCKFROST_PROJECT_ID=preview...
```

### Self-hosted

Any Blockfrost-compatible implementation works. Two known to serve the endpoints this backend needs:

- [Dolos](https://github.com/txpipe/dolos), which follows Cardano directly and needs no database.
  Requires [txpipe/dolos#1151](https://github.com/txpipe/dolos/pull/1151) for complete endpoint
  coverage, so until that is in a stable release you need a build containing it.
- [blockfrost-backend-ryo](https://github.com/blockfrost/blockfrost-backend-ryo), the official
  self-hosted API server. Note it sits on top of db-sync, so it does not remove the database.

Point the node at whichever you run and leave `blockfrost_project_id` unset:

```sh
BLOCKFROST_ENDPOINT=http://127.0.0.1:3000
```

## What is not supported

- **Genesis generation still requires db-sync.** The `generate-*-genesis` subcommands read Cardano
  data directly through SQL rather than through a data source. Running them with only
  `blockfrost_endpoint` set fails with a message saying so; they need
  `db_sync_postgres_connection_string`.
- **Partner-chains Prometheus metrics are unavailable on this path.** `McFollowerMetrics` is not
  writable from outside the `partner-chains-db-sync-data-sources` crate. The Midnight-side
  `MidnightDataSourceMetrics` is populated, using the same `midnight_data_source_query_time_elapsed`
  labels as the db-sync sources, so both backends can be compared in one dashboard.

## Operating cost

Roughly **2 requests per Midnight block**, so about **28,000 requests/day** for a node at tip.
Blockfrost's free tier (50,000/day) covers steady-state operation; the initial sync is the
expensive part.

| Network | Blocks | Bootstrap requests | Bootstrap time |
|---------|--------|--------------------|----------------|
| preview | ~226,000 | ~0.5M | ~12 h |
| preprod | ~1.91M | ~4.0M | ~1.5 days |
| mainnet | ~1.95M | ~4.05M | ~2.5 days |

Bootstrap times are with a plan whose rate limit is not the constraint; on the free tier the daily
cap sets the pace instead (preview is roughly ten days, mainnet is not practical). Bootstrap
request totals are floors, since cNIGHT and bridge activity grows over time.

Resource use is modest enough for small hardware: a Raspberry Pi 4 (4 GB) follows preview at tip
using about 816 MB RSS for the node, alongside a co-located Dolos at about 719 MB.

## Tuning

`cnight_observation_window_size` (default 100,000) sets how many Cardano blocks the cNIGHT
observation cache fetches per extension, and therefore roughly how much observation history it
retains. Lower it to reduce memory at the cost of more fetch rounds. It does **not** cap the work
of a single call: during catch-up one call extends until transaction capacity binds or coverage
reaches the referenced tip.

That catch-up cost is dominated by request count rather than per-request latency, so it is
noticeably cheaper against a co-located backend. At tip the same query is served from the in-memory
window and costs nothing either way.

## Troubleshooting

**Sync stops making progress, no errors.** Check for the over-quota message:

```
Blockfrost project is over its request limit (HTTP 402). Cardano main chain data cannot be read,
so block import and authoring will not progress until the daily quota resets or the project's
plan is upgraded.
```

402 is deliberately not retried, since a daily quota does not clear within a retry window.

**Node exits at startup with a URL error.** `blockfrost_endpoint` is parsed at startup rather than
per request, so a missing scheme fails immediately:

```
blockfrost_endpoint is not a valid URL (relative URL without a base): cardano-preview.blockfrost.io/api/v0
```

**403 "Network token mismatch".** The project id belongs to a different network than the endpoint.

**Connects but behaves oddly against a self-hosted backend.** If the backend listens on IPv6 (`[::]:3000`) and something else on the host holds IPv4 `127.0.0.1:3000`, the node will reach the wrong service. Use `http://[::1]:3000` explicitly.

**Request timing.** Every trait method and every HTTP call logs its duration under the `blockfrost`
log target: run with `-l blockfrost=debug`.

## Equivalence with db-sync

The db-sync SQL is the specification: both backends must produce identical inherent data, because
that data is consensus-critical.

`node/tests/blockfrost_parity.rs` is an `#[ignore]`d test that points both backends at the same
anchor block and asserts their results are equal. It requires a live db-sync and a live
Blockfrost-compatible endpoint for the same network:

```sh
DB_SYNC_POSTGRES_CONNECTION_STRING=postgres://... \
BLOCKFROST_ENDPOINT=https://cardano-preview.blockfrost.io/api/v0 \
BLOCKFROST_PROJECT_ID=preview... \
CNIGHT_MAPPING_VALIDATOR_ADDRESS=... CNIGHT_POLICY_ID=... \
COMMITTEE_CANDIDATE_ADDRESS=... PERMISSIONED_CANDIDATE_POLICY_ID=... \
FEDAUTH_COUNCIL_ADDRESS=... FEDAUTH_COUNCIL_POLICY_ID=... \
FEDAUTH_TECHNICAL_COMMITTEE_ADDRESS=... FEDAUTH_TECHNICAL_COMMITTEE_POLICY_ID=... \
BRIDGE_TOKEN_POLICY_ID=... ILLIQUID_CIRCULATION_SUPPLY_VALIDATOR_ADDRESS=... \
RESERVE_VALIDATOR_ADDRESS=... \
cargo test -p midnight-node --test blockfrost_parity -- --ignored --nocapture
```

The per-domain values come from `res/<network>/` (`cnight-addresses.json`, `ics-addresses.json`,
`federated-authority-addresses.json`) and the network's chain spec. Cardano network parameters
default to testnet values, so mainnet and preprod runs also need `CARDANO_SECURITY_PARAMETER`,
`MC__FIRST_EPOCH_TIMESTAMP_MILLIS`, `MC__EPOCH_DURATION_MILLIS`, `MC__FIRST_EPOCH_NUMBER` and
`MC__FIRST_SLOT_NUMBER` from `res/cfg/<network>.toml`.

`PARITY_WINDOW_BLOCKS` (default 1000) sets the look-back window. A domain whose inputs are not
supplied fails the run rather than being skipped silently; set `PARITY_ALLOW_PARTIAL=1` to accept a
partial comparison.

One caveat when interpreting a pass: a comparison over a range containing no activity is equality
between two empty sets and proves nothing. Check that the addresses under test actually have
transactions in the chosen window before treating a green run as meaningful.
