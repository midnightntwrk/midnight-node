#!/usr/bin/env node

import { createRequire, registerHooks, type ResolveHookContext } from 'node:module';
import { sep } from 'node:path';

/** A regular expression to match module resolution paths in error messages. */
const ERROR_MODULE_REGEXP = /module '(?<path>.*)'$/;
/** Currently supported ledger versions. */
const SUPPORTED_LEDGER_VERSIONS = [7, 8];

const ledgerVersionStr = process.env.LEDGER_VERSION ?? '8';
const ledgerVersion = parseInt(ledgerVersionStr, 10);

if (!SUPPORTED_LEDGER_VERSIONS.includes(ledgerVersion)) {
  console.error(`Unsupported LEDGER_VERSION: ${ledgerVersionStr} (expected one of ${SUPPORTED_LEDGER_VERSIONS.join(', ')})`);
  process.exit(1);
}

const toolkitPackageName = `@midnight-ntwrk/node-toolkit-v${ledgerVersion}`;
const require = createRequire(import.meta.url);
const toolkitRequire = createRequire(require.resolve(toolkitPackageName));
const cjsPathSegment = `${sep}dist${sep}cjs${sep}`;
const esmPathSegment = `${sep}dist${sep}esm${sep}`;

const toolkitResolve = (specifier: string) => {
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
}

registerHooks({
  resolve(specifier: string, context: ResolveHookContext, next) {
    // Intercept imports of the 'compact-js*' packages and resolve them relative to their version installed
    // in the toolkit package that will be run for the current LEDGER_VERSION...
    if (specifier.startsWith('@midnight-ntwrk/compact-js')) {
      return {
        url: `file://${toolkitResolve(specifier)}`,
        shortCircuit: true
      }
    }
    // ... otherwise, use the default resolution logic.
    return next(specifier, context);
  }
});

// Dynamically import the appropriate version of the toolkit based on the LEDGER_VERSION environment variable
// and run it.
import(toolkitPackageName)
  .then(({run}) => run())
  .catch((error) => {
    console.error('Unexpected error running toolkit:', error);
    process.exit(1);
  });
