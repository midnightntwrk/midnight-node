#!/usr/bin/env bash
# Run a command with transient credentials for private GitHub cargo dependencies.
#
# Cargo resolves git dependencies by shelling out to git (.cargo/config.toml sets
# net.git-fetch-with-cli), and git reads ~/.netrc for https auth. The netrc is written here
# and removed on exit, so the token never survives into an image layer or the remote build
# cache. That is why every cargo step wraps itself in this script rather than a single
# shared RUN in the base image: Earthly exposes a secret only to the RUN that declares it,
# and a token written into a shared layer would be pushed to the ghcr remote cache.
#
# NODE_GIT_TOKEN comes from `RUN --secret NODE_GIT_TOKEN=`. Empty is the normal case -- the
# workspace has no private dependencies by default, and this script is then a passthrough.
set -euo pipefail

netrc="${HOME:-/root}/.netrc"

if [ -n "${NODE_GIT_TOKEN:-}" ]; then
    if [ -e "$netrc" ]; then
        # Someone (a developer running this by hand) already has credentials here. Use them
        # and leave the file alone -- we must never delete a netrc we did not create.
        echo "with-git-creds: $netrc already exists; using it, ignoring NODE_GIT_TOKEN" >&2
    else
        # noclobber makes the create atomic, so we only ever arm the trap on a file we own.
        (
            umask 077
            set -o noclobber
            printf 'machine github.com\n  login x-access-token\n  password %s\n' \
                "$NODE_GIT_TOKEN" >"$netrc"
        )
        trap 'rm -f "$netrc"' EXIT
    fi
fi

# Belt and braces: foreign images (srtool) don't necessarily pick up .cargo/config.toml,
# and libgit2 ignores .netrc.
export CARGO_NET_GIT_FETCH_WITH_CLI=true

# No exec -- the EXIT trap must still fire to remove the netrc.
"$@"
