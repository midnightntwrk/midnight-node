#!/usr/bin/env python3
"""Generate third-party/Cargo.toml for reindeer from workspace cargo metadata.

Collects every external (non-path) normal/build dependency declared by any
workspace member, unioning features and default-features across declarations.
Dev-deps are skipped for now (node-binary milestone).
"""
import json, os, re, sys, tomllib, urllib.parse
from collections import defaultdict

META = sys.argv[1]
OUT = sys.argv[2]

meta = json.load(open(META))
root_manifest = tomllib.load(open(meta["workspace_root"] + "/Cargo.toml", "rb"))
members = set(meta["workspace_members"])
pkg_by_id = {p["id"]: p for p in meta["packages"]}

# key: (manifest_key, source) -> merged entry
entries = {}
conflicts = defaultdict(set)  # manifest_key -> set of sources

for mid in members:
    pkg = pkg_by_id[mid]
    for dep in pkg["dependencies"]:
        # include normal, build AND dev deps: rust_test targets need dev-deps,
        # and buck builds each third-party crate once, so the manifest must
        # carry the union.
        src = dep.get("source")
        if not src:  # path dep (workspace member or local)
            continue
        key = dep.get("rename") or dep["name"]
        target = dep.get("target")  # cfg string or None
        k = (key, src, dep["req"], target)
        e = entries.get(k)
        if e is None:
            e = {
                "name": dep["name"],
                "key": key,
                "source": src,
                "target": target,
                "req": dep["req"],
                "features": set(),
                "default": False,
                "optional_everywhere": True,
            }
            entries[k] = e
        e["features"].update(dep.get("features") or [])
        if dep.get("uses_default_features", True):
            e["default"] = True
        if not dep.get("optional", False):
            e["optional_everywhere"] = False
        conflicts[(key, target)].add((src, dep["req"]))

# Same manifest key declared with different (source, req) pairs across members
# (e.g. rand ^0.8 with `getrandom` vs rand ^0.10 where that feature is gone).
# One Cargo.toml can't hold two deps under one key. Variants whose reqs resolve
# to the same version in the workspace graph are merged (cargo would unify
# them); genuinely distinct versions get suffixed keys, e.g. `rand-0-8`.
def vparse(v):
    return tuple(int(n) for n in re.findall(r"\d+", v)[:3])

def satisfies(version, req):
    vt = vparse(version)
    for part in (p.strip() for p in req.split(",")):
        if not part:
            continue
        op = "^"
        if part[0] in "^=~<>":
            m = re.match(r"([\^=~]|>=|<=|<|>)\s*(.*)", part)
            op, base = m.group(1), m.group(2)
        else:
            base = part
        bt = vparse(base)
        if op in ("^", "~"):
            # caret: leftmost nonzero component must match, and version >= base
            digits = re.findall(r"\d+", base)
            if op == "^":
                idx = next((i for i, d in enumerate(digits) if d != "0"), len(digits) - 1)
            else:  # tilde: major.minor fixed
                idx = min(1, len(digits) - 1)
            if vt[: idx + 1] != bt[: idx + 1] or vt < bt:
                return False
        elif op == "=":
            if vt[: len(vparse(base))] != bt:
                return False
        elif op == ">=":
            if vt < bt:
                return False
        elif op == "<=":
            if vt > bt:
                return False
        elif op == "<":
            if vt >= bt:
                return False
        elif op == ">":
            if vt <= bt:
                return False
    return True

# buck2 builds each third-party crate ONCE with a fixed feature set — there is
# no per-consumer feature selection like cargo. Seed each entry with the
# feature set the crate resolves to in the *whole-workspace* build (cargo's
# unification), so e.g. `std` that first-party enables via feature propagation
# (runtime "std" -> "pallet-collective/std") is on. Without this, reindeer
# resolves features from the third-party manifest alone and FRAME crates build
# no_std, breaking every std-only genesis_build/host-function path.
resolved_features = {n["id"]: set(n.get("features") or []) for n in meta["resolve"]["nodes"]}

