#!/usr/bin/env python3

# This file is part of midnight-node.
# Copyright (C) Midnight Foundation
# SPDX-License-Identifier: Apache-2.0
# Licensed under the Apache License, Version 2.0 (the "License");
# You may not use this file except in compliance with the License.
# You may obtain a copy of the License at
# http://www.apache.org/licenses/LICENSE-2.0
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

"""Generate BUCK files for workspace members from cargo metadata.

Usage: gen-first-party.py <meta.json> <lint-meta.json> <test-meta.json> [member-name ...]
Without member names, generates for every workspace member. The extra metadata
come from regen.sh: lint-meta has the lint feature set on (where it differs from
meta, a target selects on root//platforms:lint); test-meta has the features
that integration tests name in `required-features`.

Maps path deps to root//<dir>:<name> and external deps to root//third-party:<key>
(key = the [dependencies] key in third-party/Cargo.toml, i.e. the cargo rename
if present). Renamed deps land in named_deps so the extern crate name matches.
Features come from the workspace-resolved feature set (cargo's unification).
Build scripts are NOT run — members with build.rs get a placeholder comment.
"""
import json, os, re, sys, tomllib
from collections import defaultdict

META, LINT_META, TEST_META = sys.argv[1:4]
only = set(sys.argv[4:])
LINT = "root//platforms:lint"

meta = json.load(open(META))
ws_root = meta["workspace_root"]

# ── workspace lints → rustc flags ────────────────────────────────────────────
# buck2 has no notion of Cargo's [lints]; translate [workspace.lints.<tool>] into
# the -A/-W/-D/-F flags cargo would pass, in cargo's priority order (ascending, so
# a higher-priority entry overrides a lower one; ties keep source order). Crates
# opt in with `[lints] workspace = true`, exactly as under cargo, and then every
# target of that crate carries the flags - so buck2 clippy honours the repo's lint
# policy (e.g. `clippy::all = allow`) instead of clippy's stricter defaults. The
# root Cargo.toml stays the single source of truth. (rustc tolerates `clippy::`
# flags, so the shared non-clippy build is unaffected.)
with open(os.path.join(ws_root, "Cargo.toml"), "rb") as _f:
    _ws_lints = tomllib.load(_f).get("workspace", {}).get("lints", {})


def _lint_flags():
    lvl = {"allow": "-A", "warn": "-W", "deny": "-D", "forbid": "-F"}
    rows, order = [], 0
    for tool, prefix in (("rust", ""), ("clippy", "clippy::")):
        for name, spec in _ws_lints.get(tool, {}).items():
            level = spec["level"] if isinstance(spec, dict) else spec
            prio = spec.get("priority", 0) if isinstance(spec, dict) else 0
            rows.append((prio, order, lvl[level] + prefix + name.replace("-", "_")))
            order += 1
    rows.sort(key=lambda r: (r[0], r[1]))
    return [f for _, _, f in rows]


LINT_FLAGS = _lint_flags()
_lint_optin = {}


def opts_into_workspace_lints(pkg):
    mp = pkg["manifest_path"]
    if mp not in _lint_optin:
        try:
            with open(mp, "rb") as f:
                _lint_optin[mp] = (
                    tomllib.load(f).get("lints", {}).get("workspace") is True
                )
        except Exception:
            _lint_optin[mp] = False
    return _lint_optin[mp]


# (name, req, source) -> third-party manifest key, written by gen-third-party.py
keymap = json.load(open(os.path.join(ws_root, "third-party", "keys.json")))
members = set(meta["workspace_members"])
pkg_by_id = {p["id"]: p for p in meta["packages"]}
resolve_by_id = {n["id"]: n for n in meta["resolve"]["nodes"]}


def resolve_nodes(path):
    return {n["id"]: n for n in json.load(open(path))["resolve"]["nodes"]}


lint_resolve_by_id = resolve_nodes(LINT_META)
test_resolve_by_id = resolve_nodes(TEST_META)

# WASM variant data. `first-party-wasm.json` (from gen-wasm-deps.py)
# maps each first-party crate in the runtime's no_std closure to its wasm feature
# set; `wasm_names` is the set of wasm-deps buck target names. Together they let
# each closure crate's lib emit `select()` on `cpu:wasm32` that swaps the std
# feature set for the no_std one and repoints `root//third-party:X` deps to
# `root//wasm-deps:X` (dropping std-only deps like frame-benchmarking that have
# no wasm-deps target). First-party deps are left as-is — buck's platform
# transition rebuilds them for wasm32, and they carry the same `select()`.
_wasm_sidecar = os.path.join(ws_root, "wasm-deps", "first-party-wasm.json")
wasm_features_map = (
    json.load(open(_wasm_sidecar)) if os.path.exists(_wasm_sidecar) else {}
)
wasm_names = set()
_wasm_buck = os.path.join(ws_root, "wasm-deps", "BUCK")
if os.path.exists(_wasm_buck):
    wasm_names = set(
        re.findall(r'^    name = "([^"]+)"', open(_wasm_buck).read(), re.M)
    )


def to_wasm_dep(label, wasm_dep_names, label_pkg):
    """Map a std dep label to its wasm variant, or None to drop it. A dep is kept
    on wasm32 only if its crate name is in the consumer's wasm dep set (so std-
    only deps like ledger's `lazy_static` or node-side `time-source` are dropped
    even when a wasm-deps target exists for them). Third-party deps are repointed
    `third-party:X` -> `wasm-deps:X`; first-party deps are kept (they transition
    to wasm32 and carry the same select)."""
    if label_pkg.get(label) not in wasm_dep_names:
        return None
    if label.startswith("root//third-party:"):
        key = label.split(":", 1)[1]
        return f"root//wasm-deps:{key}" if key in wasm_names else None
    return label


