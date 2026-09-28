#toolkit #docker #security
# Bump AL2023 base image to clear critical unbound CVE

Bump the `amazonlinux:2023-minimal` base digest in the node, toolkit and
hardfork-test-upgrader images from the April 2026 snapshot to 2023.12.20260918,
which ships unbound 1.17.1-1.amzn2023.0.14 (ALAS2023-2026-3101, critical) plus
fixed openssl, glibc, libnghttp2, glib2, libsolv and others. The explicit
`curl-minimal`, `vim-enhanced` and `jq` pins are bumped to the versions in that
snapshot, since the old pins downgraded curl and held vim/jq on vulnerable builds.

PR: https://github.com/midnightntwrk/midnight-node/pull/2207
