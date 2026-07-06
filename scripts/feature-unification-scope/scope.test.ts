// Behaviour tests for the feature-unification scoper.
//
// The pure decision lives in computeScope(); these drive it end-to-end with
// synthetic `cargo metadata` and Cargo.lock inputs and assert the exact
// cargo-hack selection string, plus unit tests for helper behaviour that
// isn't otherwise observable through computeScope's output.
//
// Run: `npm test` (i.e. `node --test *.test.ts`, Node >= 22.18).

import assert from "node:assert/strict";
import { describe, test } from "node:test";
import {
	computeScope,
	EXCLUDED,
	lockAffected,
	lockChanged,
	type Meta,
	owningCrate,
	parseLock,
	reverseReachMembers,
	type ScopeInput,
	WORKSPACE_ARGS,
} from "./scope.ts";

const ROOT = "/ws";
const REGISTRY = "registry+https://github.com/rust-lang/crates.io-index";

// A cargo-metadata-shaped object. `deps` are [name, kind] where kind is
// null (normal), "build", or "dev" -- matching `cargo metadata`'s output.
type DepSpec = [name: string, kind: string | null];
function mkMeta(crates: { name: string; dir: string; deps?: DepSpec[] }[]): Meta {
	return {
		workspace_root: ROOT,
		packages: crates.map((c) => ({
			name: c.name,
			manifest_path: `${ROOT}/${c.dir}/Cargo.toml`,
			dependencies: (c.deps ?? []).map(([name, kind]) => ({ name, kind })),
		})),
	};
}

// A Cargo.lock TOML fragment. Members omit source/checksum; registry crates set
// them. Renders dependencies one-per-line the way cargo does.
function mkLock(
	pkgs: { name: string; version?: string; source?: string; checksum?: string; deps?: string[] }[],
): string {
	return (
		pkgs
			.map((p) => {
				const lines = [`[[package]]`, `name = ${JSON.stringify(p.name)}`];
				if (p.version !== undefined) lines.push(`version = ${JSON.stringify(p.version)}`);
				if (p.source !== undefined) lines.push(`source = ${JSON.stringify(p.source)}`);
				if (p.checksum !== undefined) lines.push(`checksum = ${JSON.stringify(p.checksum)}`);
				if (p.deps?.length) {
					lines.push(`dependencies = [`);
					for (const d of p.deps) lines.push(` ${JSON.stringify(d)},`);
					lines.push(`]`);
				}
				return lines.join("\n");
			})
			.join("\n\n") + "\n"
	);
}

// A workspace: top -> mid -> leaf (normal edges), usesbuild -> buildtool (build
// edge), devdependent -> devonly (dev edge only), and a nested crate whose dir
// sits under nest-parent's dir.
const META = mkMeta([
	{ name: "leaf", dir: "crates/leaf" },
	{ name: "mid", dir: "crates/mid", deps: [["leaf", null]] },
	{ name: "top", dir: "crates/top", deps: [["mid", null]] },
	{ name: "buildtool", dir: "crates/buildtool" },
	{ name: "usesbuild", dir: "crates/usesbuild", deps: [["buildtool", "build"]] },
	{ name: "devonly", dir: "crates/devonly" },
	{ name: "devdependent", dir: "crates/devdependent", deps: [["devonly", "dev"]] },
	{ name: "nest-parent", dir: "pallets/x" },
	{ name: "nest-child", dir: "pallets/x/mock" },
]);

// Locks where `mid` pulls in the registry crate `external`; base and head differ
// only in external's resolved version/checksum.
const LOCK_HEAD = mkLock([
	{ name: "leaf" },
	{ name: "mid", deps: ["leaf", "external"] },
	{ name: "top", deps: ["mid"] },
	{ name: "external", version: "2.0.0", source: REGISTRY, checksum: "bbb" },
]);
const LOCK_BASE = mkLock([
	{ name: "leaf" },
	{ name: "mid", deps: ["leaf", "external"] },
	{ name: "top", deps: ["mid"] },
	{ name: "external", version: "1.0.0", source: REGISTRY, checksum: "aaa" },
]);

// computeScope with sensible defaults; `over` supplies the changed files and any
// lock/meta overrides for a given case.
function scope(over: Partial<ScopeInput> & { changed: string[] }): string {
	return computeScope({ baseLock: "", tomlDiff: "", headLock: "", meta: META, ...over });
}

