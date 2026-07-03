#!/usr/bin/env node

import { readFileSync } from 'node:fs';

import { installCompactcResolver, resolveCompactcVersion } from './compactc-resolver.js';
import { expandSecretFileArgs } from './secret-args.js';

// Secrets arrive via temp files, never argv (see secret-args.ts). Expand them
// into process.argv before the CLI parses it; the kernel-visible cmdline is
// unaffected.
expandSecretFileArgs(process.argv, (path) => readFileSync(path, 'utf8'));

// Determine the supported variant for the current COMPACTC_VERSION and install the resolution hook that
// redirects `compact-js*` / `compact-runtime` imports to that variant's pinned copies.
const compactcVersion = resolveCompactcVersion();
const toolkitPackageName = installCompactcResolver(compactcVersion);

// Dynamically import the appropriate version of the toolkit based on the COMPACTC_VERSION environment
// variable and run it.
import(toolkitPackageName)
  .then(({ run }) => run())
  .catch((error) => {
    console.error('Unexpected error running toolkit:', error);
    process.exit(1);
  });