# Match by name + version-satisfies only, NOT source: a [patch]'d dep keeps its
# original registry source in the dependency record while the resolved package
# is git, so a source-kind filter would miss every patched crate (all the
# midnight-ledger ones) and leave their serde/fixed-point features off.
for e in entries.values():
    cands = [p for p in meta["packages"] if p["name"] == e["name"]]
    matched = [p for p in cands if satisfies(p["version"], e["req"])] or cands
    for p in matched:
        e["features"].update(resolved_features.get(p["id"], set()))

# All versions of each crate present in the workspace-resolved graph.
graph_versions = defaultdict(set)
for p in meta["packages"]:
    graph_versions[p["name"]].add(p["version"])

bad = {kt: s for kt, s in conflicts.items() if len(s) > 1}
for (key, target), variants in sorted(bad.items()):
    # resolved version per variant: highest graph version satisfying its req
    resolved = {}
    for src, req in variants:
        e = entries[(key, src, req, target)]
        cands = [v for v in graph_versions[e["name"]] if satisfies(v, req)]
        resolved[(src, req)] = max(cands, key=vparse) if cands else f"?{req}"
    groups = defaultdict(list)
    for sr, ver in resolved.items():
        groups[ver].append(sr)
    if len(groups) == 1:
        # cargo unifies these: merge into one entry under the tightest req
        ordered = sorted(variants, key=lambda sr: vparse(re.sub(r"[^\d.]", "", sr[1]) or "0"), reverse=True)
        keep = entries[(key,) + ordered[0] + (target,)]
        for sr in ordered[1:]:
            e = entries.pop((key,) + sr + (target,))
            keep["features"].update(e["features"])
            keep["default"] |= e["default"]
            keep.setdefault("aliases", []).append((e["name"], e["req"], e["source"]))
        continue
    print(f"multi-version dep {key}: {sorted(groups)}", file=sys.stderr)
    ordered_vers = sorted(groups, key=vparse, reverse=True)
    for ver in ordered_vers[1:]:
        maj_min = vparse(ver)
        suffix = f"{maj_min[0]}-{maj_min[1]}" if maj_min[0] == 0 else str(maj_min[0])
        for sr in groups[ver]:
            e = entries[(key,) + sr + (target,)]
            e["key"] = f"{key}-{suffix}"
            print(f"  {sr[1]} -> {e['key']}", file=sys.stderr)
    # merge within each version group too
    for ver, srs in groups.items():
        if len(srs) > 1:
            keep = entries[(key,) + srs[0] + (target,)]
            for sr in srs[1:]:
                e = entries.pop((key,) + sr + (target,))
                keep["features"].update(e["features"])
                keep["default"] |= e["default"]
                keep.setdefault("aliases", []).append((e["name"], e["req"], e["source"]))

# One package may be declared under several renames across members
# (e.g. prometheus-endpoint AND substrate-prometheus-endpoint for
# substrate-prometheus-endpoint). Cargo forbids depending on one crate twice
# under different names in a single manifest — merge to one key, preferring
# the unrenamed one; keymap aliases keep gen-first-party.py routing right.
by_ident = defaultdict(list)
for k, e in list(entries.items()):
    by_ident[(e["name"], e["source"], e["req"], e["target"])].append(k)
for ident, ks in by_ident.items():
    if len(ks) < 2:
        continue
    ks.sort(key=lambda k: (entries[k]["key"] != entries[k]["name"], entries[k]["key"]))
    keep = entries[ks[0]]
    print(f"rename collision for {ident[0]}: keeping key {keep['key']}, "
          f"dropping {[entries[k]['key'] for k in ks[1:]]}", file=sys.stderr)
    for k in ks[1:]:
        e = entries.pop(k)
        keep["features"].update(e["features"])
        keep["default"] |= e["default"]
        keep.setdefault("aliases", []).extend(
            [(e["name"], e["req"], e["source"])] + e.get("aliases", []))

# Sidecar: (name, req, source) -> manifest key, for gen-first-party.py
keymap = {}
for e in entries.values():
    keymap[f"{e['name']}|{e['req']}|{e['source']}"] = e["key"]
    for name, req, src in e.get("aliases", []):
        keymap[f"{name}|{req}|{src}"] = e["key"]

