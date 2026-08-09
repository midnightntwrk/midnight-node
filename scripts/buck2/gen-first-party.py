#!/usr/bin/env python3
"""Generate BUCK files for workspace members from cargo metadata.

Usage: gen-first-party.py <meta.json> [member-name ...]
Without member names, generates for every workspace member.

Maps path deps to root//<dir>:<name> and external deps to root//third-party:<key>
(key = the [dependencies] key in third-party/Cargo.toml, i.e. the cargo rename
if present). Renamed deps land in named_deps so the extern crate name matches.
Features come from the workspace-resolved feature set (cargo's unification).
Build scripts are NOT run — members with build.rs get a placeholder comment.
"""
import json, os, re, sys
from collections import defaultdict

META = sys.argv[1]
only = set(sys.argv[2:])

meta = json.load(open(META))
ws_root = meta["workspace_root"]
# (name, req, source) -> third-party manifest key, written by gen-third-party.py
keymap = json.load(open(os.path.join(ws_root, "third-party", "keys.json")))
members = set(meta["workspace_members"])
pkg_by_id = {p["id"]: p for p in meta["packages"]}
resolve_by_id = {n["id"]: n for n in meta["resolve"]["nodes"]}

# WASM variant data (task #11). `first-party-wasm.json` (from gen-wasm-deps.py)
# maps each first-party crate in the runtime's no_std closure to its wasm feature
# set; `wasm_names` is the set of wasm-deps buck target names. Together they let
# each closure crate's lib emit `select()` on `cpu:wasm32` that swaps the std
# feature set for the no_std one and repoints `root//third-party:X` deps to
# `root//wasm-deps:X` (dropping std-only deps like frame-benchmarking that have
# no wasm-deps target). First-party deps are left as-is — buck's platform
# transition rebuilds them for wasm32, and they carry the same `select()`.
_wasm_sidecar = os.path.join(ws_root, "wasm-deps", "first-party-wasm.json")
wasm_features_map = json.load(open(_wasm_sidecar)) if os.path.exists(_wasm_sidecar) else {}
wasm_names = set()
_wasm_buck = os.path.join(ws_root, "wasm-deps", "BUCK")
if os.path.exists(_wasm_buck):
    wasm_names = set(re.findall(r'^    name = "([^"]+)"', open(_wasm_buck).read(), re.M))

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
    git = source[len("git+"):]
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
    aliases = [("mn_ledger", "LEDGER_7_VERSION"),
               ("mn_ledger_8", "LEDGER_8_VERSION"),
               ("mn_ledger_9", "LEDGER_9_VERSION")]
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
        # sqlx-macros (0.9) verifies query_as!/query! at compile time against the
        # .sqlx offline cache, found by trying (in order) SQLX_OFFLINE_DIR,
        # <manifest_dir>/.sqlx, then <workspace_root>/.sqlx — the last shells out to
        # `cargo metadata`, which EOFs in buck's sandboxed __srcs tree (no cargo
        # workspace). We land on candidate #2: the crate carries its own real .sqlx/
        # (SRC_HOOKS globs it into srcs, so it stages exactly like src/*.rs — a
        # __srcs symlink to project source, which the remote worker materializes),
        # and CARGO_MANIFEST_DIR="." is auto-absolutized by the rust prelude
        # (_DIRECTORY_ENV) to the __srcs root, so <manifest_dir>/.sqlx resolves on
        # both local and RE without cargo metadata. A relative SQLX_OFFLINE_DIR
        # missed (rustc's CWD is the repo root, not __srcs) and a $(location) one
        # missed too (that filegroup artifact isn't materialized at its logical
        # path under rebuck2's CAS). Proven with CARGO=/bin/false: build still green
        # => cargo metadata was never invoked. CARGO stays set to satisfy sqlx's
        # pre-offline presence check.
        "CARGO": "cargo",
        "SQLX_OFFLINE": "true",
    },
    # runtime lib.rs include!s $OUT_DIR/wasm_binary.rs; point at the stub dir
    # (WASM_BINARY=None) since substrate-wasm-builder can't run under buck.
    "midnight-node-runtime": lambda p, n: {
        # Real on-chain blob (task #12): OUT_DIR provides a wasm_binary.rs that
        # include_bytes!'s the wasm-opt'd + zstd-compressed runtime. The runtime
        # lib is std here, but the blob is the wasm cdylib built via the wasm32
        # configured_alias — so no build cycle. Was :runtime-wasm-stub (None).
        "OUT_DIR": "$(location root//:runtime-wasm-binary-rs)",
    },
    "partner-chains-demo-runtime": lambda p, n: {
        "OUT_DIR": "$(location root//:runtime-wasm-stub)",
    },
    # node/build.rs (generate_cargo_keys) emits these from git; provide static
    # values so the CLI --version works without running git under buck.
    "midnight-node": lambda p, n: {
        "SUBSTRATE_CLI_IMPL_VERSION": f"{p['version']}-buck",
        "SUBSTRATE_CLI_COMMIT_HASH": "",
    },
}

