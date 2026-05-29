#!/usr/bin/env node

import { createRequire, registerHooks, type ResolveHookContext } from 'node:module';
import { sep } from 'node:path';

/** A regular expression to match module resolution paths in error messages. */
const ERROR_MODULE_REGEXP = /module '(?<path>.*)'$/;
/** Currently supported `compactc` versions (in `<major>.<minor>` form). Each maps to a sibling
 * `compact-<major>.<minor>/` workspace pinning the matched `@midnight-ntwrk/compact-js` line. */
const SUPPORTED_COMPACTC_VERSIONS = ['0.29', '0.30', '0.31'];
const DEFAULT_COMPACTC_VERSION = '0.31';

// Accept either `<major>.<minor>` or `<major>.<minor>.<patch>` — `compact-js` is patch-stable, so we
// dispatch on `<major>.<minor>` only.
const rawCompactcVersion = process.env.COMPACTC_VERSION ?? DEFAULT_COMPACTC_VERSION;
const compactcVersion = rawCompactcVersion.split('.').slice(0, 2).join('.');

if (!SUPPORTED_COMPACTC_VERSIONS.includes(compactcVersion)) {
  console.error(`Unsupported COMPACTC_VERSION: ${rawCompactcVersion} (expected one of ${SUPPORTED_COMPACTC_VERSIONS.join(', ')})`);
  process.exit(1);
}

const toolkitPackageName = `@midnight-ntwrk/node-toolkit-compact-${compactcVersion}`;
const require = createRequire(import.meta.url);
const toolkitRequire = createRequire(require.resolve(toolkitPackageName));
const cjsPathSegment = `${sep}dist${sep}cjs${sep}`;
const esmPathSegment = `${sep}dist${sep}esm${sep}`;

/**
 * Resolves a module relative to the toolkit package, with special handling to rewrite paths to support
 * both CommonJS and ESM versions.
 *
 * @param specifier The module to resolve.
 * @returns A string representing the resolved path to of `specifier` relative to the toolkit package.
 * @throws If `specifier` cannot be resolved.
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
    // to the version installed in the toolkit package that will be run for the current
    // COMPACTC_VERSION...
    if (specifier.startsWith('@midnight-ntwrk/compact-js') || specifier.startsWith('@midnight-ntwrk/compact-runtime')) {
      return {
        url: `file://${toolkitResolve(specifier)}`,
        shortCircuit: true
      }
    }
    // ... otherwise, use the default resolution logic.
    return next(specifier, context);
  }
});

// Dynamically import the appropriate version of the toolkit based on the COMPACTC_VERSION environment
// variable and run it.
import(toolkitPackageName)
  .then(({run}) => run())
  .catch((error) => {
    console.error('Unexpected error running toolkit:', error);
    process.exit(1);
  });
