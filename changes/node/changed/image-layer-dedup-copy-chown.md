#node #toolkit #docker
# Stop duplicating binaries and `res/` in a second image layer

The node and toolkit images copied their files as root and then ran
`chown -R appuser:appuser …` over the same paths. `chown` rewrites every file,
so buildkit stored a second full copy of them in the `chown` layer. Measured on
the node image with a stand-in 133M binary, that layer was 210MB: the binary
plus the whole 74M `res/` tree, duplicated.

The files are now copied with `COPY --chown=appuser:appuser` (the `appuser`
account moved above the `COPY`s in both base Dockerfiles), and the remaining
`chown` only covers directories created by `RUN mkdir`. Same for the toolkit
image, where `/toolkit-js` was being duplicated as well.

Side effect: `chown -R … ./bin` never reached the file it was aimed at, since
`/bin` is a usr-merge symlink and `chown -R` does not traverse it. `.envrc` is
now owned by `appuser` like the rest.

PR: https://github.com/midnightntwrk/midnight-node/pull/2048
