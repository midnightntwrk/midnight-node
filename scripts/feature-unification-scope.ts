#!/usr/bin/env node
// Compute the cargo-hack package scope for the feature-unification check.
//
// Usage:
//   node feature-unification-scope.ts <changed-files> <base-lock> <toml-diff>
//
//   <changed-files>  file: PR-changed paths, one per line (git diff --name-only)
//   <base-lock>      file: the base commit's Cargo.lock (empty if none)
//   <toml-diff>      file: `git diff` of the root Cargo.toml (empty if untouched)
//
// All git access happens before the build (the CI workflow, or a few git
// commands locally) and lands in .scope/; this script is pure computation over
// those files plus `cargo metadata` and the head `Cargo.lock` (read from the
// current workspace, i.e. inside the check container). Prints the package
// selection args for `cargo hack check --no-dev-deps`:
//
//   * "-p a -p b ..."              only these crates and their reverse-dependency
//                                  closure need re-checking
//   * "--workspace --exclude ..."  every crate is affected (computed, or a
//                                  global input like rust-toolchain changed)
//   * "" (empty)                   nothing compile-relevant changed, skip the check
//
// Scope is the union of two computed sets -- there is no blanket "manifest
// changed, check everything" path:
//
//   1. File attribution: each changed file maps to the crate whose directory
//      contains it; take the reverse-dependency closure over workspace
//      normal/build edges. Dev-dependency edges are ignored on purpose -- the
//      check strips dev-deps, so a change can never reach a dependent through
//      one.
//   2. Lock diff: if Cargo.lock changed, fingerprint every package in the base
//      and head locks by (version, source, checksum, deps) and reverse-walk the
//      lock graph from the changed packages to the workspace members whose
//      resolution they participate in. If the root Cargo.toml changed, dep names
//      harvested from its diff hunks are added as seeds (this catches
//      feature-only [workspace.dependencies] edits, which never touch the lock).
//
// Runs on Node >= 22.18 (native TypeScript type stripping), no build step.
// Deps: scripts/package.json (smol-toml); `npm ci` in the check target. The CI
// image pins node, so the result is reproducible.

import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { parse as parseToml } from "smol-toml";

const EXCLUDED = ["partner-chains-demo-node", "partner-chains-demo-runtime"];
const WORKSPACE_ARGS = `--workspace ${EXCLUDED.map((e) => `--exclude ${e}`).join(" ")}`;
const MAX_BUFFER = 512 * 1024 * 1024; // cargo metadata can be MBs

// Global inputs with no diffable crate mapping: toolchain and cargo config.
const GLOBAL = [/^\.cargo\//, /^\.config\//, /^rust-toolchain/];
// Handled out-of-band by the lock/manifest diff below (root manifests only).
const HANDLED = [/^Cargo\.toml$/, /^Cargo\.lock$/];
// Never compile-relevant (only consulted for files outside every crate dir).
// Earthfile and this scoper are deliberately here: build-recipe or scoper
// edits should not force a full workspace re-check.
const IGNORE = [
	/^changes\//,
	/^\.changes_archive\//,
	/^\.github\//,
	/\.md$/,
	/^LICENSE/,
	/^Earthfile$/,
	/^\.gitignore$/,
	// the scoper's own files: changing them can't change whether crates compile
	/^scripts\/feature-unification-scope\.ts$/,
	/^scripts\/package(-lock)?\.json$/,
];

interface LockPkg {
	name: string;
	version: string;
	source: string;
	checksum: string;
	deps: string[];
}
interface MetaPkg {
	name: string;
	manifest_path: string;
	dependencies: { name: string; kind: string | null }[];
}
interface Meta {
	workspace_root: string;
	packages: MetaPkg[];
}
interface Crate {
	name: string;
	dir: string; // relative to the workspace root
}

function readOr(path: string | undefined, fallback = ""): string {
	if (!path) return fallback;
	try {
		return readFileSync(path, "utf8");
	} catch {
		return fallback;
	}
}

// Cargo.lock is TOML; parse it as such. A dependency entry is "name" or
// "name version (source)" -- keep the leading name. source/checksum/dependencies
// are absent for path/workspace members, hence the defaults.
function parseLock(text: string): LockPkg[] {
	const doc = parseToml(text) as { package?: Record<string, unknown>[] };
	return (doc.package ?? []).map((p) => ({
		name: String(p.name),
		version: String(p.version ?? ""),
		source: String(p.source ?? ""),
		checksum: String(p.checksum ?? ""),
		deps: ((p.dependencies as string[]) ?? []).map((d) => d.split(" ")[0]),
	}));
}

// name -> canonical fingerprint of all its locked entries (a name may resolve
// to several versions). deps stay in lock order inside an entry; the entries
// themselves are sorted so the fingerprint is order-independent.
function lockFingerprint(pkgs: LockPkg[]): Map<string, string> {
	const byName = new Map<string, string[]>();
	for (const p of pkgs) {
		const entry = JSON.stringify([p.version, p.source, p.checksum, p.deps]);
		(byName.get(p.name) ?? byName.set(p.name, []).get(p.name)!).push(entry);
	}
	const fp = new Map<string, string>();
	for (const [name, entries] of byName) fp.set(name, JSON.stringify(entries.sort()));
	return fp;
}

// Names whose locked fingerprint differs between two lock files.
function lockChanged(oldPkgs: LockPkg[], newPkgs: LockPkg[]): string[] {
	const o = lockFingerprint(oldPkgs);
	const n = lockFingerprint(newPkgs);
	const names = new Set([...o.keys(), ...n.keys()]);
	return [...names].filter((name) => o.get(name) !== n.get(name));
}

// Workspace members reverse-reachable from `seeds` in the lock graph.
function reverseReachMembers(lock: LockPkg[], seeds: string[], members: string[]): string[] {
	const radj = new Map<string, string[]>(); // dep -> [dependents]
	for (const p of lock)
		for (const d of p.deps) (radj.get(d) ?? radj.set(d, []).get(d)!).push(p.name);
	const cur = new Set(seeds);
	let grew = seeds.length > 0;
	while (grew) {
		grew = false;
		for (const x of [...cur])
			for (const r of radj.get(x) ?? []) if (!cur.has(r)) cur.add(r), (grew = true);
	}
	const memberSet = new Set(members);
	return [...cur].filter((m) => memberSet.has(m)).sort();
}

// Grow `seeds` with every member that transitively depends on one, walking
// `deps` (name -> [workspace dep names]) in reverse to a fixed point.
function reverseClosure(deps: Map<string, string[]>, names: string[], seeds: string[]): string[] {
	const cur = new Set(seeds);
	let grew = true;
	while (grew) {
		grew = false;
		for (const n of names) {
			if (cur.has(n)) continue;
			if ((deps.get(n) ?? []).some((d) => cur.has(d))) cur.add(n), (grew = true);
		}
	}
	return [...cur].sort();
}

// The crate whose directory contains `file`; longest prefix wins, so nested
// crates (e.g. pallets/x/mock) beat their parent. Null if unowned.
function owningCrate(file: string, crates: Crate[]): string | null {
	let best: Crate | null = null;
	for (const c of crates)
		if (file.startsWith(c.dir + "/") && (!best || c.dir.length > best.dir.length)) best = c;
	return best?.name ?? null;
}

// Member names whose resolution changed between the base and head lock files,
// plus dep-name seeds from a changed root manifest. Returns null when Cargo.lock
// changed but there is no base lock to diff against: caller falls back to full.
function lockAffected(
	changed: string[],
	members: string[],
	baseLock: string,
	tomlDiff: string,
): string[] | null {
	const headLock = parseLock(readFileSync("Cargo.lock", "utf8"));
	const seeds = new Set<string>();
	if (changed.includes("Cargo.lock")) {
		if (baseLock.trim().length === 0) return null;
		for (const c of lockChanged(parseLock(baseLock), headLock)) seeds.add(c);
	}
	if (changed.includes("Cargo.toml")) {
		// Dep names from changed lines of the root manifest; tokens that are not
		// package names simply match nothing in the lock graph.
		for (const line of tomlDiff.split("\n")) {
			const m = line.match(/^[+-]\s*([A-Za-z0-9_-]+)\s*=/);
			if (m) seeds.add(m[1]);
		}
	}
	return reverseReachMembers(headLock, [...seeds], members);
}

function main(): void {
	const [, , changedPath, baseLockPath, tomlDiffPath] = process.argv;
	const changed = readOr(changedPath)
		.split("\n")
		.filter((s) => s.length > 0);
	const baseLock = readOr(baseLockPath);
	const tomlDiff = readOr(tomlDiffPath);

	const meta: Meta = JSON.parse(
		execFileSync("cargo", ["metadata", "--no-deps", "--format-version", "1"], {
			encoding: "utf8",
			maxBuffer: MAX_BUFFER,
		}),
	);
	const root = meta.workspace_root;
	const crates: Crate[] = meta.packages.map((p) => {
		let dir = p.manifest_path.replace(/\/Cargo\.toml$/, "");
		if (dir.startsWith(root + "/")) dir = dir.slice(root.length + 1);
		return { name: p.name, dir };
	});
	const names = crates.map((c) => c.name);
	const nameSet = new Set(names);
	// name -> [workspace deps], normal/build kinds only (dev edges can't reach a
	// dependent through the no-dev-deps check; see header).
	const deps = new Map<string, string[]>();
	for (const p of meta.packages)
		deps.set(
			p.name,
			p.dependencies
				.filter((d) => (d.kind === null || d.kind === "build") && nameSet.has(d.name))
				.map((d) => d.name),
		);

	let extra: string[] = [];
	if (changed.includes("Cargo.lock") || changed.includes("Cargo.toml")) {
		const affected = lockAffected(changed, names, baseLock, tomlDiff);
		if (affected === null) {
			process.stdout.write(WORKSPACE_ARGS + "\n");
			return;
		}
		extra = affected;
	}

	// Sort the changed files into owned / ignored / unattributable.
	const files = changed.filter((f) => !HANDLED.some((re) => re.test(f)));
	const touched = files
		.map((f) => owningCrate(f, crates))
		.filter((x): x is string => x !== null);
	const unattributable = files.filter(
		(f) => owningCrate(f, crates) === null && !IGNORE.some((re) => re.test(f)),
	);

	let out: string;
	if (changed.some((f) => GLOBAL.some((re) => re.test(f))) || unattributable.length > 0) {
		out = WORKSPACE_ARGS;
	} else {
		const closure = reverseClosure(deps, names, [...new Set([...touched, ...extra])]).filter(
			(n) => !EXCLUDED.includes(n),
		);
		const allNonExcluded = names.filter((n) => !EXCLUDED.includes(n)).sort();
		if (closure.length === 0) out = "";
		else if (
			closure.length === allNonExcluded.length &&
			closure.every((c, i) => c === allNonExcluded[i])
		)
			out = WORKSPACE_ARGS;
		else out = closure.map((p) => "-p " + p).join(" ");
	}
	process.stdout.write(out + "\n");
}

main();
