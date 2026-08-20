import path from 'path';
import { fileURLToPath } from 'url';
import { generateCoverageReport } from './generate.ts';
import { runCoverageChecks } from './check.ts';

const parseArgs = (argv: string[]): Record<string, string> => {
  const out: Record<string, string> = {};
  for (const arg of argv) {
    if (!arg.startsWith('--')) continue;
    const [k, ...rest] = arg.slice(2).split('=');
    out[k] = rest.join('=') || 'true';
  }
  return out;
};

const main = async (): Promise<void> => {
  const [, , command = 'generate', ...rawArgs] = process.argv;
  const args = parseArgs(rawArgs);
  const thisDir = path.dirname(fileURLToPath(import.meta.url));
  const repoRoot = path.resolve(thisDir, '../../../../');
  const qaTestsRoot = path.resolve(thisDir, '../../');

  const schemaPath = args['schema-path']
    ? path.resolve(repoRoot, args['schema-path'])
    : path.resolve(repoRoot, 'indexer-api/graphql/schema-v4.graphql');
  const operationDirPath = args['operations-dir']
    ? path.resolve(repoRoot, args['operations-dir'])
    : path.resolve(repoRoot, 'qa/tests/utils/indexer/graphql');
  const testResultsPath = args['results-file']
    ? path.resolve(qaTestsRoot, args['results-file'])
    : path.resolve(qaTestsRoot, 'reports/json/test-results.json');
  const outputDir = args['output-dir']
    ? path.resolve(qaTestsRoot, args['output-dir'])
    : path.resolve(qaTestsRoot, 'coverage/output');
  const facetKeywordsPath = path.resolve(qaTestsRoot, 'coverage/config/facet-keywords.json');
  const mappingOverridesPath = path.resolve(qaTestsRoot, 'coverage/config/mapping-overrides.json');
  const staticMethodMapPath = path.resolve(qaTestsRoot, 'coverage/config/static-method-map.json');
  const fieldAreasPath = path.resolve(qaTestsRoot, 'coverage/config/field-areas.json');
  const checkThresholdsPath = path.resolve(qaTestsRoot, 'coverage/config/check-thresholds.json');
  const criticalOperationsPath = path.resolve(qaTestsRoot, 'coverage/config/critical-operations.json');

  if (command === 'generate') {
    const result = await generateCoverageReport({
      repoRoot,
      schemaPath,
      operationDirPath,
      testResultsPath,
      outputDir,
      facetKeywordsPath,
      mappingOverridesPath,
      staticMethodMapPath,
      fieldAreasPath,
      qaTestsRoot,
      targetEnv: args['target-env'],
      indexerApiVersion: args['indexer-api-version'],
    });
    console.log(`Coverage reports written to ${result.outputDir}`);
    return;
  }

  if (command === 'check') {
    await runCoverageChecks({
      repoRoot,
      outputDir,
      checkThresholdsPath,
      criticalOperationsPath,
    });
    console.log('Coverage checks passed');
    return;
  }

  throw new Error(`Unknown command "${command}". Use: generate | check`);
};

main().catch((error: unknown) => {
  const message = error instanceof Error ? error.message : String(error);
  console.error(message);
  process.exit(1);
});