def underscore(s):
    return s.replace("-", "_")


def _format_ledger_version(version, source):
    """Mirror of ledger/helpers/build.rs::format_version — the build script
    only emits three rustc-env vars, so we compute them statically instead of
    running it under buck."""
    if not source or not source.startswith("git+"):
        return version
    git = source[len("git+") :]
    locator, _, commit = git.partition("#")
    commit = commit or "unknown"
    label = ""
    if "?" in locator:
        query = locator.split("?", 1)[1]
        for p in query.split("&"):
            if p.startswith("tag=") or p.startswith("branch="):
                label = p.replace("=", ": ", 1) + ", "
                break
    return f"{version} ({label}rev: {commit})"


def _ledger_helpers_env(pkg, node):
    aliases = [
        ("mn_ledger", "LEDGER_7_VERSION"),
        ("mn_ledger_8", "LEDGER_8_VERSION"),
        ("mn_ledger_9", "LEDGER_9_VERSION"),
    ]
    env = {}
    for alias, var in aliases:
        rd = next((d for d in node["deps"] if d["name"] == alias), None)
        if rd is None:
            continue
        dp = pkg_by_id[rd["pkg"]]
        env[var] = _format_ledger_version(dp["version"], dp.get("source"))
    return env


# package name -> fn(pkg, node) producing extra compile-time env that a build.rs
# would otherwise set. Lets us skip running the script under buck.
ENV_HOOKS = {
    "midnight-node-ledger-helpers": _ledger_helpers_env,
    "midnight-primitives-mainchain-follower": lambda p, n: {
        # sqlx-macros checks queries at compile time against the .sqlx offline
        # cache, which it looks for at <manifest_dir>/.sqlx and then, by running
        # `cargo metadata`, at the workspace root. That fails in buck2's sources
        # tree, so MAPPED_SRCS_EXTERNAL puts the workspace's cache at the first.
        # (sqlx 0.8 reads SQLX_OFFLINE_DIR from a .env file only, never the
        # environment.) CARGO only satisfies sqlx's presence check.
        "CARGO": "cargo",
        "SQLX_OFFLINE": "true",
    },
    # runtime lib.rs include!s $OUT_DIR/wasm_binary.rs; point at the stub dir
    # (WASM_BINARY=None) since substrate-wasm-builder can't run under buck.
    "midnight-node-runtime": lambda p, n: {
        # Real on-chain blob: OUT_DIR provides a wasm_binary.rs that
        # include_bytes!'s the wasm-opt'd + zstd-compressed runtime. The runtime
        # lib is std here, but the blob is the wasm cdylib built via the wasm32
        # configured_alias, so no build cycle.
        "OUT_DIR": "$(location root//:runtime-wasm-binary-rs)",
    },
    # node/build.rs (generate_cargo_keys) emits these from git; provide static
    # values so the CLI --version works without running git under buck.
    "midnight-node": lambda p, n: {
        "SUBSTRATE_CLI_IMPL_VERSION": f"{p['version']}-buck",
        "SUBSTRATE_CLI_COMMIT_HASH": "",
    },
}

# Crates whose sources include_str!/include_bytes! files ABOVE their own crate
# dir (e.g. ../../../COMPACTC_VERSION). rustc resolves include! relative to the
# source file, but buck sandboxes srcs rooted at the target's package, so those
# paths escape. Fix (as reindeer does for git crates): use mapped_srcs to build
# a srcs tree that MIRRORS the repo — the crate's own files at <rel>/… and the
# external files at their repo-relative paths — with crate_root/CARGO_MANIFEST_DIR
# prefixed by <rel>. The external files live in other buck packages, so they're
# pulled via export_file labels (see EXPORTS). Value: {label: dest-in-tree}.

# buck2 runs rust tests in place at the project root, so a test reads res/,
# docs/, Cargo.toml etc. from the real tree (midnight_node_res::
# locate_workspace_root). A `resources` entry does not place a file anywhere:
# it declares the file an input, so the cached test action re-runs when the
# fixture changes. A filegroup tracks a whole res/ subdir in one entry.
TEST_RESOURCES = {
    # res tests: locate_workspace_root() walks up from CWD (= project root) for
    # res/cfg/default.toml, then read cfg presets + chainspec/openrpc/genesis blobs.
    # Globbed relative to the res package (same-package), tracked as inputs.
    "midnight-node-res": 'glob(["cfg/**", "**/*.json", "**/*.mn", "**/*.hbs"])',
    # node: only the LIB unit-test reads fixtures (see
    # TEST_TARGET_RESOURCES["midnight-node-unit-test"]); the integration tests read
    # none, so no package-wide entry.
    # docs_tests reads res/cfg, the runtime/node/metadata manifests, README, toolkit
    # source and docs/*.md - all from the real in-place tree via its root() walk. Listed
    # here only to declare them as inputs (so a version bump re-runs the drift test).
    "docs": "[\n"
    '        "root//res:cfg-default",\n'
    '        "root//runtime:lib-rs",\n'
    '        "root//runtime:runtime-cargo-toml",\n'
    '        "root//node:node-cargo-toml",\n'
    '        "root//metadata:metadata-cargo-toml",\n'
    '        "root//:readme",\n'
    '        "root//util/toolkit:fetcher-runtimes",\n'
    '    ] + glob(["*.md"])',
}

