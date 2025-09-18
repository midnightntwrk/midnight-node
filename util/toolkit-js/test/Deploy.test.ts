import { Effect, Layer } from 'effect';
import { Command } from '@effect/cli';
import { NodeContext } from '@effect/platform-node';
import { describe, it, expect } from 'vitest';
import { ConfigCompiler, deployCommand } from '@midnight-ntwrk/compact-js-command/effect';
import { resolve } from 'node:path';

const COUNTER_CONFIG_FILEPATH = resolve(import.meta.dirname, 'contract/contract.config.ts');
const COUNTER_OUTPUT_FILEPATH = resolve(import.meta.dirname, 'intent.bin');
const COUNTER_OUTPUT_PS_FILEPATH = resolve(import.meta.dirname, 'output-ps.json');

const testLayer = Layer.mergeAll(
  ConfigCompiler.layer.pipe(
    Layer.provideMerge(NodeContext.layer)
  )
);

describe('Deploy Command', () => {
  it('should run to success', async () => {
    await Effect.gen(function*() {
      // Make a command line instance from the 'deploy' command...
      const cli = Command.run(deployCommand, { name: 'deploy', version: '0.0.0' });

      // ...and then execute it. We'll use the '-c' option to provide a path to a configuration file, and the
      // '-o' to provide a path to where we want the serialized Intent to be written.
      yield* cli([
        'node', 'deploy.ts',
        '-c', COUNTER_CONFIG_FILEPATH,
        '-o', COUNTER_OUTPUT_FILEPATH,
        '--output-ps', COUNTER_OUTPUT_PS_FILEPATH
      ]);
    }).pipe(
      Effect.provide(testLayer),
      Effect.runPromise
    );
  });
});
