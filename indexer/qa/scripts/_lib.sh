#!/bin/bash

# Shared helpers for the undeployed-stack startup scripts.
# Source from `startup-localenv-*.sh`:
#   . "$(dirname "$0")/_lib.sh"

# Default node tag for a run that did not set NODE_TAG: the stable entry of the
# repo's NODE_VERSIONS file. This mirrors readSupportedNodeVersion() in
# qa/tests/environment/model.ts, which reads the same file and takes the second
# line — so a stack started without NODE_TAG runs the node version the test
# framework points its toolkit at by default. Falls back to the legacy
# single-version NODE_VERSION file, which older checkouts still carry.
# Prints the tag; returns non-zero with a message when neither file is usable.
resolve_node_tag() {
    if [ -f NODE_VERSIONS ]; then
        awk 'NF { v[++n] = $1 } END { if (n == 0) exit 1; print (n >= 2 ? v[2] : v[n]) }' \
            NODE_VERSIONS && return 0
        echo "ERROR: NODE_VERSIONS exists but holds no version; set NODE_TAG explicitly." >&2
        return 1
    fi

    if [ -f NODE_VERSION ]; then
        tr -d '[:space:]' < NODE_VERSION
        return 0
    fi

    echo "ERROR: neither NODE_VERSIONS nor NODE_VERSION found in $(pwd)." >&2
    echo "       Run from the repository root, or set NODE_TAG explicitly." >&2
    return 1
}

# Resolve the indexer images to run and export INDEXER_TAG (plus IMAGE_REGISTRY when
# the images were only found on GHCR). An externally supplied INDEXER_TAG is left
# alone. Otherwise the default is the newest published main build at or below this
# branch's base: every push to main is tagged <workspace-version>-<sha8> (the scheme
# docker/metadata-action applies in build-indexer-images), and those per-commit tags
# go to GHCR only — Docker Hub carries release tags alone — so GHCR is probed first.
#
# Both halves of the tag are read at the same commit. Taking the version from the
# working tree and the sha from somewhere else yields tags that were never published,
# because a release bump changes the version without rebuilding older commits.
resolve_indexer_image() {
    if [ -n "${INDEXER_TAG:-}" ]; then
        echo "Using externally defined INDEXER_TAG: $INDEXER_TAG"
        return 0
    fi

    local registries="ghcr.io/midnight-ntwrk midnightntwrk"
    if [ -n "${IMAGE_REGISTRY:-}" ]; then
        registries="$IMAGE_REGISTRY"
    fi

    # Commits are only built once they are on main, so start from this branch's base
    # there and walk back — a branch's own commits have no images.
    local base
    base=$(git merge-base HEAD origin/main 2>/dev/null) || base=$(git rev-parse HEAD)

    echo "Looking for a published indexer build at or below $(git rev-parse --short=8 "$base")..."

    local sha version candidate registry
    for sha in $(git rev-list --max-count="${INDEXER_TAG_SEARCH_DEPTH:-10}" "$base"); do
        version=$(git show "$sha:Cargo.toml" 2>/dev/null \
            | sed -n 's/^version[[:space:]]*=[[:space:]]*"\([^"]*\)".*/\1/p' | head -n 1)
        [ -z "$version" ] && continue

        candidate="${version}-${sha:0:8}"

        # `docker manifest inspect` asks the registry without downloading the image.
        for registry in $registries; do
            if docker manifest inspect "${registry}/indexer-api:${candidate}" >/dev/null 2>&1; then
                export INDEXER_TAG="$candidate"
                export IMAGE_REGISTRY="$registry"
                echo "Using indexer images ${registry}/*:${candidate}"
                return 0
            fi
        done
        echo "No published build for ${candidate}"
    done

    echo "ERROR: no published indexer build found in the last" \
         "${INDEXER_TAG_SEARCH_DEPTH:-10} commits of ${base} across: ${registries}." >&2
    echo "       Set INDEXER_TAG explicitly (with IMAGE_REGISTRY when the image is not" >&2
    echo "       on GHCR), or raise INDEXER_TAG_SEARCH_DEPTH to look further back." >&2
    return 1
}

# Derive Docker Compose project name the same way Docker Compose does
# (basename of cwd, lowercased, dots stripped, hyphens kept).
derive_docker_project_name() {
    local project_dir
    project_dir=$(basename "$(pwd)")
    echo "$project_dir" | tr '[:upper:]' '[:lower:]' | sed 's/\.//g'
}

