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
import { sep } from 'node:path';

/** A regular expression to match module resolution paths in error messages. */
const ERROR_MODULE_REGEXP = /module '(?<path>.*)'$/;

/** Currently supported `compactc` versions (in full `<major>.<minor>.<patch>` form). Each maps to a sibling
 * `compact-<major>.<minor>.<patch>/` workspace pinning the matched `@midnight-ntwrk/compact-js` line. */
export const SUPPORTED_COMPACTC_VERSIONS = ['0.29.0', '0.30.0', '0.31.0', '0.31.108'] as const;

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
  const packageName = toolkitPackageName(compactcVersion);
  const require = createRequire(import.meta.url);
  const toolkitRequire = createRequire(require.resolve(packageName));
  const cjsPathSegment = `${sep}dist${sep}cjs${sep}`;
  const esmPathSegment = `${sep}dist${sep}esm${sep}`;

  /**
   * Resolves a module relative to the toolkit package, with special handling to rewrite paths to support
   * both CommonJS and ESM versions.
   */
  const toolkitResolve = (specifier: string) => {
    // While this is dependant on the exact error message format of MODULE_NOT_FOUND errors, it is the
    // most simple way to support both CJS and ESM versions of paths without having to build a full resolver.
    // In the future, we may want to consider building a more robust resolver or adopt a third party package.
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
    }
  };

  registerHooks({
    resolve(specifier: string, context: ResolveHookContext, next) {
      // Intercept imports of the 'compact-js*' and 'compact-runtime' packages, and resolve them relative
      // to the version installed in the toolkit package that will be run for the current COMPACTC_VERSION...
      if (
        specifier.startsWith('@midnight-ntwrk/compact-js') ||
        specifier.startsWith('@midnight-ntwrk/compact-runtime')
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
