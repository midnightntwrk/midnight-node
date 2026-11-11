#node #ledger #toolkit
# Bump ledger 9 to 9.1.0.2-rc.0

Ledger 9 moves to 9.1.0.2-rc.0. Three crates change version and none is on crates.io yet,
so each is pinned to its per-crate tag in the public ledger repo:

| Crate                       | From (crates.io) | To    | Source                                                |
| --------------------------- | ---------------- | ----- | ----------------------------------------------------- |
| `midnight-ledger-v9`        | 1.0.1            | 1.0.2 | ledger tag `crate-ledger-9.1.0.2-rc.0` (`bfd7adff`)   |
| `midnight-storage`          | 3.0.0            | 3.1.0 | ledger tag `storage-3.1.0-rc.0` (`4586496a`)          |
| `midnight-transient-crypto` | 3.0.1            | 3.1.0 | ledger tag `transient-crypto-3.1.0-rc.0` (`61f1a4be`) |

`midnight-zkir` 3.2.0 moves from crates.io to the `zkir-3.2.0` tag of `midnight-zkir`
(`446c7fec`), because the ledger crate now names it as a git dependency; its source is
identical to the registry crate. Every other ledger-family crate stays on crates.io, and
ledger 8 stays on 8.1.3. `scoped-tls` 1.0.1 is a new dependency of `midnight-storage`.

What the ledger change contains:

- Transaction construction partitions transcripts against a mock transaction's size and
  time-to-dismiss, takes an expected `TransactionShape`, and rehearses calls with the widest
  commitment-tree indices. Client-side construction code, used by the toolkit.
- `midnight-storage` gains `ALLOW_SHORT_EXTENSION_CHAINS`, a thread-local that lets clients
  load tries holding an extension shorter than 255 nibbles with an extension child. The node
  never sets it: under it a node would accept wire data other nodes reject. Without it the
  trie canonicity check is unchanged.
- `PureGeneratorPedersen::smallest_representable` is added to `midnight-transient-crypto`.

Downgrade check: each new tag descends from the commit the previous crates.io crate was
published from (`git merge-base --is-ancestor`), and a `diff -r` of each crate's `src/`
against the previous registry crate shows no fix removed - the one guard touched, the trie
extension-chain check, is kept unless the client-only flag is set.

PR: TBD
