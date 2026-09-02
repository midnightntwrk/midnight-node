# Coverage Data Contracts

This document defines the stable contracts for coverage output artifacts.

## `coverage.json`

Top-level fields:
- `metadata`
- `summary`
- `fields`
- `gaps`
- `gapsByArea`
- `diagnostics`

### `metadata`
- `generatedAtUtc`: ISO-8601 timestamp in UTC
- `branch`: active git branch
- `commitSha`: git commit SHA
- `schemaFingerprint`:
  - `path`: schema path relative to repository root
  - `sha256`: SHA-256 hash of schema contents
- `testRunContext`:
  - `sourceResultsPath`
  - `targetEnv` (optional)
  - `indexerApiVersion` (optional)
  - `totalSuites`
  - `totalTests`
  - `detectedProjects` (dynamic list)
  - `runSuccess`
- `output`:
  - `uniqueId`
  - `outputDir`

### `summary`
- `totalFields`
- `coveredFields`
- `partialFields`
- `missingFields`
- `percentCovered`
- `byRootType`: per `Query`, `Mutation`, `Subscription`
- `byArea`: partitioned stats (e.g. `indexer-core`, `spo-api`)

### `fields[]`
- `rootType`
- `field`
- `status` (`covered|partial|missing`)
- `projects[]`
- `facets[]`
- `supportingTests[]`:
  - `testId`, `testFile`, `fullName`, `status`, `project`, `facets[]`
  - optional: `externalId`
  - `confidence`
  - `evidenceReason`
  - `evidenceSource` (`static|log|both|heuristic|override`)

### `gaps[]`
- `rootType`
- `field`
- `reason`

### `gapsByArea`
- area-keyed missing coverage details (e.g. `indexer-core`, `spo-api`, `unclassified`)
- each entry contains:
  - `rootType`
  - `field`
  - `reason`

### `diagnostics`
- `orphanOperationFields[]`: helper-backed root fields without detected test evidence
- `testedUnknownSchemaFields[]`: evidence mapped to fields not present in schema
- `schemaFieldsWithoutHelper[]`: schema root fields without any helper operation

## `coverage.md`

Must include:
- metadata block mirroring required metadata fields
- summary totals and by-root-type stats
- missing field list
- per-field status rows

The markdown report is human-readable and intentionally denormalized from `coverage.json`.

## Additional Schema Artifacts

- `indexer-schema.md`: compact schema snapshot
- `indexer-schema-expanded.md`: expanded schema call-shape baseline grouped by `Query`/`Mutation`/`Subscription`, listing valid argument variants
  - required arguments are always present in listed variants
  - optional arguments include empty-call variants when allowed
  - oneOf input objects expand into mutually exclusive variants

## Run Pointer Artifact

- `coverage/output/latest-run.json` stores:
  - `uniqueId`
  - `outputDir` (repo-relative path to the unique-id directory)
  - `generatedAtUtc`
