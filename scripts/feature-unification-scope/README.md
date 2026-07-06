# feature-unification-scope

Computes the `cargo hack check --no-dev-deps` package selection for the
[Feature Unification](../../.github/workflows/feature-unification.yml) CI check:
the reverse-dependency closure of the crates a PR diff touches, so the (serial,
slow) check only runs over what could actually have regressed.

`scope.ts` is a single TypeScript file run directly via Node's native
type-stripping (Node >= 22.18) — there is no build step. See the header comment
in `scope.ts` for the full algorithm and the three output shapes.

## Layout

- `scope.ts` — the scoper. `computeScope()` is the pure decision (no I/O);
  `main()` is the thin shell that reads argv / `cargo metadata` / `Cargo.lock`.
- `scope.test.ts` — behaviour tests driving `computeScope()` end-to-end plus
  unit tests for the helpers.
- `package.json` / `package-lock.json` — the one runtime dep (`smol-toml`) and
  `@types/node` (dev-only, for editor/LSP type-checking).
- `tsconfig.json` — tuned so the LSP enforces exactly what Node's type-stripping
  allows.

## Develop

```sh
npm ci        # install smol-toml + @types/node
npm test      # node --test *.test.ts
```

## Run locally

From the repo root (git access is only needed to produce the three inputs):

```sh
mkdir -p .scope
git diff --name-only HEAD^1 HEAD   > .scope/changed.txt
git show HEAD^1:Cargo.lock         > .scope/base-lock.txt
git diff HEAD^1 HEAD -- Cargo.toml > .scope/toml-diff.txt
node scripts/feature-unification-scope/scope.ts \
    .scope/changed.txt .scope/base-lock.txt .scope/toml-diff.txt
```
