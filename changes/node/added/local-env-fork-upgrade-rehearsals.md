#node #local-environment

# Add mainnet-shaped local fork upgrade rehearsals

Extends `local-environment/` so fork rehearsals can target `preview`,
`preprod`, and `mainnet` in addition to the earlier test networks. Adds a
two-phase `full-upgrade` command that rolls node images and then executes the
governance runtime-upgrade flow against the running fork, plus an
`--allow-same-version` escape hatch for local rehearsals where the candidate
runtime intentionally keeps the same `spec_version`.

Well-known network forks can now also reuse previously restored local snapshot
state on subsequent runs, while validating that the generated mock-authorities
artifacts and restored `data/` directories are still present before doing so.

PR: <add PR link after creating the PR>
