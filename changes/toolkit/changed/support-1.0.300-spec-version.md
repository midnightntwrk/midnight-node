#toolkit
# Accept spec version 1.0.300 when replaying chain history

`RuntimeVersion::try_from` matched exact spec versions and knew `001_000_003` (1.0.3) but not
`001_000_300` (1.0.300), which devnet ran before the 2.1.0 hard fork. Any replay reaching a pre-fork
devnet block aborted with `UnsupportedBlockVersion(1000300)`, breaking `show-night-pools`,
`show-wallet` on a cold cache, and `generate-txs` without a warm one.

Both versions are ledger-8 era — `LedgerVersion::from_spec_version` already classifies the whole
`000_022_000..=001_999_999` range as Ledger8 — so they share a decoder and map to one variant.

Closes: #2158
PR: https://github.com/midnightntwrk/midnight-node/pull/2161