# Per-TEST-TARGET resources (keyed by rust_test name), for packages whose test
# targets need different runtime files. Takes precedence over TEST_RESOURCES.
# res/ input set shared by toolkit's unit-test and cli_tests (declared inputs only; both
# read the real in-place tree - unit-test via test_paths::res()'s walk, cli_tests after
# chdir to CARGO_MANIFEST_DIR then ../../res). Filegroups track whole subdirs; the two
# genesis blobs by name (res/genesis holds 23, only these two are read).
_TOOLKIT_RES = (
    '        "root//res:fg-cfg",\n'
    '        "root//res:fg-dev",\n'
    '        "root//res:fg-test-contract",\n'
    '        "root//res:fg-test-tx-deserialize",\n'
    '        "root//res:genesis-block-undeployed",\n'
    '        "root//res:genesis-state-undeployed",\n'
)

TEST_TARGET_RESOURCES = {
    # node unit-test reads res/cfg/*.toml, docs/openrpc.json and res/mock-bridge-data/
    # from the in-place tree; listed as inputs for cache-correctness.
    "midnight-node-unit-test": "[\n"
    '        "root//docs:openrpc-json",\n'
    '        "root//res:fg-cfg",\n'
    '        "root//res:fg-mock-bridge-data",\n'
    "    ]",
    # cached_context reads test-data/genesis/genesis_block_undeployed.mn at runtime
    # (via CARGO_MANIFEST_DIR, resolved at runtime after the env!-> std::env::var fix).
    "midnight-node-toolkit-cached_context": 'glob(["test-data/genesis/**"])',
    # toolkit unit tests read test-data/** (same-package, via manifest_dir()) and a broad
    # res/ set (via test_paths::res()'s walk to the real tree).
    "midnight-node-toolkit-unit-test": "[\n"
    + _TOOLKIT_RES
    + '    ] + glob(["test-data/**"])',
    # cli_tests (trycmd) chdirs to CARGO_MANIFEST_DIR (util/toolkit) and runs the README +
    # tests/cmd/*.toml cases, which exec the toolkit bin and read ../../res/... from the
    # real tree. The corpus is same-package; the res block is the shared input set.
    "midnight-node-toolkit-cli_tests": "[\n" + _TOOLKIT_RES + '        "README.md",\n'
    '    ] + glob(["tests/cmd/*.toml"])',
}

# Data-file globs a prefixed package needs in addition to src/** + Cargo.toml
# (the `else` branch gets these via SRCS_HOOKS; the prefix branch needs them here
# so runtime fixture reads resolve at the project-relative layout).
PREFIX_DATA_GLOBS = {
    # What res's tests read: the cfg presets (toml) and the chainspec and
    # genesis blobs.
    "midnight-node-res": ["**/*.mn", "**/*.json", "**/*.hbs", "**/*.toml"],
}

MAPPED_SRCS_EXTERNAL = {
    # res owns every file its tests read, so no cross-package labels — membership
    # here (even empty) just switches it to the project-relative `res/` prefix.
    "midnight-node-res": {},
    # node/src/warp_ledger_sync/integration_tests.rs include_bytes!s
    # `../../../res/genesis/genesis_state_preview.mn` - a file above the crate
    # dir, so node switches to the repo-mirroring prefix (src -> node/src, the
    # genesis blob at res/genesis/...) and the `../../../res/...` path resolves.
    "midnight-node": {
        "root//res:genesis-state-preview": "res/genesis/genesis_state_preview.mn",
    },
    "midnight-primitives-mainchain-follower": {
        "root//:sqlx-cache": "primitives/mainchain-follower/.sqlx",
    },
    # ledger/src/host_api/ledger_9.rs include_bytes!s the undeployed genesis
    # state under `runtime-benchmarks`, which the lint platform turns on.
    "midnight-node-ledger": {
        "root//res:genesis-state-undeployed": "res/genesis/genesis_state_undeployed.mn",
    },
    "midnight-node-toolkit": {
        "root//:COMPACTC_VERSION": "COMPACTC_VERSION",
        "root//node:node-cargo-toml": "node/Cargo.toml",
        "root//res:test-contract-addr": "res/test-contract/contract_address_undeployed.mn",
        # cli_tests (trycmd) fixtures the README examples + tomls read via
        # `../../res/…` (CWD = crate dir). Mapped so they land at __srcs/res/….
        "root//res:test-contract-tx1": "res/test-contract/contract_tx_1_deploy_undeployed.mn",
        "root//res:genesis-block-undeployed": "res/genesis/genesis_block_undeployed.mn",
        "root//res:serialized-tx": "res/test-tx-deserialize/serialized_tx.mn",
        # toolkit_e2e include_str!s it.
        "root//res:undeployed-genesis-seeds": "res/dev/undeployed-genesis-seeds.json",
    },
}

