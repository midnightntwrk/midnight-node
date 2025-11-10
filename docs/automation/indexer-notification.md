# Indexer Notification on Release

**Jira**: [TPM-774](https://shielded.atlassian.net/browse/TPM-774)

## Overview

Automatically notifies midnight-indexer when node release published (non-draft, non-prerelease). Sends `repository_dispatch` to indexer repo → creates integration PR.

**Workflow**: [`.github/workflows/notify-indexer-on-release.yml`](../../.github/workflows/notify-indexer-on-release.yml)

## Usage

Include `metadata.scale` in release:

```bash
# Create release with metadata
gh release create v0.19.0 metadata.scale \
  --repo midnightntwrk/midnight-node \
  --title "Midnight Node 0.19.0"

# Or add to existing release
gh release upload v0.19.0 metadata.scale
```

Check result: https://github.com/midnightntwrk/midnight-indexer/pulls

## Troubleshooting

**No indexer PR?** Check [workflow runs](https://github.com/midnightntwrk/midnight-node/actions/workflows/notify-indexer-on-release.yml), verify `INDEXER_DISPATCH_TOKEN` secret configured.

Contact: @cosmir17, Giles, Oscar
