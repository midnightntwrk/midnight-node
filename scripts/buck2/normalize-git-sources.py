#!/usr/bin/env python3
"""Give every (git repo, commit) in a reindeer BUCK file its own git_fetch.

reindeer keys git_fetch on the repo URL, so a workspace that pins one repo at
several commits (midnight-ledger: one rc tag per crate) gets one fetch for all of
them and every crate builds against whichever commit that fetch pinned. It also
sets CARGO_MANIFEST_DIR to the repo root for a crate in a subdirectory, which
breaks proc-macro-crate (it reads $CARGO_MANIFEST_DIR/Cargo.toml at expansion).

Both are fixed upstream in facebookincubator/reindeer PRs 118 and 119; until they
land, do the same to the output: rename each git crate's fetch target from
<repo>-<urlhash> to <repo>-<urlhash><commit8>, point CARGO_MANIFEST_DIR at the
crate's subdirectory, and split each git_fetch into one per commit. Which
commit a rule wants comes from `cargo metadata` on the directory's Cargo.toml,
matched by package name + version. Delete this once the PRs land.

Usage: normalize-git-sources.py <third-party-dir>   (idempotent)
"""
import json, os, re, subprocess, sys

d = sys.argv[1] if len(sys.argv) > 1 else "third-party"
BUCK = os.path.join(d, "BUCK")

meta = json.loads(
    subprocess.run(
        [
            "cargo",
            "metadata",
            "--format-version",
            "1",
            "--locked",
            "--manifest-path",
            os.path.join(d, "Cargo.toml"),
        ],
        env={**os.environ, "CARGO_HOME": os.path.abspath(os.path.join(d, ".cargo"))},
        check=True,
        stdout=subprocess.PIPE,  # stderr streams: a cargo failure must explain itself
        text=True,
    ).stdout
)


def subdir_in_repo(manifest_path):
    # cargo checks the repo out under git/checkouts/<repo>-<hash>/<commit7>/,
    # the only ancestor holding a .git entry.
    crate_dir = cur = os.path.dirname(manifest_path)
    while not os.path.exists(os.path.join(cur, ".git")):
        parent = os.path.dirname(cur)
        if parent == cur:
            sys.exit(f"{manifest_path}: no git checkout root above it")
        cur = parent
    return os.path.relpath(crate_dir, cur)


# (name, version) -> (commit, subdir) for every git-sourced package.
git_pkgs = {}
for p in meta["packages"]:
    src = p["source"] or ""
    if src.startswith("git+"):
        commit = src.rsplit("#", 1)[1]
        git_pkgs[(p["name"], p["version"])] = (
            commit,
            subdir_in_repo(p["manifest_path"]),
        )


def package_for(rule_name):
    """Match `<pkg>-<ver-prefix>[-suffix]` the way reindeer shortens names."""
    best = None
    for (name, version), info in git_pkgs.items():
        rest = rule_name[len(name) + 1 :] if rule_name.startswith(name + "-") else None
        if rest is None:
            continue
        parts = version.split(".")
        for n in range(len(parts), 0, -1):
            v = ".".join(parts[:n])
            if rest == v or rest.startswith(v + "-"):
                key = (len(name), n)
                if best is None or key > best[0]:
                    best = (key, name, version, info)
                break
    if best is None:
        sys.exit(
            f"{BUCK}: rule {rule_name!r} uses a git source but matches no git package "
            f"in {d}/Cargo.toml's metadata (names: {sorted({n for n, _ in git_pkgs})})"
        )
    return best[1:]


NAME = re.compile(r'^\s*name = "([^"]+)",$', re.M)
MANIFEST_ENV = re.compile(r'("CARGO_MANIFEST_DIR": ")([^"]+)(")')

text = open(BUCK).read()
# Rule blocks: `<kind>(` at column 0 through the matching `)` at column 0.
blocks = re.split(r"(?m)^(?=[A-Za-z_][\w.]*\($)", text)

# Only stems of git_fetch targets still keyed by URL hash alone (16 hex); a
# split one carries the commit too (24 hex), so a rerun finds nothing to do.
stems = [
    NAME.search(b).group(1)[: -len(".git")]
    for b in blocks
    if b.startswith("git_fetch(")
]
stems = [s for s in stems if re.search(r"-[0-9a-f]{16}$", s)]
GIT_REF = (
    re.compile(r"\b(" + "|".join(map(re.escape, stems)) + r")(?=\.git\b|/|\")")
    if stems
    else None
)
if GIT_REF is None:
    print(f"normalize-git-sources: {d}: nothing to do")
    sys.exit(0)

fetch_subtargets = {}  # new target name -> set(sub_target paths)
fetch_commits = {}  # old fetch name -> {commit: new fetch name}
renamed = 0
out = []
for blk in blocks:
    if blk.startswith("git_fetch(") or not GIT_REF.search(blk):
        out.append(blk)
        continue
    rule = NAME.search(blk).group(1)
    name, version, (commit, subdir) = package_for(rule)
    old = GIT_REF.search(blk).group(1)
    new = f"{old}{commit[:8]}"
    blk = blk.replace(old, new)
    if subdir != ".":
        blk = MANIFEST_ENV.sub(
            lambda m: (
                m.group(1) + f"{new}/{subdir}" + m.group(3)
                if m.group(2) == new
                else m.group(0)
            ),
            blk,
        )
    for sub in re.findall(rf":{re.escape(new)}\.git\[([^\]]+)\]", blk):
        fetch_subtargets.setdefault(new, set()).add(sub)
    fetch_commits.setdefault(old, {})[commit] = new
    renamed += 1
    out.append(blk)

split = 0
final = []
for blk in out:
    m = NAME.search(blk) if blk.startswith("git_fetch(") else None
    old = m.group(1)[: -len(".git")] if m else None
    if old not in fetch_commits:
        final.append(blk)
        continue
    for commit, new in sorted(fetch_commits[old].items()):
        subs = "".join(
            f'        "{s}",\n' for s in sorted(fetch_subtargets.get(new, ()))
        )
        subs = (
            f"    sub_targets = [\n{subs}    ],\n"
            if subs
            else "    sub_targets = [],\n"
        )
        b = blk.replace(f'"{old}.git"', f'"{new}.git"')
        b = re.sub(r'    rev = "[0-9a-f]+",\n', f'    rev = "{commit}",\n', b)
        b = re.sub(
            r"    sub_targets = \[(?:\n(?:        [^\n]*\n)*    |)\],\n", subs, b
        )
        final.append(b)
        split += 1

if renamed:
    open(BUCK, "w").write("".join(final))
print(f"normalize-git-sources: {d}: {renamed} rule(s) rekeyed, {split} git_fetch(es)")
