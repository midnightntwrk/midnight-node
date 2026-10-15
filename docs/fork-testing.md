# Fork Testing with Snapshots

You can fork an existing chain by restoring a bootnode snapshot into a local
Docker Compose network and then swapping the live authority set for a synthetic
mock-validator set.

The local tooling only consumes snapshot archives. It no longer captures them
from Kubernetes directly. Obtain the archive from CI, a backup job, or another
external process first.

## Supported snapshot inputs

`--from-snapshot` accepts an `http://` or `https://` URL, or a local archive
(a path or `file://` URL), in one of these formats:

- `.tar`
- `.tar.gz` or `.tgz`
- `.tar.zst`

A local archive skips the download, so fetch a large snapshot once and
reuse it across forks:

```bash
curl -fLo ~/snapshots/preview.tar.gz <snapshot URL>
npm run run:preview -- --from-snapshot ~/snapshots/preview.tar.gz --num-validators 3
```

Local restore requires:

- `curl` (URLs only)
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

To run fewer validators than the network's checked-in Compose topology, pass a
positive `--num-validators` value no larger than the configured
`mock.validatorServices` list:

```bash
npm run run:qanet -- \
  --from-snapshot https://example.com/snapshots/qanet-latest.tar.zst \
  --num-validators 3
```

The option selects the first N configured validator services, disables the
remaining validator services in the generated Compose override, and asks
`mock-authorities convert` to generate N keysets. It must be used with
`--from-snapshot`; changing an existing fork's count without regenerating its
authority data is rejected.

The restore flow:

1. Downloads and extracts the archive.
2. Runs `mock-authorities convert --node-dir` once over the extracted state.
3. Replicates the converted state into every compose `data/` mount for the
   selected network, validators and non-validators (e.g. `boot-node`) alike,
   so every peer starts from the same fork block.
4. Generates a compose override that mounts the generated validator seeds and
   switches the main-chain follower into mock mode. When `--num-validators` is
   provided, the override also disables validator services above that count.

## Reusing an existing local fork

After the first restore succeeds, you can omit `--from-snapshot` on later runs
for the same network. The tooling will reuse the existing restored `data/`
directories and generated mock-authorities output.

```bash
npm run image-upgrade:qanet
npm run governance-runtime-upgrade:qanet -- \
  --council-uris //Dave //Eve //Ferdie \
  --technical-uris //Alice //Bob //Charlie \
  --executor-uri //Alice
```

With no `--wasm`, the candidate runtime is extracted from `$NEW_NODE_IMAGE` (node
images ship it under `/artifacts-<arch>/`). Pass `--wasm-from-image <image>` to name
a different image, or `--wasm <path under artifacts/>` for a blob that is not in an
image — a release asset, say, which is the srtool build rather than the Earthly one.

If the generated fork-mode artifacts or restored `data/` directories are
missing, the command will fail fast and ask you to rerun with `--from-snapshot`.

## Chainspec compatibility

Before restoring a snapshot, confirm the chainspec embedded in the node image
was built with the same `networkId` as the genesis used to produce that
snapshot. Recent runtimes validate this at boot and refuse to start when the
network id does not match the restored state.
