# Add node upgrade option to fork-network workflow

Extend the `fork-network` GitHub Actions workflow with a `new_node_image`
input and delegate the upgrade flow (snapshot restore, mock-authorities
convert, override generation, compose up, and rolling image swap) to
`npm run image-upgrade:${NETWORK}` in `local-environment`. This lets a
forked network be brought up on one node image and then rolled to a
second image in the same run, so upgrade behavior can be exercised
against real forked state.

PR: https://github.com/midnightntwrk/midnight-node/pull/1469
Issue: https://github.com/midnightntwrk/midnight-node/issues/1468