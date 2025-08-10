# Ledger - Conditional compilation

Compiling the ledger using the `HARDFORK_TEST` environment variable will change public exports to use the `hardfork_test` version of the ledger. The exports that are affected are:

```
midnight_node_ledger::types::active_version
midnight_node_ledger::types::active_ledger_bridge
```
