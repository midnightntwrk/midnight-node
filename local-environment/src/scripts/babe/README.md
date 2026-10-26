# AURA → BABE local-env checks

Read-only checks for rehearsing the [AURA → BABE migration](../../../../docs/aura-to-babe-migration-runbook.md)
on local-env, plus fish helpers to roll nodes one at a time. They reuse `createApi` from
`src/lib/runtimeUpgradeUtils.ts` and run with `ts-node` from `local-environment/`.

```fish
cd local-environment
source src/scripts/babe/fns.fish   # bstate, babe <script>, roll <n> [image], syncnode full|warp [down], savelogs <tag>
```

| Script       | Runbook phase    | Usage (`babe <script> …`)                   | Checks                                                                                                                                                                                                                                                                  |
| ------------ | ---------------- | ------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `state`      | 1.1, 2.1, 3, 4   | `state [ws]`                                | spec, committee storage version, `engineState`, `babe.genesisSlot`, `babe.authorities`, best/finalized                                                                                                                                                                  |
| `digests`    | all              | `digests [ws] [N]`                          | pre-runtime / consensus / seal digests of the last N blocks                                                                                                                                                                                                             |
| `activation` | 2.1              | `activation [ws]`                           | `setCode` and activation block                                                                                                                                                                                                                                          |
| `mirror`     | 1.2–3            | `mirror <ws> <from> <to>`                   | dual-digest window: BABE `SecondaryPlain` after AURA, same slot, index = AURA author                                                                                                                                                                                    |
| `flip`       | 4                | `flip <ws> <from> <to>`                     | slots, digests and `engineState` around the flip                                                                                                                                                                                                                        |
| `postpone`   | 3–4 (M8)         | `postpone [ws]`                             | schedules the flip with the motion closed in an epoch's last block, stops that next epoch's last-slot author, restarts it after the slot; logs each block to the flip. Needs `Aura` + BABE authorities                                                                  |
| `halt`       | all (E2)         | `halt <ws> <seconds> [epochs] [--schedule]` | stops validators 1–5 for `<seconds>`, restarts them; per block: session, current/queued/next committee epochs, `babe.epochIndex`, `engineState`. Expect one catch-up rotation on the first block back. `--schedule` closes a `scheduleFlip` motion just before the halt |
| `forks`      | 4 (steady state) | `forks <ws> [seconds]`                      | all imported heads per slot: `P+P` = primary collisions (≈3.4 % at c = 1/4), `P+Sv` = secondary orphaned by a primary (expected)                                                                                                                                        |
| `seals`      | 4 (2.8)          | `seals <ws> <from> <to>`                    | recovers each block's seal signer, compares it with the claimed authority: BABE `babe.authorities[idx]` at the block, AURA `slot % n` at the parent; exits 1 on a mismatch                                                                                              |

`ws` defaults to `ws://localhost:9933` (node-1). Point `mirror` / `flip` / `forks` at the archive node
`ws://localhost:9945` — it keeps non-canonical blocks.

`roll` recreates one node with another image (default `$V3`) using the same env as the CLI
(`.env.default` + generated secrets). Don't run `governance-runtime-upgrade` or
`consensus-upgrade-schedule-flip` without `--skip-run` mid-rollout: their compose bring-up resets
every node to `MIDNIGHT_NODE_IMAGE`.
