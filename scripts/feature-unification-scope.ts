#!/usr/bin/env node
// This file is part of midnight-node.
// Copyright (C) Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// You may not use this file except in compliance with the License.
// You may obtain a copy of the License at
// http://www.apache.org/licenses/LICENSE-2.0
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

// Print the package selection for `cargo hack check --no-dev-deps`: "-p a -p b"
// (only these need re-checking), "--workspace --exclude ..." (everything
// affected), or "" (skip). Union of two sets:
//   1. changed file -> owning crate -> reverse-dependency closure over
//      workspace normal/build edges (dev edges can't reach past a
//      no-dev-deps check);
//   2. lock packages whose (version, source, checksum, deps) changed, plus dep
//      names from a root Cargo.toml diff (feature-only [workspace.dependencies]
//      edits never touch the lock), reverse-walked to the members using them.
//
// Usage: node feature-unification-scope.ts <changed-files> <base-lock> <toml-diff>
// (git-derived files in .scope/ -- see the Earthfile target). Node >= 22.18.

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
// Never compile-relevant (consulted only for files outside every crate dir);
// the Earthfile and the scoper's own files are deliberately here -- build-recipe
// or scoper edits can't change whether crates compile.
const IGNORE = [
	/^changes\//,
	/^\.changes_archive\//,
	/^\.github\//,
	/\.md$/,
	/^LICENSE/,
	/^Earthfile$/,
	/^\.gitignore$/,
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

// A lock dependency entry is "name" or "name version (source)" -- keep the name.
// source/checksum/dependencies are absent for path members, hence the defaults.
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

// name -> canonical fingerprint of its locked entries (a name may resolve to
// several versions); entries are sorted so the fingerprint is order-independent.
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

// Members whose lock resolution changed (base vs head), plus dep-name seeds
// from a root-manifest diff. Null = lock changed with no base to diff -> full.
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
		// Dep names from changed root-manifest lines; non-package tokens simply
		// match nothing in the lock graph.
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
	// name -> [workspace deps], normal/build kinds only (see header).
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