describe("computeScope: file attribution", () => {
	test("no compile-relevant changes -> empty scope (skip the check)", () => {
		assert.equal(scope({ changed: [] }), "");
		// Only files the scoper explicitly ignores, including its own folder --
		// editing the scoper itself must never force a full check.
		assert.equal(
			scope({
				changed: [
					"changes/node/added/foo",
					".github/workflows/ci.yml",
					"docs/design.md",
					"README.md",
					"LICENSE",
					"Earthfile",
					"scripts/feature-unification-scope/scope.ts",
					"scripts/feature-unification-scope/scope.test.ts",
					"scripts/feature-unification-scope/package.json",
					"scripts/feature-unification-scope/package-lock.json",
					"scripts/feature-unification-scope/tsconfig.json",
				],
			}),
			"",
		);
	});

	test("a leaf crate's source -> that crate plus its reverse-dependency closure", () => {
		assert.equal(scope({ changed: ["crates/leaf/src/lib.rs"] }), "-p leaf -p mid -p top");
	});

	test("a terminal crate's source -> only itself (nothing depends on it)", () => {
		assert.equal(scope({ changed: ["crates/top/src/main.rs"] }), "-p top");
	});

	test("build-dependency edges are followed; dev-dependency edges are not", () => {
		assert.equal(scope({ changed: ["crates/buildtool/src/lib.rs"] }), "-p buildtool -p usesbuild");
		// devdependent depends on devonly only via a dev-dep, so it must not appear
		// (the check strips dev-deps).
		assert.equal(scope({ changed: ["crates/devonly/src/lib.rs"] }), "-p devonly");
	});

	test("nested crates win by longest prefix; multiple changed crates union their closures", () => {
		assert.equal(scope({ changed: ["pallets/x/mock/src/lib.rs"] }), "-p nest-child");
		assert.equal(scope({ changed: ["pallets/x/src/lib.rs"] }), "-p nest-parent");
		assert.equal(
			scope({ changed: ["crates/leaf/src/lib.rs", "crates/buildtool/src/lib.rs"] }),
			"-p buildtool -p leaf -p mid -p top -p usesbuild",
		);
	});
});

describe("computeScope: whole-workspace fallbacks", () => {
	test("global inputs (toolchain, .cargo/, .config/) force the whole workspace", () => {
		assert.equal(
			scope({ changed: ["rust-toolchain.toml", ".cargo/config.toml", ".config/nextest.toml"] }),
			WORKSPACE_ARGS,
		);
	});

	test("an unattributable file (under no crate, not ignored) forces the whole workspace", () => {
		assert.equal(scope({ changed: ["some/orphan/file.rs"] }), WORKSPACE_ARGS);
	});

	test("when the closure covers every non-excluded crate, collapse to --workspace", () => {
		const small = mkMeta([
			{ name: "a", dir: "crates/a" },
			{ name: "b", dir: "crates/b", deps: [["a", null]] },
			{ name: "partner-chains-demo-node", dir: "crates/pcn" },
			{ name: "partner-chains-demo-runtime", dir: "crates/pcr" },
		]);
		// touching a pulls in a and b == every non-excluded member.
		assert.equal(
			computeScope({ changed: ["crates/a/x.rs"], baseLock: "", tomlDiff: "", headLock: "", meta: small }),
			WORKSPACE_ARGS,
		);
	});
});

describe("computeScope: excluded crates", () => {
	test("excluded crates never appear in the selection", () => {
		const meta = mkMeta([
			{ name: "keep", dir: "crates/keep" },
			{ name: "partner-chains-demo-node", dir: "crates/pcn" },
			{ name: "partner-chains-demo-runtime", dir: "crates/pcr" },
		]);
		// changing an excluded crate that nothing else depends on -> empty scope.
		const out = computeScope({
			changed: ["crates/pcn/src/lib.rs"],
			baseLock: "",
			tomlDiff: "",
			headLock: "",
			meta,
		});
		assert.equal(out, "");
		for (const e of EXCLUDED) assert.doesNotMatch(out, new RegExp(`\\b${e}\\b`));
	});
});

describe("computeScope: lock and manifest diffs", () => {
	test("Cargo.lock changed with no base lock -> whole workspace (can't diff)", () => {
		assert.equal(
			scope({ changed: ["Cargo.lock"], baseLock: "", headLock: LOCK_HEAD }),
			WORKSPACE_ARGS,
		);
	});

	test("Cargo.lock changed -> members whose resolution depends on the changed crate", () => {
		// external bumped 1.0.0 -> 2.0.0; mid depends on it, top depends on mid.
		assert.equal(
			scope({ changed: ["Cargo.lock"], baseLock: LOCK_BASE, headLock: LOCK_HEAD }),
			"-p mid -p top",
		);
	});

	test("Cargo.lock listed but identical to base -> empty scope", () => {
		assert.equal(
			scope({ changed: ["Cargo.lock"], baseLock: LOCK_HEAD, headLock: LOCK_HEAD }),
			"",
		);
	});

	test("Cargo.toml dep-name seeds from the diff reach dependent members", () => {
		// A feature-only [workspace.dependencies] edit never touches the lock, so the
		// dep name is harvested from the manifest diff instead.
		assert.equal(
			scope({
				changed: ["Cargo.toml"],
				tomlDiff: ['+external = { version = "2.0" }', "-unrelated_line"].join("\n"),
				headLock: LOCK_HEAD,
			}),
			"-p mid -p top",
		);
	});

	test("lock scope unions with file-attributed scope", () => {
		assert.equal(
			scope({
				changed: ["Cargo.lock", "crates/buildtool/src/lib.rs"],
				baseLock: LOCK_BASE,
				headLock: LOCK_HEAD,
			}),
			"-p buildtool -p mid -p top -p usesbuild",
		);
	});
});

