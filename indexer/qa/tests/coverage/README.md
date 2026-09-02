# QA API Feature Coverage

This folder contains an Xray-independent API feature coverage toolchain for `qa/tests`.

The goal is to measure GraphQL API feature coverage against the live schema surface:
- root fields in `Query`, `Mutation`, and `Subscription`
- scenario facets (positive/negative/schema validation/edge case/streaming)
- project coverage (`smoke`, `integration`, `e2e`, and future additions)

## Canonical Inputs

- Schema: `indexer-api/graphql/schema-v4.graphql`
- GraphQL operation helpers: `qa/tests/utils/indexer/graphql`
- Vitest execution results: `qa/tests/reports/json/test-results.json`

## Identity Model

- `testId = <relativeTestFile>#<fullTestName>`
- `externalId` is optional and reserved for future integrations

## Coverage Taxonomy

- Root type: `Query | Mutation | Subscription`
- Coverage status: `covered | partial | missing`
- Facets:
  - `positive`
  - `negative`
  - `schemaValidation`
  - `edgeCase`
  - `streaming`

## Required Metadata In Both Reports

The generator writes these metadata fields to both `coverage.json` and `coverage.md`:
- UTC timestamp
- branch name
- commit SHA
- schema fingerprint (sha256 + schema path)
- test run context (results source, detected projects, totals)

## Output Artifacts

Default output directory: `qa/tests/coverage/output/<unique-id>`

Unique id format:
- `<schemaShortHash>-<commitShortSha>-<utcCompact>`
- `utcCompact` uses `YYYYMMDDHHMMSS`

- `coverage.json`: machine-readable baseline and CI gating source
- `coverage.md`: human-readable report for PRs and manual review
- `indexer-schema.md`: snapshot of current schema items used by the run
- `indexer-schema-expanded.md`: expanded baseline call-shape view grouped by operation type (`Query`, `Mutation`, `Subscription`), including argument variants that respect required args and oneOf constraints
- `report.html`: graphical summary and per-field evidence/source view
- `latest-run.json` (in `coverage/output/`): pointer to the most recent unique-id output directory

## Commands

Run from `qa/tests`:

- Generate report:
  - `bun run coverage:generate`
- Generate report with explicit context:
  - `bun run coverage:generate --target-env=preview --indexer-api-version=v4`
- Run soft gate check (regression + optional critical checks):
  - `bun run coverage:check`

## Configuration

- `config/facet-keywords.json`: keyword classification fallback for facet detection
- `config/critical-operations.json`: critical API fields and required evidence
- `config/check-thresholds.json`: soft-gate thresholds and baseline behavior
- `config/mapping-overrides.json`: explicit test-to-field mapping overrides
- `config/static-method-map.json`: static method-to-schema-field mappings for hybrid evidence
- `config/field-areas.json`: schema field partitioning (e.g. `indexer-core` vs `spo-api`)
  - Query fields that are not explicitly assigned are reported under `unclassified` (to avoid accidental inflation of core coverage).

## Notes On Evolution

- Project groups are discovered from test file paths and are not hardcoded to only smoke/integration/e2e.
- If API schema path/version changes, update schema path in command flags or defaults.
- Keep overrides minimal: prefer inferred mappings and use overrides for ambiguous tests only.

## Coverage Health Checklist

For QA/API PR reviews:
- Does `coverage.json` include current branch/SHA/timestamp metadata?
- Are new schema root fields represented in operation helpers or explicitly documented as out-of-scope?
- Did critical operations retain required facets and project coverage?
- Are any low-confidence mappings newly introduced and should they move to overrides?
- If a deliberate gap is introduced, is it tracked in PR notes and baseline strategy?