# Tear down any prior compose stack and the associated node_data volume.
# Args: $1 = docker-compose project name (used to scope volume cleanup).
teardown_prior_stack() {
    local project_name="$1"

    echo "Tearing down any prior compose stack..."
    docker compose --profile cloud down --remove-orphans 2>&1 \
        || echo "[startup] No prior stack to remove (normal on first run; if docker daemon is down, subsequent steps will fail)."

    # Belt-and-suspenders: stop any container still holding the node_data volume.
    # `docker volume rm` has no force flag — the volume can only be removed once no
    # container references it.
    local volume_users
    volume_users=$(docker ps -a -q --filter volume="${project_name}_node_data" 2>/dev/null)
    if [ -n "$volume_users" ]; then
        echo "Removing containers still holding node_data volume..."
        docker rm -f $volume_users
    fi

    if docker volume ls | grep -q "${project_name}_node_data"; then
        local volumes
        volumes=$(docker volume ls | grep "${project_name}_node_data" | awk -F " " '{print $2}')
        for volume in $volumes; do
            docker volume rm $volume
        done
        echo "Named volumes removed."
    else
        echo "No named volumes to remove."
    fi
}

# Poll the indexer /ready endpoint until it responds or the budget is spent.
# The loop early-exits the moment /ready answers, so warm starts still return
# in a couple of seconds; the budget only bounds genuinely slow cold starts
# (image pulls plus initial chain catch-up), which exceeded the old 20s cap.
# Override the budget with INDEXER_READY_TIMEOUT_SECONDS. Exits non-zero on
# timeout after dumping container state and indexer-api logs.
wait_for_indexer_ready() {
    local budget="${INDEXER_READY_TIMEOUT_SECONDS:-120}"
    local interval=2
    # Ceiling division so elapsed time always covers the budget and there is at
    # least one attempt even for small overrides (e.g. budget=1 → 1 attempt,
    # not 0; budget=5 → 3 attempts → 6s rather than an undercounted 4s).
    local attempts=$(( (budget + interval - 1) / interval ))
    echo "Waiting for indexer API to become ready (${budget}s budget)..."
    local ready=0 i
    for (( i=1; i<=attempts; i++ )); do
        if curl -sf http://localhost:8088/ready >/dev/null; then
            echo "Indexer API is ready"
            ready=1
            break
        fi
        echo "Not ready yet... ($i/$attempts)"
        sleep "$interval"
    done
    if [ "$ready" -ne 1 ]; then
        echo "ERROR: Indexer API did not become ready within ${budget}s. Dumping container state:"
        docker compose --profile cloud ps
        echo "Last 50 lines of indexer-api logs:"
        docker compose --profile cloud logs --tail=50 indexer-api 2>&1 || true
        exit 1
    fi
}

# Clear the toolkit fetch cache and the block-scanner's per-env scan cursor + block cache.
# Stale entries would otherwise cause generate:data to skip the current chain's blocks
# and write outdated hashes into the test data files.
clear_block_scanner_cache() {
    echo "Deleting toolkit cache..."
    rm -rf qa/tests/.tmp/toolkit/.sync_cache-undeployed/

    echo "Clearing block-scanner cache for undeployed..."
    rm -f qa/tools/block-scanner/tmp_scan/undeployed_*.jsonl
    rm -f qa/tools/block-scanner/stats/undeployed_*.json
}

# Ensure the block-scanner's dependencies are installed before `generate:data`.
# On a fresh checkout/worktree there is no node_modules, so the scan would fail
# with an opaque module-resolution error (e.g. "ENOENT while resolving package
# 'esprima'"). Install on demand so a first run on a clean worktree works.
ensure_block_scanner_deps() {
    if [ ! -d qa/tools/block-scanner/node_modules ]; then
        echo "Installing block-scanner dependencies (first run on this checkout)..."
        # The startup scripts don't set errexit, so guard explicitly: otherwise
        # a failed install falls through to `generate:data` and surfaces the
        # same opaque module-resolution error this helper exists to prevent.
        if ! (cd qa/tools/block-scanner && bun install); then
            echo "ERROR: bun install failed in qa/tools/block-scanner" >&2
            return 1
        fi
    fi
}
