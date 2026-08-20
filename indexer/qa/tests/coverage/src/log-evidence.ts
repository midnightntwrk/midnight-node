import path from 'path';
import { parse, Kind } from 'graphql';
import type { RootField } from './types.ts';
import { listFiles, pathExists, readText } from './io.ts';

export interface LogEvidenceByTest {
  [testId: string]: RootField[];
}
export interface KnownTestsByFile {
  [testFile: string]: string[];
}

const TEST_START_RE = /^STARTED TEST:\s(.+)$/m;
const TEST_END_RE = /^TEST COMPLETED:\s(.+)$/m;
const LOG_HEADER_RE = /^(?:\[[^\]]+\]\s+)?[A-Z]+\s*:\s?(.*)$/;

const extractRootFieldsFromGraphQL = (queryText: string): RootField[] => {
  const trimmed = queryText.trim();
  if (!trimmed) return [];
  try {
    const doc = parse(trimmed);
    const fields: RootField[] = [];
    for (const def of doc.definitions) {
      if (def.kind !== Kind.OPERATION_DEFINITION) continue;
      const rootType =
        def.operation === 'query'
          ? 'Query'
          : def.operation === 'mutation'
            ? 'Mutation'
            : 'Subscription';
      for (const selection of def.selectionSet.selections) {
        if (selection.kind !== Kind.FIELD) continue;
        fields.push({ rootType, field: selection.name.value });
      }
    }
    return fields;
  } catch {
    return [];
  }
};

const extractGraphQLFromMessage = (msg: string): RootField[] => {
  const markers = ['Using query', 'subscription query:'];
  const marker = markers.find((m) => msg.includes(m));
  if (!marker) return [];
  const idx = msg.indexOf('\n');
  if (idx === -1 || idx === msg.length - 1) return [];
  const queryText = msg.slice(idx + 1);
  return extractRootFieldsFromGraphQL(queryText);
};

const normalizeName = (value: string): string =>
  value
    .toLowerCase()
    .replace(/[^\w\s]/g, '')
    .replace(/\s+/g, ' ')
    .trim();

const parseLogFile = (
  content: string,
  testFileRelPath: string,
  knownTestsForFile: string[],
): LogEvidenceByTest => {
  const lines = content.split('\n');
  const entries: string[] = [];
  let current = '';
  for (const line of lines) {
    const m = LOG_HEADER_RE.exec(line);
    if (m) {
      if (current.trim()) entries.push(current.trim());
      current = m[1] ?? '';
      continue;
    }
    if (current.length > 0) {
      current += `\n${line}`;
    }
  }
  if (current.trim()) entries.push(current.trim());
  const evidence: LogEvidenceByTest = {};
  const normalizedTestMap = new Map(knownTestsForFile.map((t) => [normalizeName(t), t]));
  let activeTestName: string | null = null;

  for (const entry of entries) {
    const message = entry;

    const start = TEST_START_RE.exec(message);
    if (start) {
      const raw = start[1].trim();
      const matched = normalizedTestMap.get(normalizeName(raw)) ?? null;
      activeTestName = matched;
      continue;
    }
    const end = TEST_END_RE.exec(message);
    if (end) {
      activeTestName = null;
      continue;
    }

    if (!activeTestName) continue;
    const fields = extractGraphQLFromMessage(message);
    if (fields.length === 0) continue;
    const testId = `${testFileRelPath}#${activeTestName}`;
    evidence[testId] = [...(evidence[testId] ?? []), ...fields];
  }
  return evidence;
};

export const readLogEvidence = async (
  qaTestsRoot: string,
  knownTestsByFile: KnownTestsByFile,
): Promise<LogEvidenceByTest> => {
  const sessionPathFile = path.join(qaTestsRoot, 'logs/sessionPath');
  if (!(await pathExists(sessionPathFile))) return {};
  const sessionDir = (await readText(sessionPathFile)).trim();
  if (!sessionDir) return {};
  if (!(await pathExists(sessionDir))) return {};

  const testFiles = (await listFiles(path.join(qaTestsRoot, 'tests'))).filter((f) => f.endsWith('.test.ts'));
  const basenameToRelative = new Map<string, string>();
  for (const testFile of testFiles) {
    const key = path.basename(testFile).replace(/\.test\.ts$/, '');
    const rel = path.relative(path.resolve(qaTestsRoot, '..', '..'), testFile).replaceAll(path.sep, '/');
    basenameToRelative.set(key, rel);
  }

  const files = (await listFiles(sessionDir)).filter((f) => f.endsWith('.log'));
  const result: LogEvidenceByTest = {};
  for (const file of files) {
    const basename = path.basename(file);
    const logBaseName = basename.replace(/\.test\.log$/, '');
    const testFileRelPath = basenameToRelative.get(logBaseName) ?? null;
    if (!testFileRelPath) continue;
    const knownTestsForFile = knownTestsByFile[testFileRelPath] ?? [];
    if (knownTestsForFile.length === 0) continue;
    const content = await readText(file);
    const parsed = parseLogFile(content, testFileRelPath, knownTestsForFile);
    for (const [testId, fields] of Object.entries(parsed)) {
      result[testId] = [...(result[testId] ?? []), ...fields];
    }
  }
  return result;
};
