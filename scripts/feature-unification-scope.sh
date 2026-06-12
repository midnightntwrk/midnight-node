#!/usr/bin/env bash
# Compute the cargo-hack package scope for the feature-unification check.
#
# Reads changed file paths (one per line) on stdin and prints the package
# selection args for `cargo hack check --no-dev-deps`:
#
#   * "-p a -p b ..."              only these crates and their reverse-dependency
#                                  closure need re-checking
#   * "--workspace --exclude ..."  every crate is affected (computed, or a
#                                  global input like rust-toolchain changed)
#   * "" (empty)                   nothing compile-relevant changed, skip the check
#
# Scope is the union of two computed sets -- there is no blanket "manifest
# changed, check everything" path:
#
#   1. File attribution: each changed file maps to the crate whose directory
#      contains it; take the reverse-dependency closure over workspace
#      normal/build edges. Dev-dependency edges are ignored on purpose -- the
#      check strips dev-deps, so a change can never reach a dependent
#      through one.
#   2. Lock diff: if Cargo.lock changed, parse the base and head lock files,
#      fingerprint every package by (version, source, checksum, deps), and
#      reverse-walk the lock graph from the changed packages to the workspace
#      members whose resolution they participate in. If the root Cargo.toml
#      changed, dependency names harvested from its diff hunks are added as
#      seeds (this catches feature-only [workspace.dependencies] edits, which
#      never show up in the lock).
#
# Base for diffs is HEAD^1 (the PR base on a merge-commit checkout); override
# with SCOPE_BASE_REF for local experiments.
#
# Self-test: scripts/feature-unification-scope.sh --self-test
set -euo pipefail

BASE_REF="${SCOPE_BASE_REF:-HEAD^1}"
WORKSPACE_ARGS="--workspace --exclude partner-chains-demo-node --exclude partner-chains-demo-runtime"

