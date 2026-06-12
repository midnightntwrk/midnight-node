#!/usr/bin/env bash
# Compute the cargo-hack package scope for the feature-unification check.
#
# Reads changed file paths (one per line) on stdin and prints the package
# selection args for `cargo hack check --no-dev-deps`:
#
#   * "--workspace --exclude ..."  a global build input changed, check everything
#   * "-p a -p b ..."              only these crates and their reverse-dependency
#                                  closure need re-checking
#   * "" (empty)                   nothing compile-relevant changed, skip the check
#
# Why the reverse closure is sufficient: the check resolves features
# per-package, so a change in crate X can only alter the outcome for X itself
# or for crates whose dependency graph contains X. Dev-dependency edges are
# ignored on purpose -- the check strips dev-deps, so X can never enter a
# dependent's checked graph through one.
#
# Files inside a crate directory attribute to that crate (even data/markdown:
# they may be include_str!'d). Files that belong to no crate and match no
# IGNORE pattern force a full --workspace run.
#
# Self-test: scripts/feature-unification-scope.sh --self-test
set -euo pipefail

# jq program. stdin: `cargo metadata --no-deps` JSON. $changed: array of paths.
# shellcheck disable=SC2016  # single quotes are deliberate: this is jq, not shell
JQ_PROG='
"--workspace --exclude partner-chains-demo-node --exclude partner-chains-demo-runtime"
  as $workspace_args
| ["partner-chains-demo-node", "partner-chains-demo-runtime"] as $excluded
# A change to any of these invalidates every crate'\''s resolution.
| ["^Cargo\\.toml$", "^Cargo\\.lock$", "^\\.cargo/", "^\\.config/",
   "^rust-toolchain", "^Earthfile$", "^scripts/feature-unification-scope\\.sh$",
   "^\\.github/workflows/feature-unification\\.yml$"] as $global
# Never compile-relevant (only consulted for files outside every crate dir).
| ["^changes/", "^\\.changes_archive/", "^\\.github/", "\\.md$", "^LICENSE"] as $ignore
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
| if any($changed[]; . as $f | any($global[]; . as $re | $f | test($re))) then
    $workspace_args
  else
    # Attribute each file to the crate with the longest matching dir prefix,
    # so nested crates (e.g. pallets/x/mock) win over their parent.
    ([$changed[] | . as $f
      | ([$pkgs[] | select((.dir + "/") as $pre | $f | startswith($pre))] | max_by(.dir | length))
      | if . != null then {pkg: .name}
        elif any($ignore[]; . as $re | $f | test($re)) then empty
        else {unattributable: $f}
        end])
      as $hits
    | if any($hits[]; .unattributable) then $workspace_args
      else
        ([$hits[].pkg] | unique) as $seed
        # Reverse-dependency closure: grow until no new dependent is added.
        | {cur: $seed, grew: ($seed | length > 0)}
        | until(.grew | not;
            . as $s
            | ([$names[] | select(. as $n
                 | ($s.cur | index($n) | not)
                 and ($deps[$n] | any(. as $d | $s.cur | index($d))))])
              as $more
            | {cur: ((.cur + $more) | sort), grew: ($more | length > 0)})
        | (.cur - $excluded) as $closure
        | if $closure == [] then ""
          elif $closure == (($names - $excluded) | sort) then $workspace_args
          else [$closure[] | "-p " + .] | join(" ")
          end
      end
  end
'

scope() { # $1: changed paths as JSON array; metadata JSON on stdin
	jq -r --argjson changed "$1" "$JQ_PROG"
}

self_test() {
	local meta ws got n=0
	ws="--workspace --exclude partner-chains-demo-node --exclude partner-chains-demo-runtime"
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
	run_case() { # $1: changed files (space-sep), $2: expected output
		local files_json
		# shellcheck disable=SC2086  # splitting $1 into one path per line is the point
		files_json=$(printf '%s\n' $1 | jq -R -s 'split("\n") | map(select(length > 0))')
		got=$(echo "$meta" | scope "$files_json")
		if [[ "$got" != "$2" ]]; then
			echo "self-test FAIL for [$1]: expected '$2', got '$got'" >&2
			exit 1
		fi
		n=$((n + 1))
	}
	# change in base ripples up through mid to leaf; not to the dev-only
	# user, the loner, or the excluded demo crate
	run_case "base/src/lib.rs" "-p base -p leaf -p mid"
	run_case "leaf/src/lib.rs" "-p leaf"
	# crate manifest change scopes like a source change
	run_case "mid/Cargo.toml" "-p leaf -p mid"
	# global inputs force a full run
	run_case "Cargo.lock" "$ws"
	run_case "Cargo.toml" "$ws"
	run_case "base/src/lib.rs .cargo/config.toml" "$ws"
	# ignorable and demo-only changes mean nothing to check
	run_case "changes/node/added/x README.md .github/workflows/other.yml" ""
	run_case "demo/node/src/main.rs" ""
	run_case "" ""
	# unattributable file: play safe
	run_case "mystery.bin" "$ws"
	# whole-workspace closure collapses to the --workspace form
	run_case "base/src/lib.rs loner/src/lib.rs dev-user/src/lib.rs" "$ws"
	echo "self-test OK ($n cases)" >&2
}

if [[ "${1:-}" == "--self-test" ]]; then
	self_test
	exit 0
fi

CHANGED_JSON=$(jq -R -s 'split("\n") | map(select(length > 0))')
cargo metadata --no-deps --format-version 1 | scope "$CHANGED_JSON"
