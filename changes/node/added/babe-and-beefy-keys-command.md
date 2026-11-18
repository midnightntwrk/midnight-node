#node #keystore #babe #beefy

# Add `babe-and-beefy-keys` command for operators

New `midnight-node babe-and-beefy-keys` subcommand ensures the node keystore
holds BABE and BEEFY keys ahead of the release that needs them. It resolves the
keystore like `key insert` (`--base-path`, `--chain`, `--keystore-path`,
keystore password options).

- Requires the cross-chain (`crch`) key; if it is missing, the command prints
  the resolved keystore path and exits with an error without writing anything.
- A missing BEEFY (`beef`) key is inserted with the cross-chain seed.
- A missing BABE (`babe`) key is generated and inserted.
- Existing BEEFY/BABE keys are kept, so the command is safe to re-run.

Output is split into PRIVATE (seeds, stored in the keystore) and PUBLIC
(public keys to send to Midnight governance) sections.

PR:
Issue:
