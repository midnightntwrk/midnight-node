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

import { createRequire, registerHooks, type ResolveHookContext } from 'node:module';
import { dirname, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

/** A regular expression to match module resolution paths in error messages. */
const ERROR_MODULE_REGEXP = /module '(?<path>.*)'$/;

/** Currently supported `compactc` versions (in full `<major>.<minor>.<patch>` form). Each maps to a sibling
 * `compact-<major>.<minor>.<patch>/` workspace pinning the matched `@midnight-ntwrk/compact-js` line. */
export const SUPPORTED_COMPACTC_VERSIONS = ['0.29.0', '0.30.0', '0.31.0', '0.33.0'] as const;

/**
 * Normalizes a raw `COMPACTC_VERSION` to the supported `<major>.<minor>.<patch>` form used to select a
 * variant workspace, exiting the process with a helpful message if it is unset or unsupported.
 *
 * There is deliberately no default: the version is pinned by the root `COMPACTC_VERSION` file (which CI and
 * the dev shell's `.envrc` both export), so a missing value means a misconfigured environment rather than a
 * value we should guess — guessing would silently mismatch the `COMPACT_HOME` compiler.
 *
 * Dispatch is on the full `<major>.<minor>.<patch>` version: a `compactc` patch can ship a contract format
 * that expects a different `@midnight-ntwrk/compact-js` patch, so each supported patch has its own variant
 * workspace. The raw value may carry a trailing build/tree-hash suffix (e.g. `0.31.0-6587676a9bb2`, the form
 * stored in the root `COMPACTC_VERSION` file); only the leading `<major>.<minor>.<patch>` is matched.
 */
export const resolveCompactcVersion = (
  rawCompactcVersion: string | undefined = process.env.COMPACTC_VERSION
): string => {
  if (!rawCompactcVersion) {
    console.error(
      `COMPACTC_VERSION is not set (expected one of ${SUPPORTED_COMPACTC_VERSIONS.join(', ')}). ` +
      'The dev shell exports it from the root COMPACTC_VERSION file; set it explicitly to target another version.'
    );
    process.exit(1);
  }
  // Take the leading `<major>.<minor>.<patch>`, dropping any `-<suffix>` (e.g. the tree-hash in
  // `0.31.0-6587676a9bb2`) and any extra version components.
  const compactcVersion = rawCompactcVersion.split('-')[0].split('.').slice(0, 3).join('.');
  if (!(SUPPORTED_COMPACTC_VERSIONS as readonly string[]).includes(compactcVersion)) {
    console.error(
      `Unsupported COMPACTC_VERSION: ${rawCompactcVersion} (expected one of ${SUPPORTED_COMPACTC_VERSIONS.join(', ')})`
    );
    process.exit(1);
  }
  return compactcVersion;
};

/** The variant workspace package name for a given supported `<major>.<minor>.<patch>` version. */
export const toolkitPackageName = (compactcVersion: string): string =>
  `@midnight-ntwrk/node-toolkit-compact-${compactcVersion}`;

/**
 * Builds a resolver bound to the variant workspace pinned for `compactcVersion`: the variant package
 * name, a `require` rooted at that variant, and a `toolkitResolve` that resolves a specifier to an
 * absolute path against it.
 *
 * `toolkitResolve` carries a CJS→ESM fallback. The vendored variant copies are ESM-only, so a CJS
 * `require.resolve` of a subpath export (e.g. `…/effect`) first fails with MODULE_NOT_FOUND pointing at
 * the non-existent `dist/cjs/…` path; we rewrite that to `dist/esm/…` and re-resolve. This depends on the
 * exact MODULE_NOT_FOUND message format, but is the simplest way to support both layouts without building
 * a full resolver. Shared by {@link installCompactcResolver} and {@link resolveVariantModule}.
 */
const makeVariantResolver = (compactcVersion: string) => {
  const packageName = toolkitPackageName(compactcVersion);
  const require = createRequire(import.meta.url);
  const toolkitRequire = createRequire(require.resolve(packageName));
  const cjsPathSegment = `${sep}dist${sep}cjs${sep}`;
  const esmPathSegment = `${sep}dist${sep}esm${sep}`;

  // Specifiers currently being resolved by `toolkitResolve`. On Node 24 the CJS `require.resolve`
  // below is itself routed through `registerHooks`, so the hook installed by `installCompactcResolver`
  // re-enters for the same bare specifier; the hook consults this set to break that recursion. (Node 22's
  // CJS resolver ignored the hook, so the set is never hit there — the guard is a no-op on Node 22.)
  const resolving = new Set<string>();

  const toolkitResolve = (specifier: string): string => {
    resolving.add(specifier);
    try {
      return toolkitRequire.resolve(specifier);
    } catch (error: unknown) {
      if (error instanceof Error && 'code' in error && error.code === 'MODULE_NOT_FOUND') {
        const match = ERROR_MODULE_REGEXP.exec(error.message);
        if (match && match.groups?.path) {
          return toolkitRequire.resolve(match.groups.path.replaceAll(cjsPathSegment, esmPathSegment));
        }
      }
      throw error;
    } finally {
      resolving.delete(specifier);
    }
  };

  return { packageName, toolkitRequire, toolkitResolve, resolving };
};

/**
 * Resolves `specifier` to an absolute `file://` URL against the variant workspace pinned for
 * `compactcVersion` — exactly the redirect the hook installed by {@link installCompactcResolver} applies
 * to bare `@midnight-ntwrk/compact-js*` imports.
 *
 * Use this to `import()` a `compact-js*` module from a context whose module loader pre-resolves bare
 * specifiers before Node's resolution hook can run — notably Vitest's runner, which would otherwise load
 * whichever copy npm hoisted to the workspace root (a *different*, version-mismatched variant). Importing
 * the returned absolute URL pins the load to this variant's copy; that copy's own (bare) transitive
 * imports then still flow through the installed hook, keeping the whole module graph on one matched
 * version. The hook must already be installed (see {@link installCompactcResolver}) for those transitive
 * imports to be redirected.
 */
export const resolveVariantModule = (compactcVersion: string, specifier: string): string =>
  `file://${makeVariantResolver(compactcVersion).toolkitResolve(specifier)}`;

/**
 * Installs a module-resolution hook that redirects every `@midnight-ntwrk/compact-js*` and
 * `@midnight-ntwrk/compact-runtime` import (including transitive ones, e.g. those reached while loading a
 * contract config file) to the copy pinned by the variant workspace for `compactcVersion`.
 *
 * This is the single source of truth for version dispatch shared by the CLI entrypoint (`bin.ts`) and the
 * test setup, so tests exercise the same resolution behaviour as production.
 *
 * @returns The resolved variant package name, so callers can dynamically import it.
 */
export const installCompactcResolver = (compactcVersion: string): string => {
  const { packageName, toolkitRequire, toolkitResolve, resolving } = makeVariantResolver(compactcVersion);

  // tsc/ts-node (used by compact-js-command's `ConfigCompiler` to type-check `contract.config.ts`) do NOT
  // honour the runtime resolve hook below — they use their own module resolution. Left to defaults they
  // pick up whichever `@midnight-ntwrk/compact-js` npm hoisted to the workspace root (a *different* variant),
  // which types the contract against a mismatched `@midnight-ntwrk/compact-runtime` and collapses the
  // generated `Witnesses` type to `never`. Pin ts-node's resolution to *this* variant's copies via
  // `TS_NODE_COMPILER_OPTIONS.paths` (ts-node merges these into the discovered tsconfig). Resolved here,
  // before the hook is registered, so these lookups can't re-enter it.
  // Resolve via `<pkg>/package.json` (exported by both packages) rather than the bare specifier — the
  // bare entry is ESM-only and not resolvable through `createRequire`'s CJS `resolve`.
  const packageDir = (id: string): string => dirname(toolkitRequire.resolve(`${id}/package.json`));
  try {
    const tsPaths: Record<string, string[]> = {};
    for (const id of ['@midnight-ntwrk/compact-js', '@midnight-ntwrk/compact-runtime']) {
      const dir = packageDir(id);
      tsPaths[id] = [dir];
      tsPaths[`${id}/*`] = [`${dir}/*`];
    }
    const existing = process.env.TS_NODE_COMPILER_OPTIONS
      ? JSON.parse(process.env.TS_NODE_COMPILER_OPTIONS)
      : {};
    process.env.TS_NODE_COMPILER_OPTIONS = JSON.stringify({
      baseUrl: dirname(dirname(fileURLToPath(import.meta.url))),
      ...existing,
      paths: { ...(existing.paths ?? {}), ...tsPaths }
    });
  } catch {
    // Best-effort: if a variant copy can't be located, leave resolution to ts-node's defaults.
  }

  registerHooks({
    resolve(specifier: string, context: ResolveHookContext, next) {
      // Intercept imports of the 'compact-js*' and 'compact-runtime' packages, and resolve them relative
      // to the version installed in the toolkit package that will be run for the current COMPACTC_VERSION...
      // `!resolving.has(specifier)` skips interception on re-entry: on Node 24 the `toolkitResolve` call
      // below issues a CJS `require.resolve` that routes back through this hook for the same specifier.
      // Deferring to `next` then lets Node's default resolution finish — rooted at the variant package,
      // since the re-entrant lookup originates from `toolkitRequire` — instead of recursing until the
      // stack overflows.
      if (
        !resolving.has(specifier) &&
        (specifier.startsWith('@midnight-ntwrk/compact-js') ||
          specifier.startsWith('@midnight-ntwrk/compact-runtime') ||
          specifier.startsWith('@midnight-ntwrk/platform-js'))
      ) {
        return {
          url: `file://${toolkitResolve(specifier)}`,
          shortCircuit: true
        };
      }
      // ... otherwise, use the default resolution logic.
      return next(specifier, context);
    }
  });

  return packageName;
};
