#toolkit
# Accept spec version 1.0.300 when replaying chain history

`RuntimeVersion::try_from` matched exact spec versions and knew `001_000_003` (1.0.3) but not
`001_000_300` (1.0.300), which devnet ran before the 2.1.0 hard fork. Any replay reaching a
pre-fork devnet block aborted with `UnsupportedBlockVersion(1000300)`, breaking
`show-night-pools`, `show-wallet` on a cold cache, and `generate-txs` without a warm one. A cache
already covering the pre-fork range hid it, so it only surfaced on fresh machines.

The 1.0.3 and 1.0.300 runtime metadata differ only in the `spec_version` constant itself — the
type registry, pallet indices and call/event schemas are byte-identical — and both are ledger-8
era, since `LedgerVersion::from_spec_version` classifies the whole `000_022_000..=001_999_999`
range as Ledger8. So the two share a decoder and mapping them onto one variant is exact rather
than approximate.

Closes: #2158