# Files a package must export_file so a mapped-srcs crate can reference them
# across package boundaries. member name -> list of export target texts.
EXPORTS = {
    "midnight-node": [
        'export_file(name = "node-cargo-toml", src = "Cargo.toml", visibility = ["PUBLIC"])',
    ],
    "docs": [
        'export_file(name = "openrpc-json", src = "openrpc.json", visibility = ["PUBLIC"])',
    ],
    "midnight-node-runtime": [
        'export_file(name = "lib-rs", src = "src/lib.rs", visibility = ["PUBLIC"])',
        'export_file(name = "runtime-cargo-toml", src = "Cargo.toml", visibility = ["PUBLIC"])',
    ],
    "midnight-node-metadata": [
        'export_file(name = "metadata-cargo-toml", src = "Cargo.toml", visibility = ["PUBLIC"])',
    ],
    "midnight-node-res": [
        # Filegroups over whole subdirs, listed as test `resources` for input-tracking.
        'filegroup(name = "fg-cfg", srcs = glob(["cfg/*.toml"]), visibility = ["PUBLIC"])',
        'filegroup(name = "fg-dev", srcs = glob(["dev/*.json"]), visibility = ["PUBLIC"])',
        'filegroup(name = "fg-test-contract", srcs = glob(["test-contract/*.mn"]), visibility = ["PUBLIC"])',
        'filegroup(name = "fg-test-tx-deserialize", srcs = glob(["test-tx-deserialize/*.mn"]), visibility = ["PUBLIC"])',
        'filegroup(name = "fg-mock-bridge-data", srcs = glob(["mock-bridge-data/*.json"]), visibility = ["PUBLIC"])',
        'export_file(name = "cfg-default", src = "cfg/default.toml", visibility = ["PUBLIC"])',
        'export_file(\n    name = "undeployed-genesis-seeds",\n'
        '    src = "dev/undeployed-genesis-seeds.json",\n'
        '    visibility = ["PUBLIC"],\n)',
        'export_file(\n    name = "genesis-state-undeployed",\n'
        '    src = "genesis/genesis_state_undeployed.mn",\n'
        '    visibility = ["PUBLIC"],\n)',
        'export_file(\n    name = "test-contract-addr",\n'
        '    src = "test-contract/contract_address_undeployed.mn",\n'
        '    visibility = ["PUBLIC"],\n)',
        'export_file(\n    name = "test-contract-tx1",\n'
        '    src = "test-contract/contract_tx_1_deploy_undeployed.mn",\n'
        '    visibility = ["PUBLIC"],\n)',
        'export_file(\n    name = "genesis-block-undeployed",\n'
        '    src = "genesis/genesis_block_undeployed.mn",\n'
        '    visibility = ["PUBLIC"],\n)',
        'export_file(\n    name = "genesis-state-preview",\n'
        '    src = "genesis/genesis_state_preview.mn",\n'
        '    visibility = ["PUBLIC"],\n)',
        'export_file(\n    name = "serialized-tx",\n'
        '    src = "test-tx-deserialize/serialized_tx.mn",\n'
        '    visibility = ["PUBLIC"],\n)',
    ],
    # cli_tests (trycmd) runs `$ midnight-node-toolkit …` and diffs clap's usage
    # strings, which echo argv[0]'s basename. Buck's bin artifact is
    # `midnight_node_toolkit` (underscore); cargo names it `midnight-node-toolkit`
    # (hyphen). Provide a hyphen-named copy so the usage output matches the README.
    "midnight-node-toolkit": [
        'export_file(name = "fetcher-runtimes", src = "src/fetcher/runtimes.rs", visibility = ["PUBLIC"])',
        'genrule(\n    name = "toolkit-hyphen-bin",\n'
        '    out = "midnight-node-toolkit",\n'
        '    cmd = "cp $(location :midnight-node-toolkit-bin) $OUT",\n'
        '    visibility = ["PUBLIC"],\n)',
    ],
}

# Per-(package, test-name) env overrides for integration tests. cli_tests (trycmd)
# needs CARGO_BIN_EXE_<name> to locate the binary (cargo sets it; buck doesn't) —
# point it at the hyphen-named copy so clap's usage strings match the README.


class Starlark(str):
    """An env value emitted as a Starlark expression, not a string literal."""


# Where a test finds the toolkit binary. Clippy only needs the variable to
# exist, so the lint platform gets a name, not a dependency on building it.
TOOLKIT_BIN = Starlark(
    f'select({{"{LINT}": "midnight-node-toolkit", '
    '"DEFAULT": "$(location :toolkit-hyphen-bin)"})'
)

TEST_ENV_HOOKS = {
    # env!("CARGO_BIN_EXE_...") at compile time.
    ("midnight-node-toolkit", "indexer_e2e"): {
        "CARGO_BIN_EXE_midnight-node-toolkit": TOOLKIT_BIN,
    },
    ("midnight-node-toolkit", "cli_tests"): {
        "CARGO_BIN_EXE_midnight-node-toolkit": TOOLKIT_BIN,
        # README show-wallet examples build ledger state -> zk-params cache dir.
        "MIDNIGHT_PP": ".zk-params",
    },
    # cached_context builds a LedgerContext, whose MidnightDataProvider needs a
    # zk-params cache dir ($MIDNIGHT_PP / $XDG_CACHE_HOME / $HOME) or it panics
    # "Could not determine ...". Point it at a per-sandbox relative dir; the
    # provider create_dir_all's it and fetches params on demand from S3.
    ("midnight-node-toolkit", "cached_context"): {
        "MIDNIGHT_PP": ".zk-params",
    },
    # docs_tests reads the real repo tree in-place via root()'s upward walk; no env.
}

# Env for a package's LIB unit-test (the tests/*.rs integration targets use
# TEST_ENV_HOOKS instead). Same zk-params reason as above.
UNIT_TEST_ENV = {
    "midnight-node-ledger-helpers": {"MIDNIGHT_PP": ".zk-params"},
    "midnight-node-toolkit": {"MIDNIGHT_PP": ".zk-params"},
}

# Crates with no unit-test target. Their tests all need substrate-test-runtime's
# wasm binary, a polkadot-sdk test fixture built by substrate-wasm-builder, which
# buck2 cannot run: it would take a second wasm universe for a third-party test
# runtime. cargo still runs them.
NO_UNIT_TEST = {"sc-partner-chains-consensus-aura", "sc-partner-chains-consensus"}