describe("owningCrate", () => {
	const crates = [
		{ name: "parent", dir: "pallets/x" },
		{ name: "child", dir: "pallets/x/mock" },
	];
	test("longest prefix wins; requires a directory boundary; unowned files are null", () => {
		assert.equal(owningCrate("pallets/x/mock/src/lib.rs", crates), "child");
		assert.equal(owningCrate("pallets/x/src/lib.rs", crates), "parent");
		// 'pallets/xyz' must not be attributed to the 'pallets/x' crate.
		assert.equal(owningCrate("pallets/xyz/src/lib.rs", crates), null);
		assert.equal(owningCrate("docs/readme.md", crates), null);
	});
});

describe("reverseReachMembers", () => {
	const lock = parseLock(LOCK_HEAD);
	const members = ["leaf", "mid", "top"];
	test("reverse-walks the lock graph, keeping only members; a member seed reappears alongside its dependents", () => {
		// external is not a member; its dependents mid and top are.
		assert.deepEqual(reverseReachMembers(lock, ["external"], members), ["mid", "top"]);
		assert.deepEqual(reverseReachMembers(lock, ["leaf"], members), ["leaf", "mid", "top"]);
	});
});

describe("lockChanged", () => {
	test("detects version, checksum, dependency-set, and membership changes; ignores identical locks and entry order", () => {
		const base = parseLock(mkLock([{ name: "x", version: "1.0.0", source: REGISTRY, checksum: "a" }]));
		assert.deepEqual(lockChanged(base, base), []);

		const versionBump = parseLock(mkLock([{ name: "x", version: "2.0.0", source: REGISTRY, checksum: "a" }]));
		assert.deepEqual(lockChanged(base, versionBump), ["x"]);

		const checksumChange = parseLock(mkLock([{ name: "x", version: "1.0.0", source: REGISTRY, checksum: "b" }]));
		assert.deepEqual(lockChanged(base, checksumChange), ["x"]);

		const depsBase = parseLock(mkLock([{ name: "x", version: "1", deps: ["a"] }]));
		const depsHead = parseLock(mkLock([{ name: "x", version: "1", deps: ["a", "c"] }]));
		assert.deepEqual(lockChanged(depsBase, depsHead), ["x"]);

		const added = parseLock(
			mkLock([
				{ name: "x", version: "1.0.0", source: REGISTRY, checksum: "a" },
				{ name: "y", version: "1.0.0", source: REGISTRY, checksum: "c" },
			]),
		);
		assert.deepEqual(lockChanged(base, added), ["y"]);

		// Same two entries for a multi-version package, emitted in the opposite
		// order -> not a change (fingerprint must be order-independent).
		const multiA = parseLock(
			mkLock([
				{ name: "x", version: "1.0.0", source: REGISTRY, checksum: "a" },
				{ name: "x", version: "2.0.0", source: REGISTRY, checksum: "b" },
			]),
		);
		const multiB = parseLock(
			mkLock([
				{ name: "x", version: "2.0.0", source: REGISTRY, checksum: "b" },
				{ name: "x", version: "1.0.0", source: REGISTRY, checksum: "a" },
			]),
		);
		assert.deepEqual(lockChanged(multiA, multiB), []);
	});
});

describe("parseLock", () => {
	test("parses fields, strips the version/source suffix from dep entries, and defaults path members", () => {
		const [pkg] = parseLock(
			mkLock([
				{
					name: "foo",
					version: "1.2.3",
					source: REGISTRY,
					checksum: "deadbeef",
					// cargo writes disambiguated deps as "name version (source)".
					deps: ["bar", "baz 2.0.0 (registry+https://example)"],
				},
			]),
		);
		assert.deepEqual(pkg, {
			name: "foo",
			version: "1.2.3",
			source: REGISTRY,
			checksum: "deadbeef",
			deps: ["bar", "baz"],
		});

		const [member] = parseLock(mkLock([{ name: "member", version: "0.1.0" }]));
		assert.deepEqual(member, { name: "member", version: "0.1.0", source: "", checksum: "", deps: [] });
	});
});

describe("lockAffected", () => {
	const members = ["leaf", "mid", "top"];
	test("returns null with no base lock to diff; otherwise combines lock-diff and manifest dep-name seeds", () => {
		assert.equal(lockAffected(["Cargo.lock"], members, "", "", LOCK_HEAD), null);
		assert.deepEqual(
			lockAffected(["Cargo.lock", "Cargo.toml"], members, LOCK_BASE, "+external = { workspace = true }", LOCK_HEAD),
			["mid", "top"],
		);
	});
});
