import path from 'path';
import { parse, Kind } from 'graphql';
import type { TypeNode } from 'graphql';
import type {
  Confidence,
  CoverageReport,
  EvidenceSource,
  FieldCoverage,
  RootField,
  TestEvidence,
  VitestJsonResult,
} from './types.ts';
import { classifyFacets, type FacetKeywordConfig } from './classify.ts';
import { getBranchName, getCommitSha } from './git.ts';
import { readJson, readText, writeJson, writeText } from './io.ts';
import { readLogEvidence } from './log-evidence.ts';
import {
  inferProjectFromTestFile,
  readMappingOverrides,
  readOperationFieldUsage,
  readSchemaRootFields,
} from './parser.ts';
import { readStaticEvidence } from './static-evidence.ts';
import { normalizeText, sha256, toKebabTokens, toRelativeFromRepo, toUtcCompact, unique } from './utils.ts';

interface SchemaFieldVariant {
  rootType: RootField['rootType'];
  field: string;
  variants: string[];
}

interface InputObjectMeta {
  oneOf: boolean;
  fields: Array<{ name: string; required: boolean }>;
}

export interface GenerateOptions {
  repoRoot: string;
  schemaPath: string;
  operationDirPath: string;
  testResultsPath: string;
  outputDir: string;
  facetKeywordsPath: string;
  mappingOverridesPath: string;
  staticMethodMapPath: string;
  fieldAreasPath: string;
  qaTestsRoot: string;
  targetEnv?: string;
  indexerApiVersion?: string;
}

export interface GenerateResult {
  report: CoverageReport;
  outputDir: string;
  uniqueId: string;
}

const parseLabelsFromMeta = (meta?: Record<string, unknown>): string[] => {
  if (!meta) return [];
  const custom = (meta.custom ?? {}) as { labels?: unknown; testKey?: string };
  return Array.isArray(custom.labels) ? custom.labels.filter((x): x is string => typeof x === 'string') : [];
};

const parseExternalId = (meta?: Record<string, unknown>): string | undefined => {
  if (!meta) return undefined;
  const custom = (meta.custom ?? {}) as { testKey?: unknown };
  return typeof custom.testKey === 'string' ? custom.testKey : undefined;
};

const scoreFieldMatch = (haystack: string, fieldName: string): number => {
  const hay = normalizeText(haystack);
  const fieldTokens = toKebabTokens(fieldName);
  if (fieldTokens.length === 0) return 0;
  let score = 0;
  for (const token of fieldTokens) if (hay.includes(token)) score += 1;
  if (hay.includes(normalizeText(fieldName))) score += 2;
  return score;
};

const inferFieldEvidence = (
  test: TestEvidence,
  candidateFields: RootField[],
): Array<{ field: RootField; confidence: Confidence; reason: string; source: EvidenceSource }> => {
  const haystack = `${test.testFile} ${test.fullName}`;
  const scored = candidateFields
    .map((field) => ({ field, score: scoreFieldMatch(haystack, field.field) }))
    .filter((x) => x.score > 0)
    .sort((a, b) => b.score - a.score);

  if (scored.length === 0) return [];

  const top = scored[0].score;
  return scored
    .filter((x) => x.score === top)
    .slice(0, 3)
    .map((x) => ({
      field: x.field,
      confidence: top >= 3 ? 'high' : top === 2 ? 'medium' : 'low',
      reason: 'keyword-match',
      source: 'heuristic',
    }));
};

const toCoverageStatus = (
  supportCount: number,
  facets: string[],
  rootType: RootField['rootType'],
): FieldCoverage['status'] => {
  if (supportCount === 0) return 'missing';
  if (rootType === 'Subscription' && !facets.includes('streaming')) return 'partial';
  if (facets.length < 2) return 'partial';
  return 'covered';
};

