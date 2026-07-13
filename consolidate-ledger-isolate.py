#!/usr/bin/env python3
"""Single-commit multi-crate "isolate" for the batch-verification ledger deps.

Builds ONE commit on top of the ledger `js/batch-verification` branch tip in which the
changed L9 crates are the only workspace members and their `[dependencies]` have `path=`
stripped (version-only, like the crates.io release tags). All the node's `[patch.crates-io]`
entries for these crates then share this one commit by `rev`; inter-crate deps among them
route registry -> patch -> same rev. See Batch-Verification-Notes.md "Ledger dependency wiring".

What it does, given target crate dirs:
  1. root Cargo.toml: filter `members` and `default-members` to the targets
  2. each target `<dir>/Cargo.toml`: strip `path="..."` from [dependencies], drop [dev-dependencies]
  3. commit; print the new commit SHA on stdout

Run it in a THROWAWAY detached worktree so the branch ref never moves, e.g.:

    LEDGER=/home/oscar/source/repos/midnight-ledger
    BR=js/batch-verification            # ledger branch to isolate from
    WT=$(mktemp -d)/iso
    git -C "$LEDGER" worktree add --detach "$WT" "origin/$BR"
    SHA=$(cd "$WT" && uv run --no-project python \
        /path/to/consolidate-ledger-isolate.py \
        ledger zswap transient-crypto onchain-state zkir zkir-v3)
    git -C "$LEDGER" worktree remove --force "$WT"
    echo "isolate commit = $SHA"
    # publish + wire up:
    git -C "$LEDGER" push -f origin "$SHA:refs/heads/js/batch-verification-isolated"
    #   then set rev = "$SHA" for the 6 crates in the node Cargo.toml [patch.crates-io]
    #   and `cargo update` those packages.

NOTE: the commit SHA is NOT reproducible (commit timestamps vary), so the rev changes on
every regeneration. `scripts/isolate.py` in the ledger repo is the per-crate/push-tags
alternative used for real crates.io releases.
"""
import pathlib, re, subprocess, sys

targets = sys.argv[1:]
if not targets:
    sys.exit("need target crate dirs, e.g. ledger zswap transient-crypto onchain-state zkir zkir-v3")
repo = pathlib.Path(subprocess.check_output(["git", "rev-parse", "--show-toplevel"], text=True).strip())
if subprocess.check_output(["git", "status", "--porcelain"], cwd=repo, text=True).strip():
    sys.exit("working tree dirty; run in a clean (detached) worktree")

root = repo / "Cargo.toml"
text = root.read_text()

def filter_list(text, key, names):
    pattern = re.compile(rf"(?ms)^({re.escape(key)}\s*=\s*\[)(.*?)(\])")
    quoted = [f'"{n}"' for n in names]
    def repl(m):
        head, body, tail = m.group(1), m.group(2), m.group(3)
        kept = [line for line in body.splitlines() if any(q in line for q in quoted)]
        if not kept:
            sys.exit(f"none of {names} found in {key}")
        return head + "\n" + "\n".join(kept) + "\n" + tail
    new, n = pattern.subn(repl, text)
    if n == 0:
        sys.exit(f"no [{key}] list in root Cargo.toml")
    return new

text = filter_list(text, "members", targets)
text = filter_list(text, "default-members", targets)
root.write_text(text)

for t in targets:
    sub = repo / t / "Cargo.toml"
    if not sub.is_file():
        sys.exit(f"no {sub}")
    section = None
    out = []
    for line in sub.read_text().splitlines(keepends=True):
        s = line.strip()
        if s.startswith("[") and s.endswith("]"):
            section = s
        if section == "[dev-dependencies]":
            continue
        if section == "[dependencies]":
            line = re.sub(r'path\s*=\s*"[^"]*"\s*,\s*', "", line)
            line = re.sub(r',\s*path\s*=\s*"[^"]*"', "", line)
        out.append(line)
    sub.write_text("".join(out))

subprocess.run(["git", "-C", str(repo), "add", "-A"], check=True)
subprocess.run(["git", "-C", str(repo), "commit", "-q", "-m",
                "isolate batch-verification crates (single commit, local)"], check=True)
print(subprocess.check_output(["git", "-C", str(repo), "rev-parse", "HEAD"], text=True).strip())
