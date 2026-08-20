import path from 'path';
import { parse, Kind } from 'graphql';
import type { MappingOverridesFile, OperationFieldUsage, RootField, RootType } from './types.ts';
import { listFiles, pathExists, readJson, readText } from './io.ts';
import { toRelativeFromRepo } from './utils.ts';

const OP_RE = /export const\s+([A-Za-z0-9_]+)\s*=\s*`([\s\S]*?)`;/gm;
const INTERPOLATION_RE = /\$\{[^}]+\}/g;

export const readSchemaRootFields = async (schemaPath: string): Promise<RootField[]> => {
  const schemaText = await readText(schemaPath);
  const document = parse(schemaText);
  const fields: RootField[] = [];
  for (const definition of document.definitions) {
    if (definition.kind !== Kind.OBJECT_TYPE_DEFINITION) continue;
    const typeName = definition.name.value as RootType | string;
    if (!['Query', 'Mutation', 'Subscription'].includes(typeName)) continue;
    for (const field of definition.fields ?? []) {
      fields.push({ rootType: typeName as RootType, field: field.name.value });
    }
  }
  return fields;
};

const rootTypeFromOperationType = (operationType: 'query' | 'mutation' | 'subscription'): RootType =>
  operationType === 'query' ? 'Query' : operationType === 'mutation' ? 'Mutation' : 'Subscription';

export const readOperationFieldUsage = async (
  repoRoot: string,
  operationDirPath: string,
): Promise<OperationFieldUsage[]> => {
  const allFiles = (await listFiles(operationDirPath)).filter((f) => f.endsWith('.ts'));
  const usages: OperationFieldUsage[] = [];

  for (const filePath of allFiles) {
    const source = await readText(filePath);
    const rel = toRelativeFromRepo(repoRoot, filePath);
    const matches = source.matchAll(OP_RE);
    for (const match of matches) {
      const exportName = match[1];
      const templateText = match[2].replace(INTERPOLATION_RE, ' ');
      let doc;
      try {
        doc = parse(templateText);
      } catch {
        continue;
      }
      for (const def of doc.definitions) {
        if (def.kind !== Kind.OPERATION_DEFINITION) continue;
        const opType = def.operation;
        const rootType = rootTypeFromOperationType(opType);
        const operationName = def.name?.value ?? exportName;
        for (const selection of def.selectionSet.selections) {
          if (selection.kind !== Kind.FIELD) continue;
          usages.push({
            rootType,
            field: selection.name.value,
            operationType: opType,
            operationName,
            sourceFile: rel,
            exportName,
          });
        }
      }
    }
  }

  return usages;
};

export const readMappingOverrides = async (configPath: string): Promise<MappingOverridesFile> => {
  if (!(await pathExists(configPath))) return { overrides: [] };
  return readJson<MappingOverridesFile>(configPath);
};

export const inferProjectFromTestFile = (testFile: string): string => {
  const normalized = testFile.replaceAll(path.sep, '/');
  const marker = '/tests/';
  const idx = normalized.lastIndexOf(marker);
  if (idx === -1) return 'unknown';
  const remainder = normalized.slice(idx + marker.length);
  const project = remainder.split('/')[0];
  return project || 'unknown';
};