# package name -> extra srcs labels (non-rust inputs the crate needs in the
# rustc srcs tree, e.g. codegen caches or include_str! data). rustc runs with
# CWD = the __srcs tree root (= CARGO_MANIFEST_DIR "."), so these land relative
# to it: .env at ./.env, the sqlx filegroup at ./.sqlx/.
# Crates whose sources include_str!/include_bytes! files ABOVE their own crate
# dir (e.g. ../../../COMPACTC_VERSION). rustc resolves include! relative to the
# source file, but buck sandboxes srcs rooted at the target's package, so those
# paths escape. Fix (as reindeer does for git crates): use mapped_srcs to build
# a srcs tree that MIRRORS the repo — the crate's own files at <rel>/… and the
# external files at their repo-relative paths — with crate_root/CARGO_MANIFEST_DIR
# prefixed by <rel>. The external files live in other buck packages, so they're
# pulled via export_file labels (see EXPORTS). Value: {label: dest-in-tree}.
# Starlark `resources` expr per package whose TEST targets read files at runtime.
# See the emit_target comment: resource `n` lands at <package>/<n>, project-relative.
TEST_RESOURCES = {
    # res tests: locate_workspace_root() walks up from CWD (= project root) for
    # res/cfg/default.toml, then read cfg presets + chainspec/openrpc/genesis blobs.
    # Globbed relative to the res package, they materialize back at res/… .
    "midnight-node-res": 'glob(["cfg/**", "**/*.json", "**/*.mn", "**/*.hbs"])',
    # node tests call into res's locate_workspace_root(); pull res's fixtures in at
    # the project-relative res/ layout so the walk + config reads resolve.
    "midnight-node": '{"../res": "root//res:test-fixtures"}',
}

# Per-TEST-TARGET resources (keyed by rust_test name), for packages whose test
# targets need different runtime files. Takes precedence over TEST_RESOURCES.
TEST_TARGET_RESOURCES = {
    # cached_context reads test-data/genesis/genesis_block_undeployed.mn at runtime
    # (via CARGO_MANIFEST_DIR, resolved at runtime after the env!-> std::env::var fix).
    "midnight-node-toolkit-cached_context": 'glob(["test-data/genesis/**"])',
}

# Data-file globs a prefixed package needs in addition to src/** + Cargo.toml
# (the `else` branch gets these via SRCS_HOOKS; the prefix branch needs them here
# so runtime fixture reads resolve at the project-relative layout).
PREFIX_DATA_GLOBS = {
    # res tests walk up from the test's CWD (= project root, since buck2 runs
    # tests with run_from_project_root=True) for `res/cfg/default.toml`, then read
    # cfg tomls + chainspec/genesis blobs. Staging res's files at the `res/` prefix
    # puts them at exactly those project-relative paths. Note the *.toml — the cfg
    # presets are toml, absent from the .mn/.json/.hbs data set.
    "midnight-node-res": ["**/*.mn", "**/*.json", "**/*.hbs", "**/*.toml"],
}

