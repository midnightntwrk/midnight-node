#node #ci
# Use content hashes for Docker image tags

Replace 8-char commit hashes with 12-char tree content hashes in image tags.
Identical source trees now produce the same tag, allowing CI to skip redundant
builds when the tree hasn't changed (e.g. merge commits, cherry-picks, reverts).

A `force_rebuild` input provides an escape hatch when needed.

PR: https://github.com/midnightntwrk/midnight-node/pull/783
