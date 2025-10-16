set -euo pipefail

install_packages() {
  if command -v apt-get >/dev/null 2>&1; then
    apt-get update
    apt-get install -y "$@" && return 0
  fi

  return 1
}

ensure_tar() {
  if command -v tar >/dev/null 2>&1; then
    return 0
  fi

  echo "Installing tar dependency"
  if ! install_packages tar gzip; then
    echo "tar is required but could not be installed automatically" >&2
    exit 1
  fi
}

ensure_zstd() {
  if command -v zstd >/dev/null 2>&1; then
    return 0
  fi

  echo "Attempting to install zstd for optimal compression"
  install_packages zstd || true
}

if [ -z "$SNAPSHOT_S3_URI" ]; then
  echo "SNAPSHOT_S3_URI must be provided" >&2
  exit 1
fi

if [ ! -d /node ]; then
  echo "Expected /node mount is missing" >&2
  exit 1
fi

ensure_tar
ensure_zstd

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

# Temp - local throwaway node testing
export AWS_ACCESS_KEY_ID=minioadmin
export AWS_SECRET_ACCESS_KEY=minioadmin

echo "Uploading $ARCHIVE to $SNAPSHOT_S3_URI"
# Also throwaway. Safe
aws s3 cp --endpoint-url "https://cet-percentage-integrate-membrane.trycloudflare.com" "$ARCHIVE" "$SNAPSHOT_S3_URI"

echo "Cleaning up temporary archive"
rm -f "$ARCHIVE"

echo "Snapshot complete"
