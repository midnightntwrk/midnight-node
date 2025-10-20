set -euo pipefail

install_packages() {
  if command -v apt-get >/dev/null 2>&1; then
    apt-get update
    apt-get install -y "$@" && return 0
  fi

  return 1
}

if [ -z "${SNAPSHOT_S3_URI:-}" ]; then
  echo "SNAPSHOT_S3_URI must be provided" >&2
  exit 1
fi

if [ -z "${SNAPSHOT_S3_ENDPOINT_URL:-}" ]; then
  echo "SNAPSHOT_S3_ENDPOINT_URL must be provided" >&2
  exit 1
fi

if [ -z "${AWS_ACCESS_KEY_ID:-}" ] || [ -z "${AWS_SECRET_ACCESS_KEY:-}" ]; then
  echo "AWS_ACCESS_KEY_ID and AWS_SECRET_ACCESS_KEY must be provided" >&2
  exit 1
fi

if [ ! -d /node ]; then
  echo "Expected /node mount is missing" >&2
  exit 1
fi

yum -y makecache
yum -y install tar zstd

TIMESTAMP=$(date +%Y%m%d%H%M%S)
ARCHIVE_BASENAME="${BOOTNODE_NAME:-bootnode}-node-$TIMESTAMP"
ARCHIVE="/tmp/$ARCHIVE_BASENAME.tar.zst"

if command -v zstd >/dev/null 2>&1; then
  echo "Compressing /node/chain with zstd to $ARCHIVE"
  tar -cf - -C /node/chain . | zstd -T0 -19 -o "$ARCHIVE"
else
  ARCHIVE="/tmp/$ARCHIVE_BASENAME.tar.gz"
  echo "zstd not available, using gzip fallback at $ARCHIVE"
  tar -czf "$ARCHIVE" -C /node/chain .
fi

# Avoid potentially previously set STS tokens for miniio compatibility
unset AWS_SESSION_TOKEN

aws s3 cp --endpoint-url "$SNAPSHOT_S3_ENDPOINT_URL" "$ARCHIVE" "$SNAPSHOT_S3_URI"

echo "Cleaning up temporary archive"
rm -f "$ARCHIVE"

echo "Snapshot complete"
