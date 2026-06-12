import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    // Install the compactc version-dispatch resolution hook before any test runs. The active version is
    // taken from COMPACTC_VERSION; run the suite once per supported version (see the `test:compat`
    // script) to cover them all.
    setupFiles: ['./test/setup-compactc-resolver.ts'],
    // Each test loads/compiles a contract config and writes serialized output to disk; keep files
    // sequential so per-version runs don't contend.
    fileParallelism: false
  }
});