# Tests that need external infra (Docker/testcontainers, a live devnet, a Cardano
# node) get a `ci-infra` label, and CI runs `buck2 test root//... --exclude
# ci-infra`. The cargo jobs that provide the infra still run them.
CI_INFRA_UNIT = {
    "partner-chains-db-sync-data-sources",  # testcontainers → postgres
    "midnight-primitives-mainchain-follower",  # #[sqlx::test] → postgres (DATABASE_URL)
}
CI_INFRA_INTEGRATION = {
    ("partner-chains-cardano-offchain", "integration_tests"),  # Cardano node
    ("midnight-node-e2e", "e2e_tests"),  # devnet
    ("midnight-node-toolkit", "single_tx"),  # docker-compose
    ("midnight-node-toolkit", "toolkit_e2e"),  # devnet / ledger static dir
    ("midnight-node-toolkit", "hardfork_e2e"),  # docker-compose
    ("midnight-node-toolkit", "indexer_show_wallet_e2e"),  # live indexer
    ("midnight-node-toolkit", "indexer_e2e"),  # live indexer
    ("sc-partner-chains-consensus", "verifier_network"),  # substrate-test-runtime wasm
}

# Crates that additionally get a `<name>-wasm-cdylib` shared-library target — the
# on-chain wasm blob source. Only the runtime; a configured_alias in
# the root BUCK builds it for wasm32, then wasm-opt + zstd produce the blob.
WASM_CDYLIB = {"midnight-node-runtime"}

# Extra src globs a crate's test targets need (test-data include_str!'d only by
# #[cfg(test)] code, sqlx::migrate! dirs, ...): unit and integration tests, not
# the lib or bins.
TEST_SRCS_HOOKS = {
    "midnight-node-ledger": ["test-data/**"],
    "partner-chains-db-sync-data-sources": ["testdata/**"],
    # README.md is the trycmd corpus for cli_tests.
    "midnight-node-toolkit": ["test-data/**", "README.md"],
}

# each value is a list of starlark expressions added to `srcs` with `+`; a
# fragment may be a list literal (["..."]) or a glob() call.
SRCS_HOOKS = {
    # include_str!("../examples/*.json") — data outside the src/ glob.
    "partner-chains-mock-data-sources": ['glob(["examples/**/*.json"])'],
    # (res moved to the `res/` prefix + PREFIX_DATA_GLOBS — see MAPPED_SRCS_EXTERNAL.)
    # subxt::subxt(runtime_metadata_path="static/*.scale") reads the SCALE
    # metadata blobs at macro-expansion time.
    "midnight-node-metadata": ['glob(["static/**"])'],
    # graphql_client::GraphQLQuery derives (indexer_client.rs) read
    # graphql/indexer.graphql + graphql/schema/*.graphql at macro-expansion time,
    # relative to CARGO_MANIFEST_DIR; stage them like src/*.rs.
    "midnight-ledger-unsafe-helpers": ['glob(["graphql/**"])'],
}


def member_rel(pkg):
    d = os.path.dirname(pkg["manifest_path"])
    return os.path.relpath(d, ws_root)


def lib_target(pkg):
    for t in pkg["targets"]:
        if any(k in ("lib", "rlib", "proc-macro") for k in t["kind"]):
            return t
    return None


# extern-name -> third-party key: build from the same rules gen-third-party used
# (rename or name), so keys agree by construction.
def dep_label(dep_pkg, extern_name):
    """dep_pkg: the resolved package; extern_name: crate name rustc sees."""
    if dep_pkg["id"] in members:
        rel = member_rel(dep_pkg)
        return f"root//{rel}:{dep_pkg['name']}", underscore(lib_target(dep_pkg)["name"])
    # external: reindeer emits a top-level alias per manifest key; our manifest
    # keys were rename-or-name, and extern_name == underscore(key) for renames.
    return f"root//third-party:{extern_name.replace('_','-')}", None


def compute_deps(pkg, node, include_dev):
    """Return (deps, named, label_pkg) for a target. include_dev adds dev-deps."""
    deps, named, label_pkg = [], {}, {}
    # index declared deps by extern name (rename-or-libname) AND by package name.
    # The resolve node's dep name is the EXTERN name (the dep's lib name); for a
    # crate whose lib name differs from its package name and isn't renamed (e.g.
    # tiny-bip39, lib `bip39`), the extern lookup misses, so fall back to the
    # package name to pick the right third-party key.
    declared, declared_by_pkg = {}, {}
    for d in pkg["dependencies"]:
        declared[underscore(d.get("rename") or d["name"])] = d
        declared_by_pkg[d["name"]] = d
    for rd in node["deps"]:
        kinds = rd.get("dep_kinds") or [{}]
        allowed = ("dev",) if include_dev else ()
        # normal deps (kind == null) always; dev only for tests. build-only deps
        # never (buck doesn't run build scripts, so build-deps like
        # substrate-wasm-builder must not become link deps).
        if all(k.get("kind") not in (None,) + allowed for k in kinds):
            continue
        dpkg = pkg_by_id[rd["pkg"]]
        extern = rd["name"]  # already rename-applied + underscored
        dl = declared.get(extern) or declared_by_pkg.get(dpkg["name"])
        # crates whose lib name differs from package name (e.g. midnight-*)
        dlib = lib_target(dpkg)
        if dpkg["id"] in members:
            rel = member_rel(dpkg)
            label = f"root//{rel}:{dpkg['name']}"
            libname = underscore(dlib["name"]) if dlib else underscore(dpkg["name"])
            if extern != libname:
                named[extern] = label
            else:
                deps.append(label)
        else:
            if dl:
                key = keymap.get(
                    f'{dl["name"]}|{dl["req"]}|{dl["source"]}',
                    dl.get("rename") or dl["name"],
                )
            else:
                key = extern.replace("_", "-")
            label = f"root//third-party:{key}"
            libname = underscore(dlib["name"]) if dlib else underscore(dpkg["name"])
            if extern != libname:
                named[extern] = label
            else:
                deps.append(label)
        # label -> dependency crate name, for the wasm select: a dep
        # is kept on wasm32 only if its crate name is in the consumer's wasm dep
        # set (sidecar), so std-only deps like ledger's `lazy_static` are dropped.
        label_pkg[label] = dpkg["name"]
    return sorted(set(deps)), named, label_pkg


