#node #docker #ci

# Slim down node Docker image by ~200 MB using multi-stage build

Multi-stage Dockerfile for the node image so gcc and the build toolchain
(gcc-c++, cmake, make, git, wget, libtool, autoconf, automake) are confined
to a builder stage and excluded from the runtime image. Only the compiled
libfaketime .so is copied across. Also removes non-deterministic
`microdnf -y update` (base image is pinned by digest) and pins libfaketime
to v0.9.10 so the base layer is cacheable between builds.

PR: https://github.com/midnightntwrk/midnight-node/pull/897
