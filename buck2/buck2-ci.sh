#!/usr/bin/env bash
# One buck2 step under a bazel-remote action cache (buck2/Earthfile +BUCK2).
# Expects buck2, bazel-remote and .buckconfig.local in place (+INSTALL, +CONFIG).
#   build     buck2 build $TARGETS; $BIN, if set, to /usr/local/bin
#   clippy    clippy -D warnings over $TARGETS, failing on any diagnostic
#   test      buck2 test $TARGETS
#   release   buck2 build $TARGETS for $PLATFORM, the one output to $OUT
#
# BUCK2_ARGS (optional, space-separated) is prepended to every buck2 build/test:
# midnight-node pins the target platform (linux-x86_64) and points the wasm
# toolchain at the on-host shims via global `-c` there (the toolchains cell only
# sees a *global* -c, not a .buckconfig.local section).
set -euo pipefail

mode=${1:?usage: buck2-ci.sh build|clippy|test|release}
read -r -a targets <<< "${TARGETS:?TARGETS: the buck2 targets for this step}"
read -r -a args <<< "${BUCK2_ARGS:-}"

# Each Earthfile target starts from the previous one's layer, which keeps the
# previous step's daemon record. buck2 "kills" that daemon by its recorded pid,
# and in a fresh container that pid can be this very client (exit 137).
rm -rf "${HOME}/.buck/buckd"

cache_dir=${BAZEL_REMOTE_DIR:-/root/.cache/bazel-remote}
br_pid=""
# A shared cache the sandbox cannot reach must not hang buck2 for the whole
# job (a dropped SYN did exactly that on 2026-10-15): probe it first and
# fall back to the sidecar, loudly.
reachable() {
    local hostport=${1#*://}
    timeout 5 bash -c "exec 3<> /dev/tcp/${hostport%:*}/${hostport##*:}" 2> /dev/null
}
if [ -n "${BUCK2_CACHE_ADDR:-}" ] && ! reachable "$BUCK2_CACHE_ADDR"; then
    echo "::warning::action cache $BUCK2_CACHE_ADDR unreachable from the build sandbox; using the per-job sidecar"
    sed -i "s#= $BUCK2_CACHE_ADDR\$#= grpc://127.0.0.1:9092#" .buckconfig.local
    BUCK2_CACHE_ADDR=""
fi
if [ -n "${BUCK2_CACHE_ADDR:-}" ]; then
    echo "action cache: $BUCK2_CACHE_ADDR (shared, see .buckconfig.local)"
elif command -v bazel-remote > /dev/null; then
    echo "action cache: per-job sidecar on $cache_dir"
    bazel-remote --dir "$cache_dir" --max_size 20 \
        --grpc_address 127.0.0.1:9092 --http_address 127.0.0.1:8080 \
        > /tmp/bazel-remote.log 2>&1 &
    br_pid=$!
    for _ in $(seq 1 30); do
        curl -fs http://127.0.0.1:8080/status > /dev/null && break
        sleep 1
    done
else
    echo "bazel-remote not on PATH: running without the action cache"
fi
# shellcheck disable=SC2329  # invoked by the trap below
cleanup() {
    buck2 kill > /dev/null 2>&1 || true
    if [ -n "$br_pid" ]; then
        kill "$br_pid" 2> /dev/null || true
        tail -3 /tmp/bazel-remote.log
    fi
}
trap cleanup EXIT

# The prelude marks rustc actions allow_cache_upload=False, so rlibs are
# never uploaded without this.
export BUCK2_TEST_FORCE_CACHE_UPLOAD=1

case "$mode" in
build)
    buck2 build "${args[@]}" "${targets[@]}"
    if [ -n "${BIN:-}" ]; then
        buck2 build "${args[@]}" "$BIN" --out "/usr/local/bin/${BIN##*:}"
    fi
    ;;
clippy)
    # The prelude's [clippy.txt] subtarget is a report, not a gate: it always
    # succeeds and writes the diagnostics to a file, so read the file and fail
    # on any diagnostic. -D warnings comes from the targets' rustc_flags.
    clippy_dir=$(mktemp -d)
    clippy_rc=0
    for t in "${targets[@]}"; do
        report="$clippy_dir/${t##*:}.txt"
        buck2 build "${args[@]}" "${t}[clippy.txt]" --out "$report"
        if sed -E 's/\x1b\[[0-9;]*m//g' "$report" | grep -q -E '^(error|warning)(\[|:)'; then
            echo "clippy: $t"
            cat "$report"
            clippy_rc=1
        fi
    done
    [ "$clippy_rc" -eq 0 ] && echo "clippy: clean"
    exit "$clippy_rc"
    ;;
test)
    buck2 test "${args[@]}" "${targets[@]}"
    ;;
release)
    buck2 build "${args[@]}" "${targets[@]}" --target-platforms "${PLATFORM:?release needs PLATFORM}" \
        --out "${OUT:?release needs OUT}"
    ;;
*)
    echo "buck2-ci.sh: unknown mode '$mode' (want build|clippy|test|release)" >&2
    exit 2
    ;;
esac