const toMarkdown = (report: CoverageReport): string => {
  const facetColumns = ['positive', 'negative', 'schemaValidation', 'edgeCase', 'streaming'] as const;
  const facetMark = (facets: string[], facet: string): string => (facets.includes(facet) ? 'x' : '');
  const lines: string[] = [];
  lines.push('# API Feature Coverage Report');
  lines.push('');
  lines.push('## Metadata');
  lines.push(`- generated_at_utc: ${report.metadata.generatedAtUtc}`);
  lines.push(`- branch: ${report.metadata.branch}`);
  lines.push(`- commit_sha: ${report.metadata.commitSha}`);
  lines.push(`- output_unique_id: ${report.metadata.output.uniqueId}`);
  lines.push(`- output_dir: ${report.metadata.output.outputDir}`);
  lines.push(`- schema_path: ${report.metadata.schemaFingerprint.path}`);
  lines.push(`- schema_sha256: ${report.metadata.schemaFingerprint.sha256}`);
  lines.push(
    `- test_run_context: source=${report.metadata.testRunContext.sourceResultsPath}, target_env=${report.metadata.testRunContext.targetEnv ?? 'n/a'}, api_version=${report.metadata.testRunContext.indexerApiVersion ?? 'n/a'}, projects=${report.metadata.testRunContext.detectedProjects.join(', ') || 'none'}`,
  );
  lines.push('');
  lines.push('## Summary');
  lines.push(
    `- fields: total=${report.summary.totalFields}, covered=${report.summary.coveredFields}, partial=${report.summary.partialFields}, missing=${report.summary.missingFields}, covered_percent=${report.summary.percentCovered.toFixed(2)}`,
  );
  for (const [rootType, stats] of Object.entries(report.summary.byRootType)) {
    lines.push(
      `- ${rootType}: total=${stats.total}, covered=${stats.covered}, partial=${stats.partial}, missing=${stats.missing}, covered_percent=${stats.percentCovered.toFixed(2)}`,
    );
  }
  lines.push('- Area Breakdown:');
  for (const [area, stats] of Object.entries(report.summary.byArea)) {
    lines.push(
      `  - ${area}: total=${stats.total}, covered=${stats.covered}, partial=${stats.partial}, missing=${stats.missing}, covered_percent=${stats.percentCovered.toFixed(2)}`,
    );
  }
  lines.push('');
  lines.push('## Missing Fields');
  for (const gap of report.gaps) {
    lines.push(`- ${gap.rootType}.${gap.field} (${gap.reason})`);
  }
  if (report.gaps.length === 0) lines.push('- none');
  lines.push('');
  lines.push('## Missing by Area');
  for (const [area, gaps] of Object.entries(report.gapsByArea)) {
    lines.push(`### ${area} (${gaps.length})`);
    if (gaps.length === 0) {
      lines.push('- none');
    } else {
      for (const gap of gaps) lines.push(`- ${gap.rootType}.${gap.field} (${gap.reason})`);
    }
    lines.push('');
  }
  lines.push('');
  lines.push('## Diagnostics');
  lines.push(`- orphan_operation_fields: ${report.diagnostics.orphanOperationFields.length}`);
  lines.push(`- tested_unknown_schema_fields: ${report.diagnostics.testedUnknownSchemaFields.length}`);
  lines.push(`- schema_fields_without_helper: ${report.diagnostics.schemaFieldsWithoutHelper.length}`);
  lines.push('');
  lines.push('## Field Details');
  lines.push(
    '| Field | Status | Projects | Sources | positive | negative | schemaValidation | edgeCase | streaming | Tests |',
  );
  lines.push(
    '|---|---|---|---|---:|---:|---:|---:|---:|---:|',
  );
  for (const field of report.fields) {
    const sources = unique(field.supportingTests.map((t) => t.evidenceSource)).join(', ') || '-';
    const projects = field.projects.join(', ') || '-';
    lines.push(
      `| ${field.rootType}.${field.field} | ${field.status} | ${projects} | ${sources} | ${facetMark(field.facets, facetColumns[0])} | ${facetMark(field.facets, facetColumns[1])} | ${facetMark(field.facets, facetColumns[2])} | ${facetMark(field.facets, facetColumns[3])} | ${facetMark(field.facets, facetColumns[4])} | ${field.supportingTests.length} |`,
    );
  }
  lines.push('');
  return lines.join('\n');
};

