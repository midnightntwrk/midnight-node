#ci #build

# Update Rust version to 1.92 and install cargo-shear directly

Updated the CI build images to use Rust 1.92 (from 1.90) and changed cargo-shear installation to use direct cargo install instead of cargo binstall. Less chance of failed builds due to binstalling too many times.

Changes include:

- Updated Rust base image from `rust:1.90-trixie` to `rust:1.92-trixie` in subxt and node-ci-image-single-platform targets
- Updated CI image tag from `1.90` to `1.92`
- Updated prep-no-copy to use the new CI image tag
- Replaced cargo binstall installation of cargo-shear with cargo install in CI image.

PR: https://github.com/midnightntwrk/midnight-node/pull/452
JIRA: https://shielded.atlassian.net/browse/PM-21169
