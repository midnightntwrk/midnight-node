#!/usr/bin/env bash
# Build @midnight-ntwrk/{platform-js,compact-js,compact-js-node,compact-js-command}
# from the pinned `midnight-sdk/` submodule into `.midnight-sdk-js/*.tgz`, where
# util/toolkit-js/compact-0.31 consumes them as `file:` tarball dependencies
# instead of npm registry tarballs. The submodule commit is the version of
# truth (the package.json versions inside midnight-sdk lag its npm publishes,
# which bump at release time).
#
# Tarballs, not `file:` directories: npm installs directory deps as symlinks
# and node resolves from the symlink's realpath, which lives outside the
# toolkit project — the package's own deps would be unreachable. Tarballs are
# extracted into node_modules like registry packages. Sibling deps inside the
# tarballs are rewritten to the exact versions built here so npm dedupes onto
# the local copies rather than fetching same-numbered (differently built)
# tarballs from the registry. `npm pack` emits a reproducible tar stream
# (fixed mtimes, sorted entries) but its gzip envelope varies across npm
# versions, so the gzip layer is re-wrapped with node zlib at level 0 (stored
# blocks — no compression heuristics to drift) to keep the package-lock.json
# integrity hashes stable across machines and rebuilds.
#
# The submodule tree is copied to a scratch dir and built there, so the
# checkout itself stays pristine. Three adjustments are made to the copy:
#   1. Its .yarnrc.yml points @midnight-ntwrk at npm.pkg.github.com with
#      npmAlwaysAuth, which needs credentials — override back to public npm.
#   2. Its yarn.lock bakes npm.pkg.github.com archive URLs into resolutions
#      (`::__archiveUrl=...`) — strip them so the registry override applies.
#      Tarball checksums can differ between the two registries for the same
#      version, so checksums are refreshed; exact versions stay lockfile-pinned.
#   3. compact-js declares @midnight-ntwrk/platform-js as a registry range —
#      resolve it to the sibling platform-js just built from the same commit.
#
# Usage: build-midnight-sdk-js.sh [SDK_SRC] [OUT]
#   SDK_SRC  midnight-sdk checkout (default: <repo>/midnight-sdk)
#   OUT      output dir            (default: <repo>/.midnight-sdk-js)
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
repo_root="$(dirname "$script_dir")"
sdk_src="${1:-$repo_root/midnight-sdk}"
out="${2:-$repo_root/.midnight-sdk-js}"

if [ ! -f "$sdk_src/compact-js/package.json" ]; then
    echo "error: no midnight-sdk sources at $sdk_src" >&2
    echo "  expected: $sdk_src/compact-js/package.json" >&2
    echo "  fix:      git submodule update --init midnight-sdk" >&2
    exit 1
fi

build="$out/.build"
rm -rf "$build"
mkdir -p "$build"
for ws in platform-js compact-js; do
    (cd "$sdk_src" && tar cf - --exclude .git "$ws") | tar xf - -C "$build"
done

# Invoke each workspace's own committed yarn release directly (no corepack).
yarn() {
    local dir="$1"; shift
    (cd "$dir" && TURBO_TELEMETRY_DISABLED=1 \
        YARN_ENABLE_IMMUTABLE_INSTALLS=false \
        YARN_CHECKSUM_BEHAVIOR=update \
        node .yarn/releases/yarn-*.cjs "$@")
}

use_public_npm() {
    yarn "$1" config set npmScopes.midnight-ntwrk.npmRegistryServer https://registry.npmjs.org >/dev/null
    yarn "$1" config set --json npmScopes.midnight-ntwrk.npmAlwaysAuth false >/dev/null
}

echo "==> platform-js"
use_public_npm "$build/platform-js"
yarn "$build/platform-js" install
yarn "$build/platform-js" build

echo "==> compact-js"
use_public_npm "$build/compact-js"
sed -i.bak -E 's/::__archiveUrl=[^"]*//g' "$build/compact-js/yarn.lock" && rm "$build/compact-js/yarn.lock.bak"
node -e '
    const fs = require("fs"), f = process.argv[1], p = JSON.parse(fs.readFileSync(f));
    p.resolutions = { ...p.resolutions,
        "@midnight-ntwrk/platform-js": "portal:../platform-js/platform-js/dist" };
    fs.writeFileSync(f, JSON.stringify(p, null, 2) + "\n");
' "$build/compact-js/package.json"
# The compact-js build compiles its test contracts unless managed/ already
# exists; pre-create it to skip the compactc download (tests are not run here).
mkdir -p "$build/compact-js/compact-js/test/contract/managed"
yarn "$build/compact-js" install
yarn "$build/compact-js" build

# Pack the built packages, with sibling deps pinned to the exact versions
# built here so npm dedupes all four onto this set (no registry fallback).
dists=(platform-js/platform-js compact-js/compact-js compact-js/compact-js-node compact-js/compact-js-command)
node -e '
    const fs = require("fs"), path = require("path"), build = process.argv[1];
    const dists = process.argv.slice(2);
    const version = {};
    for (const d of dists) {
        const p = JSON.parse(fs.readFileSync(path.join(build, d, "dist/package.json")));
        version[p.name] = p.version;
    }
    for (const d of dists) {
        const f = path.join(build, d, "dist/package.json");
        const p = JSON.parse(fs.readFileSync(f));
        for (const dep of Object.keys(p.dependencies ?? {}))
            if (version[dep]) p.dependencies[dep] = version[dep];
        fs.writeFileSync(f, JSON.stringify(p, null, 2) + "\n");
    }
' "$build" "${dists[@]}"

echo "==> packing into $out"
for pkg in "${dists[@]}"; do
    name="$(basename "$pkg")"
    rm -f "$out/$name.tgz"
    (cd "$build/$pkg/dist" && npm pack --ignore-scripts --pack-destination "$out" >/dev/null)
    # One stable filename per package, whatever version the tree self-reports.
    mv "$out"/midnight-ntwrk-"$name"-*.tgz "$out/$name.tgz"
    # Normalize the gzip envelope (see header comment). The header OS byte
    # (offset 9) is platform-dependent in zlib — pin it to 0x03 (Unix).
    node -e '
        const fs = require("fs"), zlib = require("zlib"), f = process.argv[1];
        const gz = zlib.gzipSync(zlib.gunzipSync(fs.readFileSync(f)), { level: 0 });
        gz[9] = 0x03;
        fs.writeFileSync(f, gz);
    ' "$out/$name.tgz"
done
rm -rf "$build"

commit="$(git -C "$sdk_src" rev-parse HEAD 2>/dev/null || echo unknown)"
echo "==> built from midnight-sdk@$commit"
for name in platform-js compact-js compact-js-node compact-js-command; do
    node -e '
        const {execSync} = require("child_process");
        const j = JSON.parse(execSync(`tar xzf ${process.argv[1]} --to-stdout package/package.json`));
        console.log(`    ${j.name} ${j.version}`);
    ' "$out/$name.tgz"
done