const toSchemaMarkdown = (
  schemaPath: string,
  schemaHash: string,
  fields: RootField[],
  generatedAtUtc: string,
  uniqueId: string,
): string => {
  const byRoot = {
    Query: fields.filter((f) => f.rootType === 'Query').map((f) => f.field).sort(),
    Mutation: fields.filter((f) => f.rootType === 'Mutation').map((f) => f.field).sort(),
    Subscription: fields.filter((f) => f.rootType === 'Subscription').map((f) => f.field).sort(),
  };
  const lines: string[] = [];
  lines.push('# Indexer Schema Snapshot');
  lines.push('');
  lines.push(`- generated_at_utc: ${generatedAtUtc}`);
  lines.push(`- schema_path: ${schemaPath}`);
  lines.push(`- schema_sha256: ${schemaHash}`);
  lines.push(`- output_unique_id: ${uniqueId}`);
  lines.push('');
  for (const rootType of ['Query', 'Mutation', 'Subscription'] as const) {
    lines.push(`## ${rootType} (${byRoot[rootType].length})`);
    for (const field of byRoot[rootType]) lines.push(`- ${field}`);
    lines.push('');
  }
  return lines.join('\n');
};

const typeToVariantHints = (typeName: string): string[] => {
  switch (typeName) {
    case 'BlockOffset':
      return ['hash', 'height'];
    case 'TransactionOffset':
      return ['hash', 'id'];
    case 'ContractActionOffset':
      return ['blockOffset', 'transactionOffset'];
    default:
      return [];
  }
};

const unwrapTypeName = (typeNode: TypeNode): string => {
  let current = typeNode;
  while (current?.kind === Kind.NON_NULL_TYPE || current?.kind === Kind.LIST_TYPE) {
    current = current.type;
  }
  return current?.name?.value ?? 'Unknown';
};

const isNonNull = (typeNode: TypeNode): boolean => typeNode.kind === Kind.NON_NULL_TYPE;

const buildInputObjectMeta = (schemaDoc: ReturnType<typeof parse>): Map<string, InputObjectMeta> => {
  const map = new Map<string, InputObjectMeta>();
  for (const def of schemaDoc.definitions) {
    if (def.kind !== Kind.INPUT_OBJECT_TYPE_DEFINITION) continue;
    const oneOf = (def.directives ?? []).some((d) => d.name.value === 'oneOf');
    const fields = (def.fields ?? []).map((f) => ({
      name: f.name.value,
      required: isNonNull(f.type),
    }));
    map.set(def.name.value, { oneOf, fields });
  }
  return map;
};

const cartesian = (chunks: string[][]): string[][] => {
  let acc: string[][] = [[]];
  for (const chunk of chunks) {
    const next: string[][] = [];
    for (const prev of acc) for (const item of chunk) next.push([...prev, item].filter(Boolean));
    acc = next;
  }
  return acc;
};

const buildExpandedSchema = (schemaText: string): SchemaFieldVariant[] => {
  const doc = parse(schemaText);
  const inputMeta = buildInputObjectMeta(doc);
  const variants: SchemaFieldVariant[] = [];
  for (const def of doc.definitions) {
    if (def.kind !== Kind.OBJECT_TYPE_DEFINITION) continue;
    if (!['Query', 'Mutation', 'Subscription'].includes(def.name.value)) continue;
    const rootType = def.name.value as RootField['rootType'];
    for (const field of def.fields ?? []) {
      const args = field.arguments ?? [];
      if (args.length === 0) {
        variants.push({ rootType, field: field.name.value, variants: [`${field.name.value}()`] });
        continue;
      }
      const hasRequiredArgs = args.some((arg) => isNonNull(arg.type));
      const argChoices: string[][] = [];
      for (const arg of args) {
        const argName = arg.name.value;
        const typeName = unwrapTypeName(arg.type);
        const required = isNonNull(arg.type);
        const oneOfMeta = inputMeta.get(typeName);
        let choices: string[];
        if (oneOfMeta?.oneOf) {
          choices = oneOfMeta.fields.map((f) => `${argName}=${f.name}`);
        } else {
          const hints = typeToVariantHints(typeName);
          choices = hints.length > 0 ? hints.map((h) => `${argName}=${h}`) : [`${argName}=<value>`];
        }
        if (required) argChoices.push(choices);
        else argChoices.push(['', ...choices]);
      }
      const callVariants = cartesian(argChoices)
        .map((parts) => parts.filter(Boolean))
        .filter((parts) => (hasRequiredArgs ? parts.length > 0 : true))
        .map((parts) => `${field.name.value}(${parts.join(', ')})`);
      variants.push({
        rootType,
        field: field.name.value,
        variants: unique(callVariants),
      });
    }
  }
  return variants.sort((a, b) => `${a.rootType}.${a.field}`.localeCompare(`${b.rootType}.${b.field}`));
};