MAPPED_SRCS_EXTERNAL = {
    # res owns every file its tests read, so no cross-package labels — membership
    # here (even empty) just switches it to the project-relative `res/` prefix.
    "midnight-node-res": {},
    "midnight-node-toolkit": {
        "root//:COMPACTC_VERSION": "COMPACTC_VERSION",
        "root//node:cargo-toml": "node/Cargo.toml",
        "root//res:test-contract-addr":
            "res/test-contract/contract_address_undeployed.mn",
        # cli_tests (trycmd) fixtures the README examples + tomls read via
        # `../../res/…` (CWD = crate dir). Mapped so they land at __srcs/res/….
        "root//res:test-contract-tx1":
            "res/test-contract/contract_tx_1_deploy_undeployed.mn",
        "root//res:genesis-block-undeployed":
            "res/genesis/genesis_block_undeployed.mn",
        "root//res:serialized-tx":
            "res/test-tx-deserialize/serialized_tx.mn",
    },
}

# Files a package must export_file so a mapped-srcs crate can reference them
# across package boundaries. member name -> list of export target texts.
EXPORTS = {
    "midnight-node": [
        'export_file(name = "cargo-toml", src = "Cargo.toml", visibility = ["PUBLIC"])',
    ],
    "midnight-node-res": [
        'export_file(\n    name = "test-contract-addr",\n'
        '    src = "test-contract/contract_address_undeployed.mn",\n'
        '    visibility = ["PUBLIC"],\n)',
        'export_file(\n    name = "test-contract-tx1",\n'
        '    src = "test-contract/contract_tx_1_deploy_undeployed.mn",\n'
        '    visibility = ["PUBLIC"],\n)',
        'export_file(\n    name = "genesis-block-undeployed",\n'
        '    src = "genesis/genesis_block_undeployed.mn",\n'
        '    visibility = ["PUBLIC"],\n)',
        'export_file(\n    name = "serialized-tx",\n'
        '    src = "test-tx-deserialize/serialized_tx.mn",\n'
        '    visibility = ["PUBLIC"],\n)',
        # Runtime fixtures other crates' tests read after locate_workspace_root()
        # anchors on res/cfg/default.toml (cfg presets + per-network config blobs).
        # Consumers add `resources = {"../res": "root//res:test-fixtures"}` so it
        # materializes back at the project-relative res/… layout.
        'filegroup(\n    name = "test-fixtures",\n'
        '    srcs = glob(["cfg/**", "**/*.json", "**/*.mn", "**/*.hbs"]),\n'
        '    visibility = ["PUBLIC"],\n)',
    ],
    # cli_tests (trycmd) runs `$ midnight-node-toolkit …` and diffs clap's usage
    # strings, which echo argv[0]'s basename. Buck's bin artifact is
    # `midnight_node_toolkit` (underscore); cargo names it `midnight-node-toolkit`
    # (hyphen). Provide a hyphen-named copy so the usage output matches the README.
    "midnight-node-toolkit": [
        'genrule(\n    name = "toolkit-hyphen-bin",\n'
        '    out = "midnight-node-toolkit",\n'
        '    cmd = "cp $(location :midnight-node-toolkit-bin) $OUT",\n'
        '    visibility = ["PUBLIC"],\n)',
    ],
}

# Per-(package, test-name) env overrides for integration tests. cli_tests (trycmd)
# needs CARGO_BIN_EXE_<name> to locate the binary (cargo sets it; buck doesn't) —
# point it at the hyphen-named copy so clap's usage strings match the README.
TEST_ENV_HOOKS = {
    ("midnight-node-toolkit", "cli_tests"): {
        "CARGO_BIN_EXE_midnight-node-toolkit": "$(location :toolkit-hyphen-bin)",
    },
}

# Crates to skip emitting a unit-test target for. sc-partner-chains-consensus-aura's
# tests all need substrate-test-runtime's dev wasm binary (a polkadot-sdk internal
# test fixture built by substrate-wasm-builder) — out of scope: it'd need a second
# wasm crate-universe + cdylib/compress pipeline just for a third-party test runtime.
NO_UNIT_TEST = {"sc-partner-chains-consensus-aura"}

# Tests that need external infra (Docker/testcontainers, a live devnet, a Cardano
# node) get a `ci-infra` label so the distributed CI can `buck2 test root//...
# --exclude ci-infra`. They still run locally against the real stack.
CI_INFRA_UNIT = {"partner-chains-db-sync-data-sources"}  # testcontainers → postgres
CI_INFRA_INTEGRATION = {
    ("partner-chains-cardano-offchain", "integration_tests"),  # Cardano node
    ("midnight-node-e2e", "e2e_tests"),                        # devnet
    ("midnight-node-toolkit", "single_tx"),                    # docker-compose
    ("midnight-node-toolkit", "toolkit_e2e"),                  # devnet / ledger static dir
}

