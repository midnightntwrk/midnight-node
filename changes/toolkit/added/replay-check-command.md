#toolkit #replay #security
# `replay-check` subcommand: scan chain history with predicates

New `replay-check` toolkit subcommand that replays a chain from any source
(RPC or file, reusing the standard `Source` args and fetch cache) and tests
every block, its intermediate pre/post ledger states, and its individual
decoded transactions against a registry of per-ledger-version predicates —
so that known, detectable vulnerabilities can be scanned for across real
chain history.

- Generic `Predicate` trait + `observe_blocks` driver compiled per ledger
  generation (7/8/9) via the existing `fork` common-module mechanism; the
  replay crosses the 7→8 fork boundary faithfully.
- Per-version predicate registries: ledger 8 ships two illustrative example
  predicates (a block-level state invariant and a transaction-level check);
  ledger 7/9 registries are wired but empty.
- `--predicate <substr>` name filter, `--fail-fast`, `--list-predicates`,
  `--from-block`/`--to-block` bounds, `--json` report output.
- `--watch`: after the initial sync, keep following the chain tip — poll for
  newly finalized blocks and run the predicates on each as it arrives
  (crossing ledger-version forks at the tip too). New violations are logged
  immediately; Ctrl-C prints the accumulated report. Transient RPC failures
  are retried with a fresh connection indefinitely.
- Exits non-zero when any violation is found.

PR: TBD