const toSchemaExpandedMarkdown = (
  generatedAtUtc: string,
  schemaPath: string,
  schemaHash: string,
  uniqueId: string,
  variants: SchemaFieldVariant[],
): string => {
  const byRoot: Record<RootField['rootType'], SchemaFieldVariant[]> = {
    Query: [],
    Mutation: [],
    Subscription: [],
  };
  for (const v of variants) byRoot[v.rootType].push(v);

  const lines: string[] = [];
  lines.push('# Indexer Schema Expanded');
  lines.push('');
  lines.push(`- generated_at_utc: ${generatedAtUtc}`);
  lines.push(`- schema_path: ${schemaPath}`);
  lines.push(`- schema_sha256: ${schemaHash}`);
  lines.push(`- output_unique_id: ${uniqueId}`);
  lines.push('');
  for (const rootType of ['Query', 'Mutation', 'Subscription'] as const) {
    lines.push(`## ${rootType}`);
    for (const v of byRoot[rootType]) {
      for (const call of v.variants) {
        lines.push(`- ${call}`);
      }
    }
    lines.push('');
  }
  return lines.join('\n');
};

const toHtml = (report: CoverageReport): string => {
  const facetColumns = ['positive', 'negative', 'schemaValidation', 'edgeCase', 'streaming'] as const;
  const facetMark = (facets: string[], facet: string): string => (facets.includes(facet) ? '✓' : '');
  const rows = report.fields
    .map((field) => {
      const sources = unique(field.supportingTests.map((t) => t.evidenceSource)).join(', ');
      return `<tr><td>${field.rootType}.${field.field}</td><td>${field.status}</td><td>${sources || '-'}</td><td>${field.projects.join(', ') || '-'}</td><td>${facetMark(field.facets, facetColumns[0])}</td><td>${facetMark(field.facets, facetColumns[1])}</td><td>${facetMark(field.facets, facetColumns[2])}</td><td>${facetMark(field.facets, facetColumns[3])}</td><td>${facetMark(field.facets, facetColumns[4])}</td><td>${field.supportingTests.length}</td></tr>`;
    })
    .join('\n');
  const areaRows = Object.entries(report.summary.byArea)
    .map(
      ([area, stats]) =>
        `<tr><td>${area}</td><td>${stats.total}</td><td>${stats.covered}</td><td>${stats.partial}</td><td>${stats.missing}</td><td>${stats.percentCovered.toFixed(2)}</td></tr>`,
    )
    .join('\n');
  const coreStats = report.summary.byArea['indexer-core'] ?? {
    total: 0,
    covered: 0,
    partial: 0,
    missing: 0,
    percentCovered: 0,
  };
  const coreCovered = coreStats.covered;
  const corePartial = coreStats.partial;
  const coreMissing = coreStats.missing;
  const coreTotal = Math.max(1, coreCovered + corePartial + coreMissing);
  const degCovered = (coreCovered / coreTotal) * 360;
  const degPartial = (corePartial / coreTotal) * 360;
  const degMissing = Math.max(0, 360 - degCovered - degPartial);
  const pieBackground = `conic-gradient(#4caf50 0deg ${degCovered}deg, #ffb300 ${degCovered}deg ${
    degCovered + degPartial
  }deg, #e53935 ${degCovered + degPartial}deg ${degCovered + degPartial + degMissing}deg)`;
  const missingAreaSections = Object.entries(report.gapsByArea)
    .map(([area, gaps]) => {
      const rows = gaps.length
        ? gaps
            .map((gap) => `<tr><td>${gap.rootType}.${gap.field}</td><td>${gap.reason}</td></tr>`)
            .join('\n')
        : '<tr><td colspan="2">none</td></tr>';
      return `<h3>${area} (${gaps.length})</h3>
  <table>
    <thead><tr><th>Field</th><th>Reason</th></tr></thead>
    <tbody>
      ${rows}
    </tbody>
  </table>`;
    })
    .join('\n');
  return `<!doctype html>
<html>
<head>
  <meta charset="utf-8" />
  <title>API Coverage Report</title>
  <style>
    body { font-family: Arial, sans-serif; margin: 20px; }
    .meta { margin-bottom: 16px; color: #333; }
    .cards { display:flex; gap:12px; margin: 12px 0 20px 0; }
    .card { border:1px solid #ddd; border-radius:8px; padding:12px; min-width:140px; }
    table { border-collapse: collapse; width: 100%; font-size: 13px; }
    th, td { border:1px solid #ddd; padding:6px 8px; text-align:left; }
    th { background:#f7f7f7; }
  </style>
</head>
<body>
  <h1>API Feature Coverage</h1>
  <div class="meta">
    <div>Generated: ${report.metadata.generatedAtUtc}</div>
    <div>Branch: ${report.metadata.branch}</div>
    <div>Commit: ${report.metadata.commitSha}</div>
    <div>Output ID: ${report.metadata.output.uniqueId}</div>
  </div>
  <div class="cards">
    <div class="card"><strong>Total</strong><div>${report.summary.totalFields}</div></div>
    <div class="card"><strong>Covered</strong><div>${report.summary.coveredFields}</div></div>
    <div class="card"><strong>Partial</strong><div>${report.summary.partialFields}</div></div>
    <div class="card"><strong>Missing</strong><div>${report.summary.missingFields}</div></div>
    <div class="card"><strong>Covered %</strong><div>${report.summary.percentCovered.toFixed(2)}</div></div>
  </div>
  <h2>Area Coverage</h2>
  <table>
    <thead><tr><th>Area</th><th>Total</th><th>Covered</th><th>Partial</th><th>Missing</th><th>Covered %</th></tr></thead>
    <tbody>
      ${areaRows}
    </tbody>
  </table>
  <h2>Indexer-Core Pie Chart</h2>
  <div style="display:flex; gap:16px; align-items:center; margin-bottom:20px;">
    <div style="width:180px; height:180px; border-radius:50%; background:${pieBackground}; border:1px solid #ddd;"></div>
    <div>
      <div><strong>indexer-core coverage:</strong> ${coreStats.percentCovered.toFixed(2)}%</div>
      <div>Covered: ${coreCovered}</div>
      <div>Partial: ${corePartial}</div>
      <div>Missing: ${coreMissing}</div>
      <div>Total: ${coreStats.total}</div>
      <div style="margin-top:8px;">
        <span style="display:inline-block;width:10px;height:10px;background:#4caf50;margin-right:6px;"></span>Covered
        <span style="display:inline-block;width:10px;height:10px;background:#ffb300;margin:0 6px 0 12px;"></span>Partial
        <span style="display:inline-block;width:10px;height:10px;background:#e53935;margin:0 6px 0 12px;"></span>Missing
      </div>
    </div>
  </div>
  <h2>Field Coverage</h2>
  <table>
    <thead><tr><th>Field</th><th>Status</th><th>Evidence Source</th><th>Projects</th><th>positive</th><th>negative</th><th>schemaValidation</th><th>edgeCase</th><th>streaming</th><th>Tests</th></tr></thead>
    <tbody>
      ${rows}
    </tbody>
  </table>
  <h2>Missing by Area</h2>
  ${missingAreaSections}
</body>
</html>`;
};