def gen_buck(pkg, prefix=""):
    # prefix: repo-relative crate dir when the target is emitted from the root
    # package (root-rooted); "" for a normal in-package member BUCK.
    pdir = (prefix + "/") if prefix else ""
    node = resolve_by_id[pkg["id"]]
    features = sorted(f for f in node.get("features", []) if f != "default")
    # Workspace lint flags, applied to every target of a crate that opts in.
    pkg_lints = LINT_FLAGS if opts_into_workspace_lints(pkg) else []
    lt = lib_target(pkg)
    bins = [t for t in pkg["targets"] if t["kind"] == ["bin"]]
    tests = [t for t in pkg["targets"] if t["kind"] == ["test"]]  # tests/*.rs
    has_build = any("custom-build" in t["kind"] for t in pkg["targets"])

    deps, named, label_pkg = compute_deps(pkg, node, include_dev=False)
    dev_deps, dev_named, _ = compute_deps(pkg, node, include_dev=True)

    def variant(by_id, include_dev, own_lib=None):
        # This package's features and deps as another metadata resolves them.
        vnode = by_id[pkg["id"]]
        vdeps, vnamed, _ = compute_deps(pkg, vnode, include_dev)
        if own_lib and lt:
            vdeps = sorted(set(vdeps + [f":{own_lib}"]))
        feats = sorted(f for f in vnode.get("features", []) if f != "default")
        return {"features": feats, "deps": vdeps, "named": vnamed}

    def lint_variant(include_dev, own_lib=False):
        # Emitted only where it differs from the default.
        return variant(
            lint_resolve_by_id, include_dev, pkg["name"] if own_lib else None
        )

    extra_env = ENV_HOOKS.get(pkg["name"], lambda p, n: {})(pkg, node)
    out = []
    out.append(
        "# @generated by scripts/buck2/gen-first-party.py — edit and regen there.\n"
    )
    if has_build and pkg["name"] not in ENV_HOOKS:
        out.append(
            "# NOTE: this crate has a build.rs which is NOT run under buck2 yet.\n"
        )

    def emit_select(field, std, alts, brackets, block, same):
        # A plain value, or a `select()` when a variant (wasm32, lint) differs.
        opener, closer = brackets
        alts = [(c, v) for c, v in alts if v is not None and not same(v, std)]
        if not alts:
            if std:
                out.append(f"    {field} = {opener}\n")
                block("        ", std)
                out.append(f"    {closer},\n")
            return
        out.append(f"    {field} = select({{\n")
        for cond, vals in alts + [("DEFAULT", std)]:
            out.append(f'        "{cond}": {opener}\n')
            block("            ", vals)
            out.append(f"        {closer},\n")
        out.append("    }),\n")

    def emit_list_field(field, std_vals, wasm_vals, lint_vals=None):
        def block(indent, vals):
            for v in vals:
                out.append(f'{indent}"{v}",\n')

        alts = [("prelude//cpu:wasm32", wasm_vals and sorted(wasm_vals))]
        same = lambda a, b: sorted(a) == sorted(b)
        n = len(out)
        emit_select(field, std_vals, alts, "[]", block, same)
        # The lint feature set only ever adds, so append its extras rather than
        # repeating the whole list in a second select.
        extra = sorted(set(lint_vals or ()) - set(std_vals))
        if not extra:
            return
        if len(out) == n:
            out.append(f"    {field} = select({{\n")
        else:
            out[-1] = out[-1].removesuffix(",\n") + " + select({\n"
        out.append(f'        "{LINT}": [\n')
        block("            ", extra)
        out.append('        ],\n        "DEFAULT": [],\n    }),\n')

    def emit_named_field(field, std_map, wasm_map, lint_map=None):
        def block(indent, m):
            for k, v in sorted(m.items()):
                out.append(f'{indent}"{k}": "{v}",\n')

        alts = [("prelude//cpu:wasm32", wasm_map), (LINT, lint_map)]
        emit_select(field, std_map, alts, "{}", block, lambda a, b: a == b)

    def emit_target(
        rule,
        name,
        crate,
        crate_root,
        tdeps,
        tnamed,
        extra=(),
        extra_globs=(),
        wasm=None,
        lint=None,
        feats=None,
        env_override=None,
        labels=None,
        rustc_flags=(),
    ):
        out.append(f"{rule}(\n")
        out.append(f'    name = "{name}",\n')
        out.append(f'    crate = "{crate}",\n')
        out.append(f'    crate_root = "{crate_root}",\n')
        out.append(f'    edition = "{pkg["edition"]}",\n')
        if prefix:
            # mapped_srcs mirror the repo layout inside the package sandbox so
            # ../ include!s resolve. Own files -> <rel>/…, externals via labels.
            ext = "".join(
                f'            ("{label}", "{dest}"),\n'
                for label, dest in sorted(
                    MAPPED_SRCS_EXTERNAL.get(pkg["name"], {}).items()
                )
            )
            globs = '"src/**", "Cargo.toml"' + "".join(
                f', "{g}"'
                for g in tuple(extra_globs)
                + tuple(PREFIX_DATA_GLOBS.get(pkg["name"], ()))
            )
            out.append("    mapped_srcs = dict(\n")
            out.append(
                f'        [(f, "{prefix}/" + f) for f in ' f"glob([{globs}])] + [\n"
            )
            out.append(ext)
            out.append("        ],\n    ),\n")
        else:
            extra_srcs = list(SRCS_HOOKS.get(pkg["name"], []))
            # glob all of src/** (not just *.rs) so include_str!/include_bytes!
            # of data files colocated under src/ (e.g. *.json) are in the sandbox.
            base_globs = '["src/**"' + "".join(f', "{g}"' for g in extra_globs) + "]"
            srcs_expr = f'glob({base_globs}) + ["Cargo.toml"]'
            for frag in extra_srcs:
                srcs_expr += " + " + frag
            out.append(f"    srcs = {srcs_expr},\n")
        emit_list_field(
            "features",
            feats if feats is not None else features,
            wasm["features"] if wasm else None,
            lint["features"] if lint and feats is None else None,
        )
        vparts = (re.findall(r"\d+", pkg["version"]) + ["0", "0", "0"])[:3]
        out.append("    env = {\n")
        out.append(f'        "CARGO_MANIFEST_DIR": "{prefix or "."}",\n')
        out.append(f'        "CARGO_PKG_NAME": "{pkg["name"]}",\n')
        out.append(f'        "CARGO_PKG_VERSION": "{pkg["version"]}",\n')
        # frame_support::pallet macro reads CARGO_PKG_VERSION_MAJOR/MINOR/PATCH
        out.append(f'        "CARGO_PKG_VERSION_MAJOR": "{vparts[0]}",\n')
        out.append(f'        "CARGO_PKG_VERSION_MINOR": "{vparts[1]}",\n')
        out.append(f'        "CARGO_PKG_VERSION_PATCH": "{vparts[2]}",\n')
        # clap's #[command(author/about)] derive reads these at compile time
        out.append(
            f'        "CARGO_PKG_AUTHORS": {json.dumps(":".join(pkg.get("authors") or []))},\n'
        )
        out.append(
            f'        "CARGO_PKG_DESCRIPTION": {json.dumps(pkg.get("description") or "")},\n'
        )
        out.append(
            f'        "CARGO_PKG_REPOSITORY": {json.dumps(pkg.get("repository") or "")},\n'
        )
        out.append(
            f'        "CARGO_PKG_HOMEPAGE": {json.dumps(pkg.get("homepage") or "")},\n'
        )
        env = dict(extra_env)
        if env_override:
            env.update(env_override)
        for k, v in sorted(env.items()):
            value = v if isinstance(v, Starlark) else json.dumps(v)
            out.append(f'        "{k}": {value},\n')
        out.append("    },\n")
        # Declared inputs only (see TEST_RESOURCES).
        if rule == "rust_test":
            res_expr = TEST_TARGET_RESOURCES.get(name) or TEST_RESOURCES.get(
                pkg["name"]
            )
            if res_expr:
                out.append(f"    resources = {res_expr},\n")
        rflags = pkg_lints + list(rustc_flags)
        if rflags:
            out.append(
                "    rustc_flags = [" + ", ".join(f'"{x}"' for x in rflags) + "],\n"
            )
        for e in extra:
            out.append(e)
        emit_list_field(
            "deps",
            tdeps,
            wasm["deps"] if wasm else None,
            lint["deps"] if lint else None,
        )
        emit_named_field(
            "named_deps",
            tnamed,
            wasm["named"] if wasm else None,
            lint["named"] if lint else None,
        )
        if labels:
            out.append("    labels = [" + ", ".join(f'"{l}"' for l in labels) + "],\n")
        out.append('    visibility = ["PUBLIC"],\n')
        out.append(")\n")

    def rel_root(t):
        return pdir + os.path.relpath(
            t["src_path"], os.path.dirname(pkg["manifest_path"])
        )

    if lt:
        is_proc = "proc-macro" in lt["kind"]
        extra = ["    proc_macro = True,\n"] if is_proc else []
        # WASM variant: closure libs (not proc-macros — those build on
        # the host) get a `select()` swapping std deps/features for the no_std set.
        wasm_lib = None
        if not is_proc and pkg["name"] in wasm_features_map:
            info = wasm_features_map[pkg["name"]]
            wset = set(info["deps"])
            wdeps = sorted({w for d in deps if (w := to_wasm_dep(d, wset, label_pkg))})
            wnamed = {
                k: w for k, v in named.items() if (w := to_wasm_dep(v, wset, label_pkg))
            }
            wasm_lib = {"features": info["features"], "deps": wdeps, "named": wnamed}
        emit_target(
            "rust_library",
            pkg["name"],
            underscore(lt["name"]),
            rel_root(lt),
            deps,
            named,
            extra,
            wasm=wasm_lib,
            lint=lint_variant(include_dev=False),
        )
        # cdylib variant of the runtime for the on-chain wasm blob:
        # always the no_std wasm deps/features, shared linkage, and
        # `--cfg substrate_runtime` (so #[runtime_interface] emits wasm extern
        # stubs). Built for wasm32 via a configured_alias in the root BUCK.
        if pkg["name"] in WASM_CDYLIB and wasm_lib:
            out.append("\n")
            emit_target(
                "rust_library",
                f"{pkg['name']}-wasm-cdylib",
                underscore(lt["name"]),
                rel_root(lt),
                wasm_lib["deps"],
                wasm_lib["named"],
                extra=['    preferred_linkage = "shared",\n'],
                rustc_flags=["--cfg", "substrate_runtime"],
                feats=wasm_lib["features"],
                # The cdylib IS the blob source — it must use the stub
                # OUT_DIR, not runtime-wasm-binary-rs (which embeds this
                # cdylib), else a configured-target cycle.
                env_override={"OUT_DIR": "$(location root//:runtime-wasm-stub)"},
            )
    for b in bins:
        name = b["name"] if not lt or b["name"] != pkg["name"] else b["name"] + "-bin"
        bdeps = sorted(set(deps + ([f":{pkg['name']}"] if lt else [])))
        out.append("\n")
        emit_target(
            "rust_binary",
            name,
            underscore(b["name"]),
            rel_root(b),
            bdeps,
            named,
            lint=lint_variant(include_dev=False, own_lib=True),
        )

    # ── tests ──────────────────────────────────────────────────────────────
    # Unit tests: recompile the lib crate with the test harness (runs #[test]
    # fns in src). Integration tests: each tests/*.rs is its own crate that
    # links the lib. Both get dev-deps.
    test_globs = tuple(TEST_SRCS_HOOKS.get(pkg["name"], []))
    if lt and "proc-macro" not in lt["kind"] and pkg["name"] not in NO_UNIT_TEST:
        out.append("\n")
        emit_target(
            "rust_test",
            f"{pkg['name']}-unit-test",
            underscore(lt["name"]),
            rel_root(lt),
            dev_deps,
            dev_named,
            extra_globs=test_globs,
            lint=lint_variant(include_dev=True),
            env_override=UNIT_TEST_ENV.get(pkg["name"]),
            labels=["ci-infra"] if pkg["name"] in CI_INFRA_UNIT else None,
        )
    for t in tests:
        # cargo skips an integration test whose required-features are off. CI
        # turns them on for the run (regen.sh TEST_FEATURES), which under buck2
        # means a second build of the lib with those features for the test to link.
        req = t.get("required-features") or []
        if any(f not in features for f in req):
            lib = f"{pkg['name']}-{'-'.join(req)}"
            vlib = variant(test_resolve_by_id, include_dev=False)
            if not lt or any(f not in vlib["features"] for f in req):
                continue
            vtest = variant(test_resolve_by_id, include_dev=True, own_lib=lib)
            for name, rule, v, globs in (
                (lib, "rust_library", vlib, ()),
                (f"{pkg['name']}-{t['name']}", "rust_test", vtest, ("tests/**",)),
            ):
                is_test = rule == "rust_test"
                out.append("\n")
                emit_target(
                    rule,
                    name,
                    underscore((t if is_test else lt)["name"]),
                    rel_root(t if is_test else lt),
                    v["deps"],
                    v["named"],
                    extra_globs=globs + (test_globs if is_test else ()),
                    feats=v["features"],
                )
            continue
        tdeps = sorted(set(dev_deps + ([f":{pkg['name']}"] if lt else [])))
        out.append("\n")
        emit_target(
            "rust_test",
            f"{pkg['name']}-{t['name']}",
            underscore(t["name"]),
            rel_root(t),
            tdeps,
            dev_named,
            extra_globs=("tests/**",) + test_globs,
            lint=lint_variant(include_dev=True, own_lib=True),
            env_override=TEST_ENV_HOOKS.get((pkg["name"], t["name"])),
            labels=(
                ["ci-infra"]
                if (pkg["name"], t["name"]) in CI_INFRA_INTEGRATION
                else None
            ),
        )
    return "".join(out)


