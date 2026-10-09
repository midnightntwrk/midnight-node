# AURA → BABE local-env checks

Read-only checks for rehearsing the [AURA → BABE migration](../../../../docs/aura-to-babe-migration-runbook.md)
on local-env, plus fish helpers to roll nodes one at a time. They reuse `createApi` from
`src/lib/runtimeUpgradeUtils.ts` and run with `ts-node` from `local-environment/`.

```fish
cd local-environment
source src/scripts/babe/fns.fish   # bstate, babe <script>, roll <n> [image], savelogs <tag>
```

| Script       | Runbook phase    | Usage (`babe <script> …`) | Checks                                                                                                                           |
| ------------ | ---------------- | ------------------------- | -------------------------------------------------------------------------------------------------------------------------------- |
| `state`      | 1.1, 2.1, 3, 4   | `state [ws]`              | spec, committee storage version, `engineState`, `babe.genesisSlot`, `babe.authorities`, best/finalized                           |
| `digests`    | all              | `digests [ws] [N]`        | pre-runtime / consensus / seal digests of the last N blocks                                                                      |
| `activation` | 2.1              | `activation [ws]`         | `setCode` and activation block                                                                                                   |
| `mirror`     | 1.2–3            | `mirror <ws> <from> <to>` | dual-digest window: BABE `SecondaryPlain` after AURA, same slot, index = AURA author                                             |
| `flip`       | 4                | `flip <ws> <from> <to>`   | slots, digests and `engineState` around the flip                                                                                 |
| `forks`      | 4 (steady state) | `forks <ws> [seconds]`    | all imported heads per slot: `P+P` = primary collisions (≈3.4 % at c = 1/4), `P+Sv` = secondary orphaned by a primary (expected) |

`ws` defaults to `ws://localhost:9933` (node-1). Point `mirror` / `flip` / `forks` at the archive node
`ws://localhost:9945` — it keeps non-canonical blocks.

`roll` recreates one node with another image (default `$V3`) using the same env as the CLI
(`.env.default` + generated secrets). Don't run `governance-runtime-upgrade` or
`consensus-upgrade-schedule-flip` without `--skip-run` mid-rollout: their compose bring-up resets
every node to `MIDNIGHT_NODE_IMAGE`.