const shortSha = (sha: string, n: number): string => (sha === 'unknown' ? 'unknown' : sha.slice(0, n));

export const generateCoverageReport = async (options: GenerateOptions): Promise<GenerateResult> => {
  const schemaFields = await readSchemaRootFields(options.schemaPath);
  const operationUsage = await readOperationFieldUsage(options.repoRoot, options.operationDirPath);
  const vitest = await readJson<VitestJsonResult>(options.testResultsPath);
  const facetConfig = await readJson<FacetKeywordConfig>(options.facetKeywordsPath);
  const overrides = await readMappingOverrides(options.mappingOverridesPath);
  const fieldAreas = await readJson<Record<string, string[]>>(options.fieldAreasPath);
  const schemaText = await readText(options.schemaPath);
  const branch = getBranchName(options.repoRoot);
  const commitSha = getCommitSha(options.repoRoot);
  const schemaHash = sha256(schemaText);
  const uniqueId = `${shortSha(schemaHash, 12)}-${shortSha(commitSha, 8)}-${toUtcCompact(new Date())}`;
  const outputDir = path.join(options.outputDir, uniqueId);
  const outputDirRel = toRelativeFromRepo(options.repoRoot, outputDir);

  const knownFieldsMap = new Map<string, RootField>();
  for (const f of schemaFields) knownFieldsMap.set(`${f.rootType}.${f.field}`, f);
  const operationCandidateFields = unique(
    operationUsage
      .filter((u) => knownFieldsMap.has(`${u.rootType}.${u.field}`))
      .map((u) => `${u.rootType}.${u.field}`),
  ).map((key) => knownFieldsMap.get(key)!);
  const operationFieldKeys = new Set(operationCandidateFields.map((f) => `${f.rootType}.${f.field}`));

  const tests: TestEvidence[] = [];
  const knownTestsByFile: Record<string, string[]> = {};
  for (const suite of vitest.testResults) {
    const relativeSuite = toRelativeFromRepo(options.repoRoot, suite.name);
    const project = inferProjectFromTestFile(relativeSuite);
    for (const assertion of suite.assertionResults) {
      const labels = parseLabelsFromMeta(assertion.meta);
      const facets = classifyFacets(
        `${assertion.fullName} ${assertion.title} ${(assertion.ancestorTitles ?? []).join(' ')}`,
        labels,
        facetConfig,
      );
      const testId = `${relativeSuite}#${assertion.fullName}`;
      tests.push({
        testId,
        testFile: relativeSuite,
        fullName: assertion.fullName,
        status:
          assertion.status === 'passed' ||
          assertion.status === 'failed' ||
          assertion.status === 'skipped' ||
          assertion.status === 'todo'
            ? assertion.status
            : 'unknown',
        project,
        facets,
        externalId: parseExternalId(assertion.meta),
      });
      knownTestsByFile[relativeSuite] = [...(knownTestsByFile[relativeSuite] ?? []), assertion.fullName];
    }
  }

  const staticEvidence = await readStaticEvidence(
    options.qaTestsRoot,
    options.staticMethodMapPath,
    options.testResultsPath,
  );
  const logEvidence = await readLogEvidence(options.qaTestsRoot, knownTestsByFile);
  const overrideMap = new Map(overrides.overrides.map((o) => [o.testId, o]));
  const fieldSupport = new Map<string, FieldCoverage['supportingTests']>();
  const testedFieldKeys = new Set<string>();

  const pushEvidence = (
    test: TestEvidence,
    field: RootField,
    confidence: Confidence,
    reason: string,
    source: EvidenceSource,
  ): void => {
    const key = `${field.rootType}.${field.field}`;
    testedFieldKeys.add(key);
    const current = fieldSupport.get(key) ?? [];
    current.push({
      ...test,
      confidence,
      evidenceReason: reason,
      evidenceSource: source,
    });
    fieldSupport.set(key, current);
  };

  for (const test of tests) {
    const override = overrideMap.get(test.testId);
    if (override) {
      for (const field of override.fields) {
        pushEvidence(
          test,
          field,
          override.confidence ?? 'high',
          override.reason ?? 'mapping-override',
          'override',
        );
      }
      continue;
    }

    const staticFields = staticEvidence[test.testId] ?? [];
    const logFields = logEvidence[test.testId] ?? [];
    const staticSet = new Set(staticFields.map((f) => `${f.rootType}.${f.field}`));
    const logSet = new Set(logFields.map((f) => `${f.rootType}.${f.field}`));
    const unionKeys = unique([...Array.from(staticSet), ...Array.from(logSet)]);

    if (unionKeys.length > 0) {
      for (const key of unionKeys) {
        const [rootType, field] = key.split('.');
        const rootField: RootField = { rootType: rootType as RootField['rootType'], field };
        const inStatic = staticSet.has(key);
        const inLog = logSet.has(key);
        const source: EvidenceSource = inStatic && inLog ? 'both' : inLog ? 'log' : 'static';
        const confidence: Confidence =
          source === 'both' ? 'high' : source === 'log' ? 'medium-high' : 'medium';
        pushEvidence(test, rootField, confidence, 'hybrid-evidence', source);
      }
      continue;
    }

    const heuristicEvidence = inferFieldEvidence(test, operationCandidateFields);
    for (const item of heuristicEvidence) {
      pushEvidence(test, item.field, item.confidence, item.reason, item.source);
    }
  }

  const fields: FieldCoverage[] = schemaFields
    .map((rootField) => {
      const key = `${rootField.rootType}.${rootField.field}`;
      const supportingTests = (fieldSupport.get(key) ?? []).sort((a, b) => a.testId.localeCompare(b.testId));
      const facets = unique(supportingTests.flatMap((t) => t.facets));
      const projects = unique(supportingTests.map((t) => t.project));
      const status = toCoverageStatus(supportingTests.length, facets, rootField.rootType);
      return { ...rootField, status, projects, facets, supportingTests };
    })
    .sort((a, b) => `${a.rootType}.${a.field}`.localeCompare(`${b.rootType}.${b.field}`));

  const byRootType = {
    Query: { total: 0, covered: 0, partial: 0, missing: 0, percentCovered: 0 },
    Mutation: { total: 0, covered: 0, partial: 0, missing: 0, percentCovered: 0 },
    Subscription: { total: 0, covered: 0, partial: 0, missing: 0, percentCovered: 0 },
  } as CoverageReport['summary']['byRootType'];

  for (const field of fields) {
    const stats = byRootType[field.rootType];
    stats.total += 1;
    if (field.status === 'covered') stats.covered += 1;
    else if (field.status === 'partial') stats.partial += 1;
    else stats.missing += 1;
  }
  for (const rootType of ['Query', 'Mutation', 'Subscription'] as const) {
    const stats = byRootType[rootType];
    stats.percentCovered = stats.total === 0 ? 0 : (stats.covered / stats.total) * 100;
  }

  const summary = {
    totalFields: fields.length,
    coveredFields: fields.filter((f) => f.status === 'covered').length,
    partialFields: fields.filter((f) => f.status === 'partial').length,
    missingFields: fields.filter((f) => f.status === 'missing').length,
    percentCovered:
      fields.length === 0 ? 0 : (fields.filter((f) => f.status === 'covered').length / fields.length) * 100,
    byRootType,
    byArea: {} as CoverageReport['summary']['byArea'],
  };

  const areaByField = new Map<string, string>();
  for (const [areaName, fieldKeys] of Object.entries(fieldAreas)) {
    for (const key of fieldKeys) areaByField.set(key, areaName);
  }
  const allAreas = unique(['indexer-core', 'unclassified', ...Object.keys(fieldAreas)]);
  const byArea: CoverageReport['summary']['byArea'] = {};
  for (const area of allAreas) byArea[area] = { total: 0, covered: 0, partial: 0, missing: 0, percentCovered: 0 };
  const resolveArea = (field: RootField): string => {
    const key = `${field.rootType}.${field.field}`;
    const explicitArea = areaByField.get(key);
    if (explicitArea) return explicitArea;
    // Keep non-query APIs under indexer-core by default; query fields require explicit classification.
    if (field.rootType !== 'Query') return 'indexer-core';
    return 'unclassified';
  };
  for (const field of fields) {
    const area = resolveArea(field);
    const stats = byArea[area];
    stats.total += 1;
    if (field.status === 'covered') stats.covered += 1;
    else if (field.status === 'partial') stats.partial += 1;
    else stats.missing += 1;
  }
  for (const area of Object.keys(byArea)) {
    const stats = byArea[area];
    stats.percentCovered = stats.total === 0 ? 0 : (stats.covered / stats.total) * 100;
  }
  summary.byArea = byArea;
  const expandedSchema = buildExpandedSchema(schemaText);
  const gaps = fields
    .filter((f) => f.status === 'missing')
    .map((f) => ({ rootType: f.rootType, field: f.field, reason: 'no supporting tests detected' }));
  const gapsByArea: CoverageReport['gapsByArea'] = {};
  for (const area of Object.keys(byArea)) gapsByArea[area] = [];
  for (const gap of gaps) {
    const area = resolveArea(gap);
    gapsByArea[area].push(gap);
  }

  const report: CoverageReport = {
    metadata: {
      generatedAtUtc: new Date().toISOString(),
      branch,
      commitSha,
      schemaFingerprint: {
        path: toRelativeFromRepo(options.repoRoot, options.schemaPath),
        sha256: schemaHash,
      },
      testRunContext: {
        sourceResultsPath: toRelativeFromRepo(options.repoRoot, options.testResultsPath),
        targetEnv: options.targetEnv,
        indexerApiVersion: options.indexerApiVersion,
        totalSuites: vitest.numTotalTestSuites,
        totalTests: vitest.numTotalTests,
        detectedProjects: unique(tests.map((t) => t.project)).sort(),
        runSuccess: vitest.success,
      },
      output: {
        uniqueId,
        outputDir: outputDirRel,
      },
    },
    summary,
    fields,
    gaps,
    gapsByArea,
    diagnostics: {
      orphanOperationFields: operationCandidateFields
        .filter((f) => !testedFieldKeys.has(`${f.rootType}.${f.field}`))
        .sort((a, b) => `${a.rootType}.${a.field}`.localeCompare(`${b.rootType}.${b.field}`)),
      testedUnknownSchemaFields: Array.from(testedFieldKeys)
        .filter((key) => !knownFieldsMap.has(key))
        .map((key) => {
          const [rootType, field] = key.split('.');
          return { rootType: rootType as RootField['rootType'], field };
        }),
      schemaFieldsWithoutHelper: schemaFields
        .filter((f) => !operationFieldKeys.has(`${f.rootType}.${f.field}`))
        .sort((a, b) => `${a.rootType}.${a.field}`.localeCompare(`${b.rootType}.${b.field}`)),
    },
  };

  await writeJson(path.join(outputDir, 'coverage.json'), report);
  await writeText(path.join(outputDir, 'coverage.md'), toMarkdown(report));
  await writeText(
    path.join(outputDir, 'indexer-schema.md'),
    toSchemaMarkdown(
      toRelativeFromRepo(options.repoRoot, options.schemaPath),
      schemaHash,
      schemaFields,
      report.metadata.generatedAtUtc,
      uniqueId,
    ),
  );
  await writeText(
    path.join(outputDir, 'indexer-schema-expanded.md'),
    toSchemaExpandedMarkdown(
      report.metadata.generatedAtUtc,
      toRelativeFromRepo(options.repoRoot, options.schemaPath),
      schemaHash,
      uniqueId,
      expandedSchema,
    ),
  );
  await writeText(path.join(outputDir, 'report.html'), toHtml(report));
  await writeJson(path.join(options.outputDir, 'latest-run.json'), {
    uniqueId,
    outputDir: outputDirRel,
    generatedAtUtc: report.metadata.generatedAtUtc,
  });
  return { report, outputDir, uniqueId };
};