# Cargo.lock -> JSON array of {name, version, source, checksum, deps:[names]}.
# The lock format is line-regular; names are crates-io-safe ([a-zA-Z0-9_-]),
# so naive JSON assembly is sound.
lock_to_json() {
	awk '
	function emit() {
		if (name == "") return
		printf "{\"name\":\"%s\",\"version\":\"%s\",\"source\":\"%s\",\"checksum\":\"%s\",\"deps\":[%s]}\n",
			name, ver, src, cks, deps
	}
	/^\[\[package\]\]/ { emit(); name=ver=src=cks=deps=""; indeps=0 }
	/^name = /     { gsub(/"/, ""); name=$3 }
	/^version = /  { gsub(/"/, ""); ver=$3 }
	/^source = /   { gsub(/"/, ""); src=$3 }
	/^checksum = / { gsub(/"/, ""); cks=$3 }
	/^dependencies = \[/ { indeps=1; next }
	indeps && /^\]/ { indeps=0 }
	indeps {
		gsub(/[",]/, "")
		# entries are "name" or "name version (source)"; keep the name
		if (deps != "") deps = deps ","
		deps = deps "\"" $1 "\""
	}
	END { emit() }
	' | jq -s '.'
}

# Names whose locked fingerprint differs between two lock JSONs. The locks
# arrive via --slurpfile (hence [0]): a full lock as JSON is megabytes and
# would overflow ARG_MAX if passed with --argjson.
# shellcheck disable=SC2016  # single quotes are deliberate: this is jq, not shell
JQ_LOCK_CHANGED='
def fp: group_by(.name)
        | map({key: .[0].name, value: (map({version, source, checksum, deps}) | sort)})
        | from_entries;
($old[0] | fp) as $o | ($new[0] | fp) as $n
| [($o | keys[]), ($n | keys[])] | unique
| map(select($o[.] != $n[.]))
'

# Workspace members ($members) reverse-reachable from $seeds in the lock
# graph ($lock via --slurpfile, see above).
# shellcheck disable=SC2016
JQ_AFFECTED='
(reduce $lock[0][] as $p ({};
   reduce ($p.deps[]?) as $d (.; .[$d] = ((.[$d] // []) + [$p.name]))))
  as $radj
| {cur: ($seeds | unique), grew: ($seeds | length > 0)}
| until(.grew | not;
    . as $s
    | ([.cur[] | $radj[.] // []] | add // [] | unique
       | map(select(. as $x | $s.cur | index($x) | not))) as $more
    | {cur: ((.cur + $more) | sort), grew: ($more | length > 0)})
| .cur as $reached
| [$members[] | select(. as $m | $reached | index($m))] | sort
'

# File attribution + workspace reverse closure. stdin: `cargo metadata
# --no-deps` JSON. $changed: paths. $extra: member names seeded by the lock
# diff. Output: the final package-selection arg string.
# shellcheck disable=SC2016
JQ_ATTRIB='
# ── helpers ──────────────────────────────────────────────────────────────
def matches_any($regexes): . as $f | any($regexes[]; . as $re | $f | test($re));

# The crate whose directory contains the file; longest prefix wins, so
# nested crates (e.g. pallets/x/mock) beat their parent. Null if unowned.
def owning_crate($pkgs):
  . as $f
  | [$pkgs[] | select((.dir + "/") as $pre | $f | startswith($pre))]
  | max_by(.dir | length)
  | .name;

# Grow the input seed array with every package that transitively depends on
# it, walking $deps (name -> [dep names]) in reverse until a fixed point.
def reverse_closure($deps; $names):
  {cur: ., grew: (length > 0)}
  | until(.grew | not;
      . as $s
      | ([$names[] | select(. as $n
           | ($s.cur | index($n) | not)
           and ($deps[$n] | any(. as $d | $s.cur | index($d))))]) as $more
      | {cur: ((.cur + $more) | sort), grew: ($more | length > 0)})
  | .cur;

# ── policy tables ────────────────────────────────────────────────────────
"--workspace --exclude partner-chains-demo-node --exclude partner-chains-demo-runtime"
  as $workspace_args
| ["partner-chains-demo-node", "partner-chains-demo-runtime"] as $excluded
# Global inputs with no diffable crate mapping: toolchain and cargo config.
| ["^\\.cargo/", "^\\.config/", "^rust-toolchain"] as $global
# Handled out-of-band by the lock/manifest diff in the wrapper script.
| ["^Cargo\\.toml$", "^Cargo\\.lock$"] as $handled
# Never compile-relevant (only consulted for files outside every crate dir).
# Earthfile and this scoper are deliberately here: build-recipe or scoper
# edits should not force a full workspace re-check.
| ["^changes/", "^\\.changes_archive/", "^\\.github/", "\\.md$", "^LICENSE",
   "^Earthfile$", "^scripts/feature-unification-scope\\.sh$"] as $ignore

# ── workspace shape, from cargo metadata on stdin ────────────────────────
| .workspace_root as $root
| [.packages[] | {name, dir: (.manifest_path | rtrimstr("/Cargo.toml") | ltrimstr($root + "/"))}]
  as $pkgs
| ($pkgs | map(.name)) as $names
# name -> [workspace deps], normal/build kinds only (see header for why not dev)
| (reduce .packages[] as $p ({};
     .[$p.name] = [$p.dependencies[]
                   | select((.kind == null or .kind == "build") and (.name as $n | $names | index($n)))
                   | .name]))
  as $deps

# ── 1. sort the changed files into owned / ignored / unattributable ──────
| ([$changed[] | select(matches_any($handled) | not)]) as $files
| ([$files[] | owning_crate($pkgs) | select(. != null)]) as $touched
| ([$files[] | select((owning_crate($pkgs) == null) and (matches_any($ignore) | not))])
  as $unattributable

# ── 2. decide the scope ──────────────────────────────────────────────────
| if any($changed[]; matches_any($global)) or ($unattributable != []) then
    $workspace_args
  else
    ((($touched + $extra) | unique) | reverse_closure($deps; $names)) - $excluded
    | if . == [] then ""
      elif . == (($names - $excluded) | sort) then $workspace_args
      else [.[] | "-p " + .] | join(" ")
      end
  end
'

scope() { # $1: changed paths JSON array; $2: extra member seeds JSON array; metadata on stdin
	jq -r --argjson changed "$1" --argjson extra "$2" "$JQ_ATTRIB"
}

# Member names whose resolution changed between the base and head lock files.
# $1: changed paths JSON array; $2: members JSON array.
lock_affected() {
	local seeds="[]" toml_seeds tmpd
	tmpd=$(mktemp -d)
	# shellcheck disable=SC2064  # expand $tmpd now, not at trap time
	trap "rm -rf '$tmpd'" RETURN
	lock_to_json <Cargo.lock >"$tmpd/new.json"
	if jq -e 'index("Cargo.lock")' >/dev/null <<<"$1"; then
		if ! git cat-file -e "$BASE_REF:Cargo.lock" 2>/dev/null; then
			return 1 # no base lock to diff against: caller falls back to full
		fi
		git show "$BASE_REF:Cargo.lock" | lock_to_json >"$tmpd/old.json"
		seeds=$(jq -n --slurpfile old "$tmpd/old.json" \
			--slurpfile new "$tmpd/new.json" "$JQ_LOCK_CHANGED")
	fi
	if jq -e 'index("Cargo.toml")' >/dev/null <<<"$1"; then
		# Dep names from changed lines of the root manifest; tokens that are
		# not package names simply match nothing in the lock graph.
		toml_seeds=$(git diff "$BASE_REF" HEAD -- Cargo.toml 2>/dev/null |
			sed -n 's/^[+-][[:space:]]*\([a-zA-Z0-9_-]\{1,\}\)[[:space:]]*=.*/\1/p' |
			jq -R -s 'split("\n") | map(select(length > 0)) | unique')
		seeds=$(jq -n --argjson a "$seeds" --argjson b "${toml_seeds:-[]}" '$a + $b | unique')
	fi
	jq -n --slurpfile lock "$tmpd/new.json" \
		--argjson members "$2" --argjson seeds "$seeds" "$JQ_AFFECTED"
}

self_test() {
	local meta ws got n=0
	ws="$WORKSPACE_ARGS"
	# base <- mid <- leaf; dev-user dev-depends on base; loner is isolated;
	# demo crates are the excluded ones.
	meta='{
	  "workspace_root": "/ws",
	  "packages": [
	    {"name": "base", "manifest_path": "/ws/base/Cargo.toml", "dependencies": []},
	    {"name": "mid", "manifest_path": "/ws/mid/Cargo.toml",
	     "dependencies": [{"name": "base", "kind": null}]},
	    {"name": "leaf", "manifest_path": "/ws/leaf/Cargo.toml",
	     "dependencies": [{"name": "mid", "kind": null}]},
	    {"name": "loner", "manifest_path": "/ws/loner/Cargo.toml", "dependencies": []},
	    {"name": "dev-user", "manifest_path": "/ws/dev-user/Cargo.toml",
	     "dependencies": [{"name": "base", "kind": "dev"}]},
	    {"name": "partner-chains-demo-node", "manifest_path": "/ws/demo/node/Cargo.toml",
	     "dependencies": [{"name": "base", "kind": null}]},
	    {"name": "partner-chains-demo-runtime", "manifest_path": "/ws/demo/runtime/Cargo.toml",
	     "dependencies": []}
	  ]
	}'
	run_case() { # $1: changed files (space-sep), $2: extra seeds JSON, $3: expected
		local files_json
		# shellcheck disable=SC2086  # splitting $1 into one path per line is the point
		files_json=$(printf '%s\n' $1 | jq -R -s 'split("\n") | map(select(length > 0))')
		got=$(echo "$meta" | scope "$files_json" "$2")
		if [[ "$got" != "$3" ]]; then
			echo "self-test FAIL for [$1|$2]: expected '$3', got '$got'" >&2
			exit 1
		fi
		n=$((n + 1))
	}
	# change in base ripples up through mid to leaf; not to the dev-only
	# user, the loner, or the excluded demo crate
	run_case "base/src/lib.rs" "[]" "-p base -p leaf -p mid"
	run_case "leaf/src/lib.rs" "[]" "-p leaf"
	# crate manifest change scopes like a source change
	run_case "mid/Cargo.toml" "[]" "-p leaf -p mid"
	# global inputs with no crate mapping force a full run
	run_case "rust-toolchain.toml" "[]" "$ws"
	run_case "base/src/lib.rs .cargo/config.toml" "[]" "$ws"
	# ignorable and demo-only changes mean nothing to check
	run_case "changes/node/added/x README.md .github/workflows/other.yml" "[]" ""
	run_case "Earthfile scripts/feature-unification-scope.sh" "[]" ""
	run_case "demo/node/src/main.rs" "[]" ""
	run_case "" "[]" ""
	# unattributable file: play safe
	run_case "mystery.bin" "[]" "$ws"
	# whole-workspace closure collapses to the --workspace form
	run_case "base/src/lib.rs loner/src/lib.rs dev-user/src/lib.rs" "[]" "$ws"
	# lock-diff seeds merge with file attribution (root manifests themselves
	# are handled out-of-band, hence no fallback here)
	run_case "Cargo.lock" '["mid"]' "-p leaf -p mid"
	run_case "Cargo.toml Cargo.lock" "[]" ""
	run_case "leaf/src/lib.rs Cargo.lock" '["loner"]' "-p leaf -p loner"

	# lock fingerprint diff: version bump of extdep, member mid's deps change
	local old new changed affected
	old='[{"name":"extdep","version":"1.0.0","source":"reg","checksum":"a","deps":[]},
	      {"name":"mid","version":"0.1.0","source":"","checksum":"","deps":["extdep"]},
	      {"name":"leaf","version":"0.1.0","source":"","checksum":"","deps":["mid"]},
	      {"name":"loner","version":"0.1.0","source":"","checksum":"","deps":[]}]'
	new=${old/1.0.0/1.1.0}
	tmpd=$(mktemp -d)
	echo "$old" >"$tmpd/old.json"; echo "$new" >"$tmpd/new.json"
	changed=$(jq -n --slurpfile old "$tmpd/old.json" --slurpfile new "$tmpd/new.json" "$JQ_LOCK_CHANGED")
	[[ "$(jq -c . <<<"$changed")" == '["extdep"]' ]] ||
		{ echo "self-test FAIL: lock diff expected [\"extdep\"], got $changed" >&2; exit 1; }
	n=$((n + 1))
	# reverse reach: extdep -> mid -> leaf, but never loner
	affected=$(jq -n --slurpfile lock "$tmpd/new.json" --argjson members '["mid","leaf","loner"]' \
		--argjson seeds "$changed" "$JQ_AFFECTED")
	[[ "$(jq -c . <<<"$affected")" == '["leaf","mid"]' ]] ||
		{ echo "self-test FAIL: affected expected [\"leaf\",\"mid\"], got $affected" >&2; exit 1; }
	n=$((n + 1))
	# identical locks: nothing changed
	changed=$(jq -n --slurpfile old "$tmpd/old.json" --slurpfile new "$tmpd/old.json" "$JQ_LOCK_CHANGED")
	rm -rf "$tmpd"
	[[ "$(jq -c . <<<"$changed")" == '[]' ]] ||
		{ echo "self-test FAIL: identical locks expected [], got $changed" >&2; exit 1; }
	n=$((n + 1))
	echo "self-test OK ($n cases)" >&2
}

if [[ "${1:-}" == "--self-test" ]]; then
	self_test
	exit 0
fi

CHANGED_JSON=$(jq -R -s 'split("\n") | map(select(length > 0))')
METADATA=$(cargo metadata --no-deps --format-version 1)
MEMBERS=$(jq '[.packages[].name]' <<<"$METADATA")

EXTRA="[]"
if jq -e 'index("Cargo.lock") or index("Cargo.toml")' >/dev/null <<<"$CHANGED_JSON"; then
	if ! EXTRA=$(lock_affected "$CHANGED_JSON" "$MEMBERS"); then
		# No base lock to diff against (e.g. shallow clone surprise): the one
		# remaining conservative fallback.
		echo "$WORKSPACE_ARGS"
		exit 0
	fi
fi

scope "$CHANGED_JSON" "$EXTRA" <<<"$METADATA"
