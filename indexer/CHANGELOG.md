# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [4.4.0-rc.2] - 2026-08-06

### 🚀 Features

- *(indexer)* Index the ledger 8→9 hard-fork crossing (#1395).
- *(chain-indexer)* Derive block authors from BABE pre-runtime digests (#1321).
- *(chain-indexer)* Gate BABE author derivation on the presence of `pallet-consensus-engine` (#1377).

### 🐛 Bug Fixes

- Dispatch runtime API and storage calls by the state runtime version at upgrade enactment blocks (#1346).
- *(indexer-common)* Read the stored count in `ledger-db` `get_root_count` (#1378).
- Publish multi-architecture manifests for indexer images (#1391).

- *(indexer-common)* [**breaking**] Rename the ledger DB's `cache_size` to `cache_max_nodes`, make it a plain node count and raise it to 100000

  The value has always been a *number of arena nodes* (storage-core's own default is 10000), but it
  was parsed as a byte size, so the shipped `"1kiB"` meant 1024 nodes — about 100x below where it
  should be. Operators must rename the key in their config and replace any byte-unit string with a
  plain integer; the `APP__INFRA__LEDGER_DB__CACHE_SIZE` environment variable becomes
  `APP__INFRA__LEDGER_DB__CACHE_MAX_NODES`. `0` means unbounded.

### ⚙️ Miscellaneous Tasks

- Add `pallet_session` support for the 2.1.0 runtime upgrade (#1333).
- Pin `cargo-audit` (#1387).

## [unreleased]

## [4.3.5] - 2026-07-25

### 🐛 Bug Fixes

- *(chain-indexer)* Validate the first regular transaction against the parent block timestamp's mempool `tblock` bump

## [4.3.4] - 2026-07-24

### 🐛 Bug Fixes

- *(chain-indexer)* Avoid OutOfDustValidityWindow on the first transaction of a block by reproducing the node's mempool `tblock` bump (#1367)

## [4.3.3] - 2026-06-04

### 🚀 Features

- *(indexer-api)* Add subscription quotas to graphql websocket (#1104)
- *(indexer-api)* Tighten shielded nullifier transactions input validation (#1126)
- *(indexer-api)* Introduce @beta GraphQL directive for in-flight API fields (#1186)
- *(indexer-api)* Add per-tree end indexes to Block (#1197)
- *(indexer-api)* Rename DustNullifier byte fields with LeBytes suffix (#1199)
- *(indexer-api)* Add transaction reference to nullifier event types (#1208)

### 🐛 Bug Fixes

- *(indexer-api)* Deliver DustGenerationDtimeUpdate items on fresh dustGenerations subscriptions (#1166)

### ⚙️ Dependencies

- Bump midnight-ledger to 8.1.0 (#1141). Version bump only from 8.1.0-rc.1, no functional change for the indexer.
- Bump node to 1.0.0 (#1204). Test data regenerated against the 1.0.0 release tag (previously 1.0.0-rc.3); no functional change for the indexer.

## [4.3.2] - 2026-05-15

### 🚀 Features

- *(indexer-api)* Add /live HTTP endpoint for kubernetes liveness probe (#1145)

### 🐛 Bug Fixes

- *(indexer-api)* DustGenerations subscription terminates on chain progress, not per-wallet cursor (#1137)

### 📚 Documentation

- *(indexer-standalone)* Document mandatory APP__INFRA__SPO_NODE__BLOCKFROST_ID (#1149)

## [4.3.1] - 2026-05-11

### 🚀 Features

- *(indexer-api)* Add transactionHash to event subscription response types (#1116)

### 🐛 Bug Fixes

- Replace remaining production panics with error returns (#1105)

## [4.3.0] - 2026-04-30

### 🚀 Features

- *(indexer-standalone)* Run spo indexer in standalone sqlite mode (#979)
- *(cli)* Support --version without confg (#1083)
- *(indexer-api)* Include DustGenerationDtimeUpdate in dustGenerations subscription (#1080)

### 🐛 Bug Fixes

- *(indexer-common)* Keep in-memory pub/sub drain tasks alive after lag (#1063)
- *(indexer-api)* Tighten dust subscription input validation (#1090)
- *(indexer-api)* Avoid infinite redirect loop on unknown /api/v3 and /api/v4 paths (#1093)

## [4.2.1] - 2026-04-27

### ⚙️ Dependencies

- Bump to node 1.0.0-rc.3 and storage-core 1.2.0-rc.3 (#1077)

### 🚀 Features

- *(bench)* Add preprod genesis and multi-tx apply benchmarks (#1076)

## [4.2.0] - 2026-04-23

### 🚀 Features

- *(indexer-api)* Add dustGenerationMerkleTreeUpdate query (#1062)
- Dust generations QDO fields and generation tree index (#1059)

### 🐛 Bug Fixes

- *(indexer-common)* Clamp block_fullness on post_block_update to match node (#1061)

### ⚡ Performance

- *(chain-indexer)* Replace backward traversal with forward iteration by block height (#1038)

## [4.1.0] - 2026-04-20

### 🚀 Features

- Log wallet ID on connect and disconnect mutations (#935)
- *(indexer-api)* Add query for zswap collapsed update (#982)
- Support node v1.0 (#985)
- *(indexer-api)* Add zswap Merkle tree root to block (#988)
- Add efficient dust wallet synchronisation endpoints (#980)
- Remove support for node 0.20 and 0.21 (#986)
- Add shieldedNullifierTransactions subscription (#996)
- Bridge async-graphql tracing to fastrace (#1037)
- Avoid unnecessary scans of shielded transactions (#1039)
- Support ledger 8.1 (#1033)
- Add node 1.0.0-rc.2 metadata and remove rc.1 (#1041)

### 🐛 Bug Fixes

- Pin upload-sarif-github-action to latest SHA (KICS/Trivy disabled)
- Add start_period to postgres healthcheck in docker-compose (#936)
- *(chain-indexer)* Falling back to metadata of parent block (#970)
- Per-generation capacity calculation and cursor pagination for dust queries (#997)
- *(indexer-api)* Make standalone sqlite spo queries match postgres (#977)
- *(api)* Align dustCommitmentMerkleTreeUpdate with zswapMerkleTreeCollapsedUpdate pattern (#1023)
- Compute transaction fees using ledger's Transaction::fees() API (#1031)

### 🚜 Refactor

- Simplify and make recent code more consistent (#1035)

### ⚡ Performance

- *(indexer-api)* Introduce DataLoader for block-by-hash lookups (#1018)
- *(indexer-api)* Introduce DataLoader for transactions-by-block-id lookups (#1022)
- *(indexer-api)* Introduce DataLoader for contract-actions-by-transaction-id lookups (#1030)
- *(indexer-api)* Introduce DataLoader for transaction-by-id lookups (#1027)

### ⚙️ Miscellaneous Tasks

- *(security)* Fix vulnerability (#914)
- *(api)* Add fee field and deprecate estimatedFees and fees wrapper (#1036)
- Bump midnight-storage-core to 1.2.0-rc.2 (#1052)
- Remove unused node 0.20 and 0.21 data (#1042)

## [4.0.0] - 2026-03-17

## [4.0.0-rc.10] - 2026-03-16

### 🐛 Bug Fixes

- Flush pending inserts before delete/root-count to preserve batch_update ordering (#904)

## [4.0.0-rc.9] - 2026-03-16

### 🚜 Refactor

- Rename cardano_address DB column to cardano_stake_key for clarity (#902)

## [4.0.0-rc.8] - 2026-03-16

### 🚜 Refactor

- Batch ledger_db_nodes inserts to reduce SQL roundtrips (#892)

## [4.0.0-rc.7] - 2026-03-13

### 🚜 Refactor

- Replace window function with scalar subquery in ledger events query (#890)

## [4.0.0-rc.6] - 2026-03-12

## [4.0.0-rc.5] - 2026-03-06

### 🐛 Bug Fixes

- Use lazy loading for ledger state deserialization to avoid recursion depth limit (#871)

## [4.0.0-rc.4] - 2026-03-04

### 🚀 Features

- Use storage-core v1.1 with layout v2 (#861)

### ⚙️ Miscellaneous Tasks

- Drop year from copyright notices (#848)

## [4.0.0-rc.3] - 2026-03-02

### 🐛 Bug Fixes

- Prevent processes from hanging on termination (#844)

### 📚 Documentation

- Update copyright year (#845)

## [4.0.0-rc.2] - 2026-02-26

### 🚀 Features

- Add User-Agent header to subxt RPC client (#826)

### 🐛 Bug Fixes

- *(indexer-api)* Replace deterministic session ID with random per-session token (#807)

### ⚙️ Miscellaneous Tasks

- Make some magic numbers configurable (#817)

## [4.0.0-rc.1] - 2026-02-20

### 🚀 Features

- *(indexer-api)* /api/v3 alias for /api/v4 for backwards compatibility (#815)

### 🐛 Bug Fixes

- Prevent standalone pub-sub from busy spinning (#792)
- Use InitialNonce instead of HashOutput for night_utxo_hash in dtime update (#795)
- *(chain-indexer)* Fold system parameters update into save_block transaction (#793)
- *(chain-indexer)* Check all spent UTXOs are updated in save_spent_unshielded_utxos (#799)

### 🚜 Refactor

- Make protocol version an enum (#800)

### ⚙️ Miscellaneous Tasks

- *(chain-indexer)* Replace panic with result in storage impl (#791)
- Update debian trixie-slim base image to resolve CVE-2025-15467 (#801)

## [4.0.0-alpha.1] - 2026-02-16

### 🚀 Features

- Support ledger v8.0.0 (#766)
- Support node v0.22.0 (#770)
- Add ledger state translation (#773)
- *(indexer-api)* Add protocolVersion to ledger event types (#784)
- Bump API and package version to v4 (#788)
- Read genesis ledger state from chain spec (#772)

### 🐛 Bug Fixes

- *(chain-indexer)* Create ledger state for oldest version and translate (#780)
- *(indexer-api)* Only keep active wallets active (#775)
- *(indexer-api)* Correctly set next index for loading relevant transactions (#777)
- *(chain-indexer)* Treat unknown highest block on node as not caught up (#778)
- *(wallet-indexer)* Correctly set max_transaction_id (#783)
- *(indexer-api)* Prevent batch requests (#786)

### ⚙️ Miscellaneous Tasks

- *(chain-indexer)* Less verbose debug level logging (#779)
- Remove obsolete schema-v3.graphql (#789)

## [3.1.0] - 2026-02-24

### 🚀 Features

- *(chain-indexer)* Fetch genesis cNight registrations from pallet storage (#733)
- *(spo)* Integrate SPO indexer into midnight-indexer (#735)
- Add support for node 0.21 (#761)

### 🐛 Bug Fixes

- *(indexer-api)* Only stream relevant txs for active wallets (#743)
- *(indexer-api)* Remove encode() call on varchar column in stakePoolOperators query (#753)

### 🚜 Refactor

- Use ledger storage for ledger state persistence (#694)

### ⚙️ Miscellaneous Tasks

- *(indexer-api)* Include ledger parameters in block query (#745)
- Use pre-built GHCR image for spo-indexer in docker-compose and remove unused NATS dependency (#752)
- Add spo-indexer docs, clean up unused NATS config and misleading blockfrost placeholder (#754)
- Update debian trixie-slim base image to resolve CVE-2025-15467 (#801)

## [3.0.0] - 2026-01-28

### 🚀 Features

- Unify configuration across components (#20)
- Extend PostgreSQL pool configuration (#50)
- *(chain-indexer)* Start supporting node 0.13 (#21)
- *(indexer-api)* Remove deprecated health endpoint (#52)
- Rename ApplyStage to TransactionResult and add segment results (#54)
- Remove support for node 0.12 (#60)
- Add unshielded token support to indexer (merging feat/ut to main) (#62)
- Enhance unshielded token subscription with sync progress data (PM-17159) (#73)
- Enhance transaction metadata with status, fees, and execution results (#82)
- *(indexer-api)* Node-like default ordering of database query results (#102)
- Implement unshielded token ownership by contracts (#97)
- Add transaction ID offset parameter to unshielded subscription (#120)
- Improve error handling for network ID mismatches in unshielded address queries (#137)
- Support multiple ledger versions (#134)
- *(indexer-api)* Introduce ApiError with client and server errors (#147)
- *(indexer-api)* Remove obsolete metrics, add wallets connected gauge (#151)
- Rework unshielded API and implementation (#156)
- Enable tracing for GraphQL subscriptions (#179)
- Make checkmarx issues visible in github (#208)
- Pin actions (#211)
- Replace checkmarx.yaml with composite action (#268)
- Upgrade checkout action to latest version and pin to hash (#319)
- *(chain-indexer)* Remove hex decoding for transactions (#320)
- *(indexer-api)* Rework shielded transactions (#315)
- End to end support for ledger events (#359)
- Add dustLedgerEvents subscription (#366)
- *(indexer-api)* Support system transactions in API (#375)
- Use untagged serialization for viewing key (#378)
- Store and expose ledger parameters in GraphQL API (PM-19727, PM-19761) (#382)
- Add DUST registration tracking to UnshieldedUtxo (#391)
- Correctly use and expose byte types (#398)
- *(indexer-api)* Change API version from v1 to v3 (#418)
- *(api)* Add dustGenerationStatus query for cNIGHT tracking (#419)
- Add ctime to unshielded UTXO (#425)
- Change network ID from enum to string (#426)
- Integrate CardanoRewardAddress changes from latest Node (#505)
- Validate network ID is lowercase at startup (#518)
- Ensure non-0 exit codes (#524)
- *(api)* Add maxCapacity field to DustGenerationStatus (#552)
- *(api)* Change dustAddress from HexEncoded to Bech32m format (#554)
- *(api)* Add Cardano UTXO reference to dustGenerationStatus (#575)
- Validate saving ledger state (#600)
- *(nats)* Add num_replicas config for JetStream stream replication (#626)
- *(api)* Validate Cardano reward address network against Midnight network (#620)
- Add governance system parameters (D-Parameter and T&C) to GraphQL API (#637)
- Support ledger v7.0.0 (#717)
- Support node v0.20.0 (#720)

### 🐛 Bug Fixes

- *(indexer-api)* Make viewing update stream infinite (#32)
- *(chain-indexer)* Reset zswap state if storage is empty (#35)
- Abstract over runtime-dependent UtxoInfo type (#80)
- *(indexer-api)* Correctly determine highest relevant index (#94)
- *(indexer-api)* Correctly determine highest relevant index for standalone (#95)
- *(justfile)* Create target/data directory before running standalone indexer (#103)
- *(wallet-indexer)* Avoid race condition saving relevant transactions (#107)
- *(indexer-api)* Ensure consistent UTXO ordering by output_index in GraphQL API (#119)
- Allow target dir to be changed (#122)
- *(wallet-indexer)* Index wallets when freshly started (#124)
- Return client error for unknown block hash in subscriptions (#172)
- Correctly determine transaction relevance (#237)
- Resolve panic when querying transaction field on ContractAction interface (#243)
- Add --wait flag to docker compose commands to prevent race condition (#244)
- Configure nextest to prevent test cancellation on failure (#255)
- Update chain-indexer to use 0.13.5 metadata for node-dev-01 compatibility (#275)
- Correctly determine transaction relevance (#313)
- *(chain-indexer)* Fetch authorities for historic block (#318)
- Correct node version extraction in GitHub Actions workflow (#326)
- Restore genesis UTXO aggregation for test compatibility (#350)
- Allow manual kick off of repo (#351)
- Remove incorrect assertion on stream ordering in chain-indexer (#368)
- Handle SystemTransactionApplied events from MidnightSystem pallet (#381)
- Use same intent hash for ClaimRewards as ledger (#400)
- Use correct intent hash for spent UTXOs (#402)
- Skip UTXO creation for failed transactions (#401)
- *(chain-indexer)* Create unshielded UTXOs from system transactions (#408)
- *(indexer-api)* Correct index update order in shieldedTransactions subscription (#455)
- Always populate zswap_state for contract actions (#471)
- Populate dust_generation_info from DustInitialUtxo events (#517)
- *(chain-indexer)* Skip balance for failed contract actions (#527)
- *(api)* DustGenerationStatus query returns zeros for some fields (#530)
- Treat dust public key as variable length encoded (#538)
- *(indexer-api)* Prevent duplicate transactions (#573)
- *(chain-indexer)* Add timeout-based recovery for stuck subscriptions (#576)
- Correct ledger state divergence from TX ordering and failed TX cost handling (#592)
- *(chain-indexer)* Rebuild ledger state from correct block height (#664)
- *(indexer-api)* Remove duplicate SIGTERM handler to allow graceful shutdown (#685)

### 💼 Other

- Add support for .envrc.local (#254)
- Simplify node update process (#322)
- Add cargo-deny SARIF output to security scanning (#370)

### 🚜 Refactor

- *(wallet-indexer)* Minimize storage access (#42)
- Simpler handling of zswap state (#46)
- Remove subxt dependency from domain (#51)
- Remove redundant unshielded UTXO handling from storage (#64)
- Split indexer-api Storage into smaller parts (#69)
- *(indexer-api)* Move NoopStorage impls to respective submodules (#76)
- Align UnshieldedUtxoStorage with other storage traits (#74)
- *(indexer-api)* Break api/v1 module into smaller submodules (#143)
- Storage unification (#171)
- Remove unused code, better naming, etc. (#232)
- Rename GraphQL field from parameters to ledgerParameters (#384)
- Use ledger INITIAL_DUST_PARAMETERS via protocol-version-aware submodule (#558)

### 📚 Documentation

- Add "Running" section to README (#24)
- Add Development Setup section to README (#61)
- Correct misleading documentation in unshielded subscription (#85)
- Update the API documentation with transaction fees and unshielded progress tracking (#88)
- Update GraphQL API documentation to match current schema (#176)
- Add missing requirements to README (#253)
- Add comprehensive guide for updating node versions (#281)
- *(api)* Enhance v3 api documentation with DUST features and field updates (#473)
- Clarify currentCapacity limitations in dustGenerationStatus (#503)
- Extend comment for nightBalance, make clear it is in STAR (#551)

### ⚡ Performance

- Add composite index on transactions(variant, id) (#647)

### ⚙️ Miscellaneous Tasks

- Add more and consistent tracing and logging (#27)
- Add logging related to caught-up state (#53)
- Some code hygiene (#63)
- *(chain-indexer)* Improve some code style (#105)
- Remove obsolete TryFrom byte slice impl for ViewingKey (#126)
- Commit staged changes across repos (#282)
- Update midnight-node to 0.16.0-da0b6c69 (#309)
- *(cleanup)* Use where clauses for trait bounds where possible (#321)
- Improve node update process robustness (#323)
- Upgrade upload-sarif-github-action to use Checkmarx CLI v2.3.35 (#357)
- *(chain-indexer)* Cleanup SubxtNode error handling (#364)
- Cleanup ledger parameter implementation (#387)
- Some code hygiene (#404)
- *(indexer-api)* Cleanup storage implementation (#406)
- Remove unnecessary clone in ledger event storage (#420)
- Enable TLS for PostgreSQL (#422)
- Update Checkmarx action to latest version (#452)
- *(docker-compose)* Use named volume for node data (#463)
- *(test)* Increased API_MAX_COMPLEXITY of standalone in native_e2e.rs, matching the cloud config. (#474)
- Address various audit findings (#477)
- Further address audit (#495)
- Richer error messages (#494)
- *(chain-indexer)* Add some debug level logging (#541)
- Propagate system parameters update errors (#673)

## [2.1.4] - 2025-06-30

### 🐛 Bug Fixes

- *(indexer-api)* Correctly determine highest relevant index (#94)
- *(indexer-api)* Correctly determine highest relevant index for standalone (#95)
- *(wallet-indexer)* Avoid race condition saving relevant transactions (#107) (#110)
- Database migration (#111)

## [2.1.3] - 2025-06-10

### 🚀 Features

- Extend PostgreSQL pool configuration (#50)

### 🚜 Refactor

- *(wallet-indexer)* Minimize storage access (#42)

## [2.1.2] - 2025-05-27

### 🐛 Bug Fixes

- *(indexer-api)* Make viewing update stream infinite (#32)

### ⚙️ Miscellaneous Tasks

- Add more and consistent tracing and logging (#27)

## [2.1.1] - 2025-05-19

### 🐛 Bug Fixes

- *(indexer-api)* Queries return correct transactions and contract actions (#15)

### 🚜 Refactor

- *(chain-indexer)* Easier, more idiomatic way to apply transactions (#10)

## [2.1.0] - 2025-05-09

### 🚀 Features

- *(indexer-api)* Add permissive CORS middleware (#635)

## [2.0.0] - 2025-05-08

### 🚀 Features

- Bump node to 0.12 and ledger to 4.0 (#537)
- *(indexer-api)* Redesign wallet subscription ProgressUpdates (#591)
- *(indexer-api)* Add tracing to API (#597)
- *(indexer-api)* Add counters for all GraphQL operations (#603)
- *(indexer-api)* Clean naming and inputs (#617)
- Only support bech32m encoded keys/addresses (#602)
- *(indexer-api)* Rename contract query and subscription contract_action (#625)

### 🐛 Bug Fixes

- *(indexer-api)* Add missing logging for wallet subscription (#575)
- Add missing error logging for loading config (#577)
- Remove common-macro from Dockerfiles, update to Rust 1.86.0 (#583)
- *(indexer-api)* Skip collapsed update for failed transactions (#588)
- *(indexer-api)* Add transaction to ContractCallOrDeploy (#608)
- *(indexer-api)* Add deploy to ContractCall (#609)
- *(wallet-indexer)* Silence harmless database error in active_wallets (#619)

### 🚜 Refactor

- Use log, logforth and fastrace for telemetry (#544)
- Replace bytes attribute with byte newtypes (#574)
- Pass SessionId (Copy) by value (#594)
- Move main/run into main.rs (#596)
- *(indexer-api)* Pass block and tx hashes (arrays) by value (#610)
- *(indexer-api)* Lazy resolving of transactions and contract actions (#611)

### 📚 Documentation

- Updates, mainly reflecting API changes (#628)
- More updates/fixes for API doc (#630)

### ⚙️ Miscellaneous Tasks

- *(indexer-api)* Apply consistent error handling (#601)
- *(indexer-api)* More debug logging (#612)

## [2.0.0] - 2025-05-08

### 🚀 Features

- Bump node to 0.12 and ledger to 4.0 (#537)
- *(indexer-api)* Redesign wallet subscription ProgressUpdates (#591)
- *(indexer-api)* Add tracing to API (#597)
- *(indexer-api)* Add counters for all GraphQL operations (#603)
- *(indexer-api)* Clean naming and inputs (#617)
- Only support bech32m encoded keys/addresses (#602)
- *(indexer-api)* Rename contract query and subscription contract_action (#625)

### 🐛 Bug Fixes

- *(indexer-api)* Add missing logging for wallet subscription (#575)
- *(indexer-api)* Skip collapsed update for failed transactions (#588)
- *(indexer-api)* Add transaction to ContractCallOrDeploy (#608)
- *(indexer-api)* Add deploy to ContractCall (#609)
- *(wallet-indexer)* Silence harmless database error in active_wallets (#619)

### 🚜 Refactor

- Use log, logforth and fastrace for telemetry (#544)
- Replace bytes attribute with byte newtypes (#574)
- Pass SessionId (Copy) by value (#594)
- Move main/run into main.rs (#596)
- *(indexer-api)* Pass block and tx hashes (arrays) by value (#610)
- *(indexer-api)* Lazy resolving of transactions and contract actions (#611)

### 📚 Documentation

- Updates, mainly reflecting API changes (#628)
- More updates/fixes for API doc (#630)

### ⚙️ Miscellaneous Tasks

- *(indexer-api)* Apply consistent error handling (#601)
- *(indexer-api)* More debug logging (#612)

## [1.0.1] - 2025-04-01

### 🐛 Bug Fixes

- *(indexer-api)* Send correct ProgressUpdates on reconnect (#531)
- Wallet subscription keeps wallet active (#510)

### 📚 Documentation

- *(adr)* Use only bech32m format for unshielded address and remove bls/hex mentions (#486)
- *(decision)* Record decision to replace Scala indexer docs with Rust indexer docs (#13933) (#427)

### ⚙️ Miscellaneous Tasks

- Rename local to standalone (#508)

## [1.0.0] - 2025-03-24

The Midnight Indexer 1.0.0 is the first release of the Rust-based indexer, replacing the previous Scala implementation. This version improves performance, modularity, and deployment flexibility. The indexer efficiently processes data from Midnight network, providing a GraphQL API for queries and real-time subscriptions.

<!-- generated by git-cliff -->
