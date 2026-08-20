import path from 'path';
import type { RootField, VitestJsonResult } from './types.ts';
import { listFiles, readJson, readText } from './io.ts';

interface StaticMethodMap {
  methods: Record<string, RootField[]>;
}

export interface StaticEvidenceByTest {
  [testId: string]: RootField[];
}

const findMethodsInFile = (fileContent: string, methodNames: string[]): string[] =>
  methodNames.filter((method) => fileContent.includes(`.${method}(`));

export const readStaticEvidence = async (
  qaTestsRoot: string,
  methodMapPath: string,
  vitestResultsPath: string,
): Promise<StaticEvidenceByTest> => {
  const methodMap = await readJson<StaticMethodMap>(methodMapPath);
  const vitest = await readJson<VitestJsonResult>(vitestResultsPath);
  const methodNames = Object.keys(methodMap.methods);

  const testsRoot = path.join(qaTestsRoot, 'tests');
  const testFiles = (await listFiles(testsRoot)).filter((f) => f.endsWith('.test.ts'));
  const fileToFields = new Map<string, RootField[]>();

  for (const file of testFiles) {
    const content = await readText(file);
    const methods = findMethodsInFile(content, methodNames);
    const fields = methods.flatMap((m) => methodMap.methods[m] ?? []);
    if (fields.length === 0) continue;
    const rel = path.relative(path.resolve(qaTestsRoot, '..', '..'), file).replaceAll(path.sep, '/');
    fileToFields.set(rel, fields);
  }

  const result: StaticEvidenceByTest = {};
  for (const suite of vitest.testResults) {
    const relSuite = path
      .relative(path.resolve(qaTestsRoot, '..', '..'), suite.name)
      .replaceAll(path.sep, '/');
    const fields = fileToFields.get(relSuite) ?? [];
    if (fields.length === 0) continue;
    for (const assertion of suite.assertionResults) {
      const testId = `${relSuite}#${assertion.fullName}`;
      result[testId] = [...(result[testId] ?? []), ...fields];
    }
  }

  return result;
};
