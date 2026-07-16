import { Effect, Layer } from 'effect';
import { Command } from '@effect/cli';
import { NodeContext } from '@effect/platform-node';
import { describe, it, expect } from 'vitest';
import { resolve } from 'node:path';
import { rm, stat } from 'node:fs/promises';
import { resolveVariantModule } from '../src/compactc-resolver.js';

const COUNTER_CONFIG_FILEPATH = resolve(import.meta.dirname, 'contract/contract.config.ts');

// The variant exercised by this run, selected by COMPACTC_VERSION and pinned by the resolution hook
// installed in `test/setup-compactc-resolver.ts`. Run the suite once per supported version (see the
// `test:compat` npm script) to cover them all.
const version = process.env.RESOLVED_COMPACTC_VERSION ?? 'default';
const COUNTER_OUTPUT_FILEPATH = resolve(import.meta.dirname, `intent-${version}.bin`);
const COUNTER_OUTPUT_PS_FILEPATH = resolve(import.meta.dirname, `output-ps-${version}.json`);
const COUNTER_OUTPUT_ZSWAP_FILEPATH = resolve(import.meta.dirname, `zswap-${version}.json`);

describe(`Deploy Command (compact ${version})`, () => {
  it('should run to success', async () => {
    // Resolve the entrypoint to the variant copy pinned for this COMPACTC_VERSION and import that absolute
    // path. A *bare* specifier here is pre-resolved by Vitest's module runner before the version-dispatch
    // hook (installed in test/setup-compactc-resolver.ts) can redirect it, so it loads whichever
    // compact-js-command npm hoisted to the workspace root — a different, version-mismatched variant whose
    // ledger deserializes the contract state at the wrong format version. Importing the resolved absolute
    // path pins it to this variant; its transitive bare imports still flow through the hook. See
    // src/compactc-resolver.ts (resolveVariantModule).
    const { ConfigCompiler, deployCommand } = await import(
      resolveVariantModule(version, '@midnight-ntwrk/compact-js-command/effect')
    );

    const testLayer = Layer.mergeAll(
      ConfigCompiler.layer.pipe(Layer.provideMerge(NodeContext.layer))
    );

    // Clean any output left over from a previous run so the existence assertion is meaningful.
    await rm(COUNTER_OUTPUT_FILEPATH, { force: true });
    await rm(COUNTER_OUTPUT_PS_FILEPATH, { force: true });
    await rm(COUNTER_OUTPUT_ZSWAP_FILEPATH, { force: true });

    await Effect.gen(function*() {
      // Make a command line instance from the 'deploy' command...
      const cli = Command.run(deployCommand, { name: 'deploy', version: '0.0.0' });

      // ...and then execute it. We'll use the '-c' option to provide a path to a configuration file, and
      // the '-o' to provide a path to where we want the serialized Intent to be written.
      yield* cli([
        'node', 'deploy.ts',
        '-c', COUNTER_CONFIG_FILEPATH,
        '-o', COUNTER_OUTPUT_FILEPATH,
        '--output-ps', COUNTER_OUTPUT_PS_FILEPATH,
        // Route the generated ZswapLocalState to a per-version path too; it otherwise defaults to a
        // 'zswap.json' written in the working directory.
        '--output-zswap', COUNTER_OUTPUT_ZSWAP_FILEPATH,
        '0' // The contract constructor receives the initial value of the counter as an argument.
      ]);
    }).pipe(
      Effect.provide(testLayer),
      Effect.runPromise
    );

    // The command should have written non-empty serialized files to disk.
    for (const outfileFile of [COUNTER_OUTPUT_FILEPATH, COUNTER_OUTPUT_PS_FILEPATH, COUNTER_OUTPUT_ZSWAP_FILEPATH]) {
      const fileStat = await stat(outfileFile);
      expect(fileStat.size).toBeGreaterThan(0);
    }
  });
});