written = set()
for mid in sorted(members):
    pkg = pkg_by_id[mid]
    if only and pkg["name"] not in only:
        continue
    rel = member_rel(pkg)
    # mapped-srcs crates (escaping include!s) stay in their own package but
    # mirror the repo layout via mapped_srcs (prefix = their repo-rel dir).
    prefix = rel if pkg["name"] in MAPPED_SRCS_EXTERNAL else ""
    body = gen_buck(pkg, prefix=prefix)
    exports = EXPORTS.get(pkg["name"])
    if exports:
        body = "\n\n".join(exports) + "\n\n" + body
    path = os.path.join(ws_root, rel, "BUCK")
    with open(path, "w") as f:
        f.write(body)
    print(f"wrote {rel}/BUCK")
    written.add(path)
print(f"{len(written)} BUCK files")

# A crate that left the workspace leaves its BUCK behind, and root//... then
# fails to load: remove every BUCK this script once wrote and no longer does.
SKIP_DIRS = {"target", "buck-out", "node_modules", "third-party", "wasm-deps"}
if not only:
    for d, dirs, files in os.walk(ws_root):
        dirs[:] = [x for x in dirs if not x.startswith(".") and x not in SKIP_DIRS]
        path = os.path.join(d, "BUCK")
        if "BUCK" not in files or path in written:
            continue
        with open(path) as f:
            stale = "@generated by scripts/buck2/gen-first-party.py" in f.read()
        if stale:
            os.remove(path)
            print(f"removed {os.path.relpath(path, ws_root)}")