# Extra src globs a crate's TEST targets need (test-data include_str!'d only by
# #[cfg(test)] code, sqlx::migrate! dirs, …). Applied to unit + integration
# tests, not the lib/bin.
# Crates that additionally get a `<name>-wasm-cdylib` shared-library target — the
# on-chain wasm blob source (task #12). Only the runtime; a configured_alias in
# the root BUCK builds it for wasm32, then wasm-opt + zstd produce the blob.
WASM_CDYLIB = {"midnight-node-runtime"}

TEST_SRCS_HOOKS = {
    "midnight-node-ledger": ["test-data/**"],
    "partner-chains-db-sync-data-sources": ["testdata/**"],
    # toolkit tests read test-data/ via env!("CARGO_MANIFEST_DIR"), which buck
    # bakes to the sandbox __srcs dir — map the fixtures in so they resolve.
    # README.md is the trycmd corpus for cli_tests.
    "midnight-node-toolkit": ["test-data/**", "README.md"],
}

# each value is a list of starlark expressions added to `srcs` with `+`; a
# fragment may be a list literal (["..."]) or a glob() call.
SRCS_HOOKS = {
    # Crate-local .sqlx/ (real source, copied from the workspace root cache) is
    # globbed in so it stages like src/*.rs and sqlx finds it via <manifest_dir>/
    # .sqlx on the remote worker (see ENV_HOOKS). .env kept for local cargo.
    "midnight-primitives-mainchain-follower": ['glob([".sqlx/**"])', '[".env"]'],
    # include_str!("../examples/*.json") — data outside the src/ glob.
    "partner-chains-mock-data-sources": ['glob(["examples/**/*.json"])'],
    # (res moved to the `res/` prefix + PREFIX_DATA_GLOBS — see MAPPED_SRCS_EXTERNAL.)
    # subxt::subxt(runtime_metadata_path="static/*.scale") reads the SCALE
    # metadata blobs at macro-expansion time.
    "midnight-node-metadata": ['glob(["static/**"])'],
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
                key = keymap.get(f'{dl["name"]}|{dl["req"]}|{dl["source"]}',
                                 dl.get("rename") or dl["name"])
            else:
                key = extern.replace("_", "-")
            label = f"root//third-party:{key}"
            libname = underscore(dlib["name"]) if dlib else underscore(dpkg["name"])
            if extern != libname:
                named[extern] = label
            else:
                deps.append(label)
        # label -> dependency crate name, for the wasm select (task #11): a dep
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
    lt = lib_target(pkg)
    bins = [t for t in pkg["targets"] if t["kind"] == ["bin"]]
    tests = [t for t in pkg["targets"] if t["kind"] == ["test"]]  # tests/*.rs
    has_build = any("custom-build" in t["kind"] for t in pkg["targets"])

    deps, named, label_pkg = compute_deps(pkg, node, include_dev=False)
    dev_deps, dev_named, _ = compute_deps(pkg, node, include_dev=True)
    extra_env = ENV_HOOKS.get(pkg["name"], lambda p, n: {})(pkg, node)
    out = []
    out.append("# @generated by scripts/buck2/gen-first-party.py — edit and regen there.\n")
    if has_build and pkg["name"] not in ENV_HOOKS:
        out.append("# NOTE: this crate has a build.rs which is NOT run under buck2 yet.\n")

    def emit_list_field(field, std_vals, wasm_vals, quote='"{}"'):
        # Emit a list-valued field, as a `select()` on cpu:wasm32 when a wasm
        # variant is supplied and differs, else a plain list.
        def block(indent, vals):
            for v in vals:
                out.append(f'{indent}{quote.format(v)},\n')
        if wasm_vals is None or sorted(wasm_vals) == sorted(std_vals):
            if std_vals:
                out.append(f"    {field} = [\n"); block("        ", std_vals)
                out.append("    ],\n")
            return
        out.append(f"    {field} = select({{\n")
        out.append('        "prelude//cpu:wasm32": [\n'); block("            ", sorted(wasm_vals))
        out.append("        ],\n")
        out.append('        "DEFAULT": [\n'); block("            ", std_vals)
        out.append("        ],\n    }),\n")

    def emit_named_field(field, std_map, wasm_map):
        def block(indent, m):
            for k, v in sorted(m.items()):
                out.append(f'{indent}"{k}": "{v}",\n')
        if wasm_map is None or wasm_map == std_map:
            if std_map:
                out.append(f"    {field} = {{\n"); block("        ", std_map)
                out.append("    },\n")
            return
        out.append(f"    {field} = select({{\n")
        out.append('        "prelude//cpu:wasm32": {\n'); block("            ", wasm_map)
        out.append("        },\n")
        out.append('        "DEFAULT": {\n'); block("            ", std_map)
        out.append("        },\n    }),\n")

    def emit_target(rule, name, crate, crate_root, tdeps, tnamed,
                    extra=(), extra_globs=(), wasm=None, feats=None,
                    env_override=None, labels=None):
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
                for label, dest in sorted(MAPPED_SRCS_EXTERNAL.get(pkg["name"], {}).items())
            )
            globs = '"src/**", "Cargo.toml"' + "".join(
                f', "{g}"' for g in tuple(extra_globs) + tuple(PREFIX_DATA_GLOBS.get(pkg["name"], ())))
            out.append("    mapped_srcs = dict(\n")
            out.append(f'        [(f, "{prefix}/" + f) for f in '
                       f'glob([{globs}])] + [\n')
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
            out.append(f'    srcs = {srcs_expr},\n')
        emit_list_field("features", feats if feats is not None else features,
                        wasm["features"] if wasm else None)
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
        out.append(f'        "CARGO_PKG_AUTHORS": {json.dumps(":".join(pkg.get("authors") or []))},\n')
        out.append(f'        "CARGO_PKG_DESCRIPTION": {json.dumps(pkg.get("description") or "")},\n')
        out.append(f'        "CARGO_PKG_REPOSITORY": {json.dumps(pkg.get("repository") or "")},\n')
        out.append(f'        "CARGO_PKG_HOMEPAGE": {json.dumps(pkg.get("homepage") or "")},\n')
        env = dict(extra_env)
        if env_override:
            env.update(env_override)
        for k, v in sorted(env.items()):
            out.append(f'        "{k}": {json.dumps(v)},\n')
        out.append("    },\n")
        # Runtime resources: buck2 runs rust tests with run_from_project_root=True in
        # an otherwise-empty remote sandbox, so files a test reads at runtime (not at
        # compile) must be declared here. Each resource named `n` materializes at the
        # project-relative path <package>/<n>, i.e. exactly where CWD-relative /
        # workspace-root-walk lookups expect it. Only test targets get these.
        if rule == "rust_test":
            res_expr = TEST_TARGET_RESOURCES.get(name) or TEST_RESOURCES.get(pkg["name"])
            if res_expr:
                out.append(f'    resources = {res_expr},\n')
        for e in extra:
            out.append(e)
        emit_list_field("deps", tdeps, wasm["deps"] if wasm else None)
        emit_named_field("named_deps", tnamed, wasm["named"] if wasm else None)
        if labels:
            out.append("    labels = [" + ", ".join(f'"{l}"' for l in labels) + "],\n")
        out.append('    visibility = ["PUBLIC"],\n')
        out.append(")\n")

    def rel_root(t):
        return pdir + os.path.relpath(t["src_path"], os.path.dirname(pkg["manifest_path"]))

    if lt:
        is_proc = "proc-macro" in lt["kind"]
        extra = ['    proc_macro = True,\n'] if is_proc else []
        # WASM variant (task #11): closure libs (not proc-macros — those build on
        # the host) get a `select()` swapping std deps/features for the no_std set.
        wasm_lib = None
        if not is_proc and pkg["name"] in wasm_features_map:
            info = wasm_features_map[pkg["name"]]
            wset = set(info["deps"])
            wdeps = sorted({w for d in deps if (w := to_wasm_dep(d, wset, label_pkg))})
            wnamed = {k: w for k, v in named.items()
                      if (w := to_wasm_dep(v, wset, label_pkg))}
            wasm_lib = {"features": info["features"], "deps": wdeps, "named": wnamed}
        emit_target("rust_library", pkg["name"], underscore(lt["name"]),
                    rel_root(lt), deps, named, extra, wasm=wasm_lib)
        # cdylib variant of the runtime for the on-chain wasm blob (task #12):
        # always the no_std wasm deps/features, shared linkage, and
        # `--cfg substrate_runtime` (so #[runtime_interface] emits wasm extern
        # stubs). Built for wasm32 via a configured_alias in the root BUCK.
        if pkg["name"] in WASM_CDYLIB and wasm_lib:
            out.append("\n")
            emit_target("rust_library", f"{pkg['name']}-wasm-cdylib",
                        underscore(lt["name"]), rel_root(lt),
                        wasm_lib["deps"], wasm_lib["named"],
                        extra=['    preferred_linkage = "shared",\n',
                               '    rustc_flags = ["--cfg", "substrate_runtime"],\n'],
                        feats=wasm_lib["features"],
                        # The cdylib IS the blob source — it must use the stub
                        # OUT_DIR, not runtime-wasm-binary-rs (which embeds this
                        # cdylib), else a configured-target cycle.
                        env_override={"OUT_DIR": "$(location root//:runtime-wasm-stub)"})
    for b in bins:
        name = b["name"] if not lt or b["name"] != pkg["name"] else b["name"] + "-bin"
        bdeps = sorted(set(deps + ([f":{pkg['name']}"] if lt else [])))
        out.append("\n")
        emit_target("rust_binary", name, underscore(b["name"]), rel_root(b),
                    bdeps, named)

    # ── tests ──────────────────────────────────────────────────────────────
    # Unit tests: recompile the lib crate with the test harness (runs #[test]
    # fns in src). Integration tests: each tests/*.rs is its own crate that
    # links the lib. Both get dev-deps.
    test_globs = tuple(TEST_SRCS_HOOKS.get(pkg["name"], []))
    if lt and "proc-macro" not in lt["kind"] and pkg["name"] not in NO_UNIT_TEST:
        out.append("\n")
        emit_target("rust_test", f"{pkg['name']}-unit-test", underscore(lt["name"]),
                    rel_root(lt), dev_deps, dev_named, extra_globs=test_globs,
                    labels=["ci-infra"] if pkg["name"] in CI_INFRA_UNIT else None)
    for t in tests:
        tdeps = sorted(set(dev_deps + ([f":{pkg['name']}"] if lt else [])))
        out.append("\n")
        emit_target("rust_test", f"{pkg['name']}-{t['name']}", underscore(t["name"]),
                    rel_root(t), tdeps, dev_named,
                    extra_globs=("tests/**",) + test_globs,
                    env_override=TEST_ENV_HOOKS.get((pkg["name"], t["name"])),
                    labels=["ci-infra"] if (pkg["name"], t["name"]) in CI_INFRA_INTEGRATION else None)
    return "".join(out)

# Workspace members intentionally excluded from the buck2 build/test set. The
# partner-chains demo node + runtime are an example that isn't part of the main
# product (its runtime isn't in the primary Cargo.toml's build), and building it
# would need a second wasm crate-universe just for the demo. Out of scope.
SKIP_MEMBERS = {"partner-chains-demo-node", "partner-chains-demo-runtime"}

count = 0
for mid in sorted(members):
    pkg = pkg_by_id[mid]
    if only and pkg["name"] not in only:
        continue
    if pkg["name"] in SKIP_MEMBERS:
        continue
    rel = member_rel(pkg)
    # mapped-srcs crates (escaping include!s) stay in their own package but
    # mirror the repo layout via mapped_srcs (prefix = their repo-rel dir).
    prefix = rel if pkg["name"] in MAPPED_SRCS_EXTERNAL else ""
    body = gen_buck(pkg, prefix=prefix)
    exports = EXPORTS.get(pkg["name"])
    if exports:
        body = "\n\n".join(exports) + "\n\n" + body
    with open(os.path.join(ws_root, rel, "BUCK"), "w") as f:
        f.write(body)
    print(f"wrote {rel}/BUCK")
    count += 1
print(f"{count} BUCK files")
