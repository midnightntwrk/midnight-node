# Fork Testing with Snapshots

You can fork an existing chain by restoring a bootnode snapshot into a local
Docker Compose network and then swapping the live authority set for a synthetic
mock-validator set.

The local tooling only consumes snapshot archives. It no longer captures them
from Kubernetes directly. Obtain the archive from CI, a backup job, or another
external process first.

## Supported snapshot inputs

`--from-snapshot` currently accepts an `http://` or `https://` URL pointing to
one of these archive formats:

- `.tar`
- `.tar.gz` or `.tgz`
- `.tar.zst`

Local restore requires:

- `curl`
- `tar`
- `zstd` for `.zst` archives

## Finding snapshot archives

The backup system publishes snapshots of the well-known networks behind a
CloudFront distribution. The manifest at
`https://dg39snjayoq3t.cloudfront.net/index.json` lists every available
archive, keyed by network name. To see which networks currently have
snapshots:

```bash
curl -fsSL https://dg39snjayoq3t.cloudfront.net/index.json | jq 'keys'
```

As of this writing the index contains `devnet`, `qanet`, and `govnet`.
`preview`, `preprod`, and `mainnet` snapshots are not published there yet;
ask the node team if you need one.

To resolve the latest archive URL for a network (this mirrors what the
`fork-network` workflow's "Resolve latest snapshot URL" step does):

```bash
NETWORK=devnet
curl -fsSL https://dg39snjayoq3t.cloudfront.net/index.json |
  jq -r --arg net "$NETWORK" '
    .[$net] | to_entries | map(.value[]) | sort_by(.timestamp) | last
    | .s3_path | sub("^s3://[^/]+/"; "https://dg39snjayoq3t.cloudfront.net/")'
```

Pass the resulting URL to `--from-snapshot`. Drop the `last` selection to list
every archive for the network (older snapshots stay useful for forking a
specific block height or runtime version).

## Initial restore

On the first bring-up of a well-known network, pass the snapshot URL to `run`,
`image-upgrade`, `governance-runtime-upgrade`, or `full-upgrade`.

```bash
npm run run:qanet -- --from-snapshot https://example.com/snapshots/qanet-latest.tar.zst
```

The restore flow:

1. Downloads and extracts the archive.
2. Replicates the extracted node state into every compose `data/` mount for the
   selected network.
3. Runs `mock-authorities convert` over the restored state.
4. Generates a compose override that mounts the generated validator seeds and
   switches the main-chain follower into mock mode.

## Reusing an existing local fork

After the first restore succeeds, you can omit `--from-snapshot` on later runs
for the same network. The tooling will reuse the existing restored `data/`
directories and generated mock-authorities output.

```bash
npm run image-upgrade:qanet
npm run governance-runtime-upgrade:qanet -- \
  --wasm upgrade/midnight_node_runtime.compact.wasm \
  --council-uris //Dave //Eve //Ferdie \
  --technical-uris //Alice //Bob //Charlie \
  --executor-uri //Alice
```

If the generated fork-mode artifacts or restored `data/` directories are
missing, the command will fail fast and ask you to rerun with `--from-snapshot`.

## Chainspec compatibility

Before restoring a snapshot, confirm the chainspec embedded in the node image
was built with the same `networkId` as the genesis used to produce that
snapshot. Recent runtimes validate this at boot and refuse to start when the
network id does not match the restored state.
