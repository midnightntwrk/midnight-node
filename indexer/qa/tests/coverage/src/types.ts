export type RootType = 'Query' | 'Mutation' | 'Subscription';
export type CoverageStatus = 'covered' | 'partial' | 'missing';
export type Confidence = 'high' | 'medium-high' | 'medium' | 'low';
export type Facet = 'positive' | 'negative' | 'schemaValidation' | 'edgeCase' | 'streaming';
export type EvidenceSource = 'static' | 'log' | 'both' | 'heuristic' | 'override';

export interface RootField {
  rootType: RootType;
  field: string;
}

export interface OperationFieldUsage extends RootField {
  operationName: string;
  operationType: 'query' | 'mutation' | 'subscription';
  sourceFile: string;
  exportName: string;
}

export interface TestEvidence {
  testId: string;
  testFile: string;
  fullName: string;
  status: 'passed' | 'failed' | 'skipped' | 'todo' | 'unknown';
  project: string;
  facets: Facet[];
  evidenceSource?: EvidenceSource;
  externalId?: string;
}

export interface FieldCoverage {
  rootType: RootType;
  field: string;
  status: CoverageStatus;
  projects: string[];
  facets: Facet[];
  supportingTests: Array<
    TestEvidence & {
      confidence: Confidence;
      evidenceReason: string;
      evidenceSource: EvidenceSource;
    }
  >;
}

export interface CoverageMetadata {
  generatedAtUtc: string;
  branch: string;
  commitSha: string;
  schemaFingerprint: {
    path: string;
    sha256: string;
  };
  testRunContext: {
    sourceResultsPath: string;
    targetEnv?: string;
    indexerApiVersion?: string;
    totalSuites: number;
    totalTests: number;
    detectedProjects: string[];
    runSuccess: boolean;
  };
  output: {
    uniqueId: string;
    outputDir: string;
  };
}

export interface CoverageReport {
  metadata: CoverageMetadata;
  summary: {
    totalFields: number;
    coveredFields: number;
    partialFields: number;
    missingFields: number;
    percentCovered: number;
    byRootType: Record<
      RootType,
      {
        total: number;
        covered: number;
        partial: number;
        missing: number;
        percentCovered: number;
      }
    >;
    byArea: Record<
      string,
      {
        total: number;
        covered: number;
        partial: number;
        missing: number;
        percentCovered: number;
      }
    >;
  };
  fields: FieldCoverage[];
  gaps: Array<RootField & { reason: string }>;
  gapsByArea: Record<string, Array<RootField & { reason: string }>>;
  diagnostics: {
    orphanOperationFields: RootField[];
    testedUnknownSchemaFields: RootField[];
    schemaFieldsWithoutHelper: RootField[];
  };
}

export interface MappingOverride {
  testId: string;
  fields: RootField[];
  confidence?: Confidence;
  reason?: string;
}

export interface MappingOverridesFile {
  overrides: MappingOverride[];
}

export interface CriticalFieldRule extends RootField {
  requiredFacets: Facet[];
  minimumProjects: number;
}

export interface VitestAssertionResult {
  fullName: string;
  title: string;
  status: string;
  ancestorTitles?: string[];
  meta?: Record<string, unknown>;
}

export interface VitestSuiteResult {
  name: string;
  assertionResults: VitestAssertionResult[];
}

export interface VitestJsonResult {
  numTotalTestSuites: number;
  numTotalTests: number;
  success: boolean;
  testResults: VitestSuiteResult[];
}
