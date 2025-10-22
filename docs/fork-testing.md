# Fork Testing with Snapshots

You can fork an existing chain by creating a snapshot of its state as of a moment in time, and restoring that snapshot to a locally running version of that network.

## Bootnode snapshots

The Midnight tools can capture the `/node` volume from a Kubernetes bootnode and
upload the archive to any S3-compatible object store. They can also restore the
archive locally before starting Docker Compose services.

### Local MinIO setup (optional)

You can run a local MinIO server to store snapshots while developing:

```bash
docker run \
  -p 9000:9000 \
  -p 9001:9001 \
  -e MINIO_ROOT_USER=midnight \
  -e MINIO_ROOT_PASSWORD=midnight123 \
  -v "$(pwd)/minio-data:/data" \
  --name midnight-minio \
  minio/minio server /data --console-address ":9001"
```

Create a bucket for snapshots using the AWS CLI (make sure it is installed
locally):

```bash
AWS_ACCESS_KEY_ID=midnight \
AWS_SECRET_ACCESS_KEY=midnight123 \
aws --endpoint-url http://127.0.0.1:9000 s3 mb s3://midnight-node-snapshots
```

Set the environment variables expected by the snapshot tooling:

```bash
export AWS_ACCESS_KEY_ID=midnight
export AWS_SECRET_ACCESS_KEY=midnight123
export MN_SNAPSHOT_S3_ENDPOINT_URL=http://127.0.0.1:9000
export MN_SNAPSHOT_S3_URI=s3://midnight-node-snapshots
```

### Creating a snapshot

Ensure your `kubectl` context points to the target Midnight cluster. Then run
one of the provided npm scripts to capture a snapshot from the bootnode of a
well-known namespace:

```bash
npm run snapshot:qanet
npm run snapshot:devnet
npm run snapshot:testnet-02
npm run snapshot:node-dev-01
```

By default the snapshot pod uploads to `MN_SNAPSHOT_S3_URI`. You can override
the destination on a per-run basis:

```bash
npm run snapshot:qanet -- --s3-uri s3://midnight-node-snapshots/qanet/latest.tar.zst
```

If your bootnode uses a different StatefulSet, PVC name, or you want to override
the helper image, pass `--bootnode`, `--pvc`, or `--snapshot-image` respectively.

### Restoring a snapshot locally

The `run`, `image-upgrade`, and `runtime-upgrade` commands can automatically
restore a bootnode snapshot before launching Docker Compose services. This
requires the AWS CLI (for `aws s3 cp`) and `zstd` if the archive is compressed.

```bash
npm run run:qanet -- --from-snapshot latest.tar.zst
```

The value passed to `--from-snapshot` can be either a full S3 URI
(`s3://bucket/path.tar.zst`) or a key relative to `MN_SNAPSHOT_S3_URI`. The
credentials and endpoint variables from the MinIO example above are re-used for
restores.