def toml_str(s):
    return '"' + s.replace('\\', '\\\\').replace('"', '\\"') + '"'

def fmt_entry(e):
    parts = []
    src = e["source"]
    if e["key"] != e["name"]:
        parts.append(f'package = {toml_str(e["name"])}')
    if src.startswith("git+"):
        url = src[4:]
        u = urllib.parse.urlparse(url)
        q = urllib.parse.parse_qs(u.query)
        base = u._replace(query="", fragment="").geturl()
        parts.append(f'git = {toml_str(base)}')
        for kind in ("tag", "branch", "rev"):
            if kind in q:
                parts.append(f'{kind} = {toml_str(q[kind][0])}')
    else:
        req = e["req"]
        parts.append(f'version = {toml_str(req)}')
    feats = sorted(f for f in e["features"] if f)
    if feats:
        parts.append("features = [" + ", ".join(toml_str(f) for f in feats) + "]")
    if not e["default"]:
        parts.append("default-features = false")
    return "{ " + ", ".join(parts) + " }"

plain = sorted((e for e in entries.values() if not e["target"]), key=lambda e: e["key"])
targeted = defaultdict(list)
for e in entries.values():
    if e["target"]:
        targeted[e["target"]].append(e)

with open(OUT, "w") as f:
    f.write("""# @generated by scripts/buck2/gen-third-party.py — direct external deps of all
# workspace members (normal + build; dev-deps omitted), features unioned.
# Regenerate: cargo metadata --format-version 1 | scripts/buck2/gen-third-party.py
[package]
name = "rust-third-party"
version = "0.0.0"
edition = "2024"
publish = false

# Standalone package — keep out of the repo's cargo workspace
[workspace]

# Dummy target to keep Cargo happy
[[bin]]
name = "top"
path = "top/main.rs"

[dependencies]
""")
    seen = set()
    for e in plain:
        if e["key"] in seen:
            # same key twice untargeted (source conflict) — emit first only
            continue
        seen.add(e["key"])
        f.write(f'{e["key"]} = {fmt_entry(e)}\n')
    for target in sorted(targeted):
        f.write(f"\n[target.{toml_str(target)}.dependencies]\n")
        tseen = set()
        for e in sorted(targeted[target], key=lambda e: e["key"]):
            if (target, e["key"]) in tseen:
                continue
            tseen.add((target, e["key"]))
            if e["key"] in seen:
                continue  # already unconditional with merged features
            f.write(f'{e["key"]} = {fmt_entry(e)}\n')

    # Propagate the workspace's [patch] sections — some members declare
    # registry versions that only resolve via these git redirects.
    # third-party/patches.toml adds buck2-specific overrides (local wins).
    patches = {k: dict(v) for k, v in root_manifest.get("patch", {}).items()}
    local_patches_path = os.path.join(os.path.dirname(OUT), "patches.toml")
    if os.path.exists(local_patches_path):
        for reg, crates in tomllib.load(open(local_patches_path, "rb")).items():
            patches.setdefault(reg, {}).update(crates)
    for registry, crates in patches.items():
        f.write(f"\n[patch.{toml_str(registry)}]\n")
        for name, spec in sorted(crates.items()):
            parts = []
            for k, v in spec.items():
                parts.append(f"{k} = {toml_str(v)}")
            f.write(f"{name} = {{ {', '.join(parts)} }}\n")

# Seed the lockfile from the workspace's — keeps versions identical to the
# cargo build and lets yanked-but-locked versions (e.g. core2 0.4.0) resolve.
lock_dst = os.path.join(os.path.dirname(OUT), "Cargo.lock")
if not os.path.exists(lock_dst):
    with open(os.path.join(meta["workspace_root"], "Cargo.lock")) as src, open(lock_dst, "w") as dst:
        dst.write(src.read())
    print(f"seeded {lock_dst} from workspace Cargo.lock")

with open(os.path.join(os.path.dirname(OUT), "keys.json"), "w") as kf:
    json.dump(dict(sorted(keymap.items())), kf, indent=1)

print(f"wrote {OUT}: {len(plain)} plain, {sum(len(v) for v in targeted.values())} targeted, {len(bad)} multi-version keys")
