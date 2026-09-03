#toolkit
# Recognise the 2.1.1 runtime when fetching blocks

The block fetcher rejects any block whose `spec_version` it does not know, so raising
`spec_version` to `002_001_001` in this branch made every block of the chain this same ref
produces fail with `UnsupportedBlockVersion(2001001)` — the fetch path (`fetch`,
`generate_txs_round_robin.py`, `tx_load_applier.py`, funding and dust registration) could not
read the network its own node binary was running.

`RuntimeVersion` gains a `V2_1_1` variant mapped to that spec version. It reuses the 2.1.0
subxt metadata snapshot: spec 2_001_001 changes only `frame_system::BlockLength` (a constant)
and the default of `ConfigurableTransactionSizeWeight` (a storage-entry default), neither of
which touches the extrinsic envelope or the event types the decoder reads — the same reasoning
under which `MidnightMetadata2_0_0` already reuses the 1.0.0 snapshot. Once the runtime
metadata is rebuilt for this spec, the variant should bind its own
`metadata/static/midnight_metadata_2.1.1.scale`.
