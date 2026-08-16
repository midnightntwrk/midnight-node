#ci #node #toolkit
# Content-addressed base Docker images

Base Docker images for node, toolkit, and hardfork-test-upgrader are now
built separately and tagged with the git tree hash of their `images/<name>/`
directory. Image targets pull pre-built bases from the registry instead of
rebuilding the Dockerfile every run, skipping redundant package installs.

A new `Build Base Images` workflow builds and pushes missing base images
(natively on amd64 and arm64 runners) whenever `images/**` changes on main.

PR: https://github.com/midnightntwrk/midnight-node/pull/950
