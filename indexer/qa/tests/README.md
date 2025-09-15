# Midnight Indexer QA tests

A test suite for validating and experimenting with the Midnight Indexer component through its GraphQL API. 
This project provides a structured environment for running  smoke and integration tests, covering both GraphQL queries and subscriptions, against various target environments (including local/undeployed), supporting rapid development and testing for the Midnight Indexer component.

---

## 📦 Dependencies

- **Node.js**: v22 or higher
- **Yarn**: v3.6.x (already included in .yarn/releases)
- **Midnight Indexer**: 3.x and above

---

## 🚀 Getting Started

1. **Install dependencies**
   ```bash
   yarn install --immutable
   ```

2. **Run tests against a local Undeployed environment**

    Indexer can be executed locally (this is known as `undeployed` environment). The easiest way is through the compose file at the root of the repo. Note that the indexer can be executed as a single `standalone` docker container or using the `cloud` configuration, which is made up by a number of containers (including nats and postgres). Both docker compose profiles also spin up a Midnight node container, used as a main component dependency to feed data required by the indexer.
   ```bash
   cd ../..  # move to the root of the repo where docker-compose.yaml is located
   docker compose --profile cloud up -d
   ```
    Once you can see all the containers up, running and healthy, in another terminal run
   ```bash
   TARGET_ENV=undeployed yarn test
   ```

3. **Run tests against an existing deployed environment**

    There are a number of deployed environments that are used for testing components of the Midnight network. The are 
    - Devnet
    - QANet
    - Testnet02

    To execute the tests against these environments just change the TARGET_ENV variable accordingly (NOTE: use lower case for environment names)
   ```bash
   TARGET_ENV=devnet yarn test // for devnet
   TARGET_ENV=testnet02 yarn test // for testnet02
   ```


NOTE: Although all the known environments are supported, right now, it only makes sense to target `undeployed` or `devnet` environments. 
This is because we are using the latest Indexer 3.x API which has incompatible changes with respect to Indexer 2.x deployed.


## ✨ Features

- **Based on Vitest**: Uses Vitest as a modern, Typescript based, test framework core
- **Smoke Tests**: Health checks and schema validation for GraphQL endpoints.
- **Basic Integration Tests**: Fine-grained GraphQL query and subscription tests for blocks, transactions, and contract actions.
- **Custom Reporters**: JUnit-compatible output for CI integration.

- **Improved Logging**: Configurable logging for debugging and test traceability.

---

## 🛠️ Future Developments, Improvments & Test Ideas

- **Contract actions**: Expand test coverage to include missing contract actions.
- **Advanced Integration tests**: Expand test coverage with the usage of Node Toolkit.
- **Test containers support**: Add support for Test Container to add better fine-grained control over the indexer sub-components
- **Add Tooling for Test Data Scraping**: Tools for generating synthetic blocks, transactions, and keys.
- **GraphQL Schema Fuzzing**: Randomized query/subscription request schema with corresponding validation 
- **Dynamic Data Fetching**: Use the block scraper to fetch recent block data to execute the test against (potentially) different test data every run 
- **Log file per test**: Right now the test execution is per test file, having log files per test will allow concurrent test execution.
