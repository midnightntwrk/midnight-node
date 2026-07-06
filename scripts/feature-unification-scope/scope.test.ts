// Behaviour tests for the feature-unification scoper.
//
// The pure decision lives in computeScope(); these drive it end-to-end with
// synthetic `cargo metadata` and Cargo.lock inputs and assert the exact
// cargo-hack selection string, plus unit tests for the trickier helpers.
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
	reverseClosure,
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
	test("no changed files -> empty scope (skip the check)", () => {
		assert.equal(scope({ changed: [] }), "");
	});

	test("only ignored files -> empty scope", () => {
		assert.equal(
			scope({
				changed: [
					"changes/node/added/foo",
					".github/workflows/ci.yml",
					"docs/design.md",
					"README.md",
					"LICENSE",
					"Earthfile",
				],
			}),
			"",
		);
	});

	test("editing the scoper's own folder -> empty scope (never self-triggers a full check)", () => {
		assert.equal(
			scope({
				changed: [
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

	test("a leaf crate's source -> that crate plus its reverse-dep closure", () => {
		assert.equal(scope({ changed: ["crates/leaf/src/lib.rs"] }), "-p leaf -p mid -p top");
	});

	test("a top crate's source -> only itself (nothing depends on it)", () => {
		assert.equal(scope({ changed: ["crates/top/src/main.rs"] }), "-p top");
	});

	test("build-dependency edges are followed", () => {
		assert.equal(
			scope({ changed: ["crates/buildtool/src/lib.rs"] }),
			"-p buildtool -p usesbuild",
		);
	});

	test("dev-dependency edges are NOT followed (the check strips dev-deps)", () => {
		// devdependent depends on devonly only via a dev-dep, so it must not appear.
		assert.equal(scope({ changed: ["crates/devonly/src/lib.rs"] }), "-p devonly");
	});

	test("nested crate wins over its parent by longest-prefix", () => {
		assert.equal(scope({ changed: ["pallets/x/mock/src/lib.rs"] }), "-p nest-child");
		assert.equal(scope({ changed: ["pallets/x/src/lib.rs"] }), "-p nest-parent");
	});

	test("multiple changed crates union their closures", () => {
		assert.equal(
			scope({ changed: ["crates/leaf/src/lib.rs", "crates/buildtool/src/lib.rs"] }),
			"-p buildtool -p leaf -p mid -p top -p usesbuild",
		);
	});
});

describe("computeScope: whole-workspace fallbacks", () => {
	test("a global input (rust-toolchain) forces the whole workspace", () => {
		assert.equal(scope({ changed: ["rust-toolchain.toml"] }), WORKSPACE_ARGS);
	});

	test("a .cargo/ config change forces the whole workspace", () => {
		assert.equal(scope({ changed: [".cargo/config.toml"] }), WORKSPACE_ARGS);
	});

	test("a .config/ change forces the whole workspace", () => {
		assert.equal(scope({ changed: [".config/nextest.toml"] }), WORKSPACE_ARGS);
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
	test("longest matching prefix wins", () => {
		assert.equal(owningCrate("pallets/x/mock/src/lib.rs", crates), "child");
		assert.equal(owningCrate("pallets/x/src/lib.rs", crates), "parent");
	});
	test("requires a directory boundary, not just a string prefix", () => {
		// 'pallets/xyz' must not be attributed to the 'pallets/x' crate.
		assert.equal(owningCrate("pallets/xyz/src/lib.rs", crates), null);
	});
	test("files under no crate are unowned", () => {
		assert.equal(owningCrate("docs/readme.md", crates), null);
	});
});

describe("reverseClosure", () => {
	const deps = new Map<string, string[]>([
		["leaf", []],
		["mid", ["leaf"]],
		["top", ["mid"]],
	]);
	const names = ["leaf", "mid", "top"];
	test("walks reverse edges to a fixed point, sorted", () => {
		assert.deepEqual(reverseClosure(deps, names, ["leaf"]), ["leaf", "mid", "top"]);
		assert.deepEqual(reverseClosure(deps, names, ["mid"]), ["mid", "top"]);
		assert.deepEqual(reverseClosure(deps, names, ["top"]), ["top"]);
	});
	test("empty seeds -> empty result", () => {
		assert.deepEqual(reverseClosure(deps, names, []), []);
	});
});

describe("reverseReachMembers", () => {
	const lock = parseLock(LOCK_HEAD);
	const members = ["leaf", "mid", "top"];
	test("reverse-walks the lock graph and keeps only members", () => {
		// external is not a member; its dependents mid and top are.
		assert.deepEqual(reverseReachMembers(lock, ["external"], members), ["mid", "top"]);
	});
	test("a member seed reappears alongside its dependents", () => {
		assert.deepEqual(reverseReachMembers(lock, ["leaf"], members), ["leaf", "mid", "top"]);
	});
	test("no seeds -> nothing", () => {
		assert.deepEqual(reverseReachMembers(lock, [], members), []);
	});
});

describe("lockChanged", () => {
	const base = parseLock(mkLock([{ name: "x", version: "1.0.0", source: REGISTRY, checksum: "a" }]));
	test("identical locks -> nothing changed", () => {
		assert.deepEqual(lockChanged(base, base), []);
	});
	test("a version bump is detected", () => {
		const head = parseLock(mkLock([{ name: "x", version: "2.0.0", source: REGISTRY, checksum: "a" }]));
		assert.deepEqual(lockChanged(base, head), ["x"]);
	});
	test("a checksum change is detected", () => {
		const head = parseLock(mkLock([{ name: "x", version: "1.0.0", source: REGISTRY, checksum: "b" }]));
		assert.deepEqual(lockChanged(base, head), ["x"]);
	});
	test("a dependency-set change is detected", () => {
		const b = parseLock(mkLock([{ name: "x", version: "1", deps: ["a"] }]));
		const h = parseLock(mkLock([{ name: "x", version: "1", deps: ["a", "c"] }]));
		assert.deepEqual(lockChanged(b, h), ["x"]);
	});
	test("added and removed packages are detected", () => {
		const h = parseLock(
			mkLock([
				{ name: "x", version: "1.0.0", source: REGISTRY, checksum: "a" },
				{ name: "y", version: "1.0.0", source: REGISTRY, checksum: "c" },
			]),
		);
		assert.deepEqual(lockChanged(base, h), ["y"]);
	});
	test("multi-version packages fingerprint order-independently", () => {
		const b = parseLock(
			mkLock([
				{ name: "x", version: "1.0.0", source: REGISTRY, checksum: "a" },
				{ name: "x", version: "2.0.0", source: REGISTRY, checksum: "b" },
			]),
		);
		// same two entries, emitted in the opposite order -> not a change.
		const h = parseLock(
			mkLock([
				{ name: "x", version: "2.0.0", source: REGISTRY, checksum: "b" },
				{ name: "x", version: "1.0.0", source: REGISTRY, checksum: "a" },
			]),
		);
		assert.deepEqual(lockChanged(b, h), []);
	});
});

describe("parseLock", () => {
	test("parses fields and strips the version/source suffix from dep entries", () => {
		const pkgs = parseLock(
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
		assert.equal(pkgs.length, 1);
		assert.deepEqual(pkgs[0], {
			name: "foo",
			version: "1.2.3",
			source: REGISTRY,
			checksum: "deadbeef",
			deps: ["bar", "baz"],
		});
	});
	test("path/workspace members with no source/checksum/deps get empty defaults", () => {
		const [p] = parseLock(mkLock([{ name: "member", version: "0.1.0" }]));
		assert.deepEqual(p, { name: "member", version: "0.1.0", source: "", checksum: "", deps: [] });
	});
});

describe("lockAffected", () => {
	const members = ["leaf", "mid", "top"];
	test("returns null when the lock changed but there is no base to diff", () => {
		assert.equal(lockAffected(["Cargo.lock"], members, "", "", LOCK_HEAD), null);
	});
	test("combines lock-diff seeds and manifest dep-name seeds", () => {
		const out = lockAffected(
			["Cargo.lock", "Cargo.toml"],
			members,
			LOCK_BASE,
			"+external = { workspace = true }",
			LOCK_HEAD,
		);
		assert.deepEqual(out, ["mid", "top"]);
	});
});
