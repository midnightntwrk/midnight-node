# Smoke Tests

## Overview

Smoke tests are quick, high-level health checks designed to validate that the Midnight Indexer is operational and accessible. These tests focus on ensuring the basic functionality of the GraphQL endpoints without performing deep integration testing.

## Test Scope

- **Service Health Checks**: Verifies that the indexer HTTP endpoints are responding correctly
- **GraphQL Endpoint Validation**: Ensures the GraphQL API is accessible via both HTTP and WebSocket channels
- **Schema Introspection**: Validates that the GraphQL schema can be retrieved and matches expected structure
- **Query Depth Limits**: Confirms that the API properly enforces query complexity restrictions

## When to Run

Smoke tests should be executed:
- After deploying or starting the indexer
- As a first step before running more comprehensive test suites
- In CI/CD pipelines as a quick sanity check
- When validating a new environment setup

## Execution

```bash
# Run only smoke tests
TARGET_ENV=undeployed bun run test:smoke
```

These tests run quickly (typically under 1 second) and do not require any pre-seeded data or cache warmup.
