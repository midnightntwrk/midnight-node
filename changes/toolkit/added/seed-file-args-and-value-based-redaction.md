#toolkit #security
# File-based secret args for generate-intent and value-based log redaction

`generate-intent` deploy/maintain now accept secrets from files, mirroring the
node's `*_seed_file` options, so key material stays off the command line
(argv is world-readable via `ps` and captured by shells and CI logs):

- `deploy --authority-seed-file <path>` (alternative to `--authority-seed`)
- `maintain --signing-file <path>` (alternative to `--signing`)
- `maintain contract --new-authority-file <path>` (alternative to `--new-authority`)

Files are read via the hardened reader (regular-file check, 4 KiB cap;
symlinks allowed for Kubernetes secret mounts), trimmed, and intermediate
buffers zeroized.

Also fixes a debug-log key disclosure: redaction of child-process arguments
was keyed on flag names, which missed the maintain-contract new-authority
seed passed positionally. Redaction is now value-based - any argument equal
to a known secret is replaced with `[REDACTED]` - with a regression test.

Secrets are kept out of the toolkit-js child's argv too: the parent writes
them to owner-only (0600) temp files, deleted when the child exits, and
`bin.ts` expands `--signing-file`/`--new-authority-file` into its in-memory
argv only - `/proc/<pid>/cmdline` is a snapshot taken at exec, so the child's
process table entry never contains key material either. The signing-key CLI
fields are also `Debug`-redacted (`SecretString`) so `--dry-run` info logs
can't leak them.

PR:
