---
name: comment-cleanup
description: Prune and tighten code comments and docstrings without touching code. Use when the user asks to clean up, trim, prune, audit or review comments; says comments are too verbose, noisy, or read like an AI wrote them; or wants a comment pass over a file, diff or PR before committing. Also runs when invoked by name ("comment-cleanup", "/comment-cleanup").
---

# Comment cleanup

Go through the comments in the target, delete the ones that add nothing, tighten the
ones that stay, and report anything you saw but were not allowed to fix.

## Ground rules

- Change **only comments and docstrings**. All executable code must stay byte-identical.
  The one exception: delete a blank line that only existed to separate a comment you removed.
- Never rename, refactor, reorder or "fix" code, even when a comment exists only because
  the code is hard to read. Keep that comment and flag the code as a refactor candidate.
- Do not add comments where there were none. Do not impose capitalisation or punctuation style.
- Scope:
  - Files: every comment in them.
  - Staged/unstaged diff or PR: only comments inside changed hunks.
- Check before finishing: `git diff` (or the equivalent) must show nothing but comment
  and blank-line changes. If anything else shows up, revert it.

## What earns a comment

A comment stays if it tells the reader something the code cannot: why it is written this
way, a constraint, a hazard, a workaround, or why the obvious alternative fails.

When you are unsure:
- about a comment that gives a **reason** → keep it.
- about a comment that **describes** the code → delete it.

A lost warning costs far more than a mediocre surviving comment.

## Decide each comment

Walk every in-scope comment through these checks in order and stop at the first match.

1. **Narrates the code** (repeats the next line, lists steps, echoes names or types,
   labels a closing brace) → delete.
2. **Talks to the reviewer, not the reader** ("now uses the pool", "removed the retry",
   "per review", "fixed off-by-one") → delete. Test: does it make sense to someone opening
   this file cold in three years? If not, it is change history and belongs in the commit.
3. **False** (the code no longer does what it says) → rewrite it to be true if the point
   still matters; otherwise delete.
4. **Leans on an unstable reference** (see References) → state the actual fact inline, or delete.
5. **Valid reason, poorly packaged** (wordy, a wall of text, misplaced) → tighten, split or
   move (see Tightening).
6. **Anything else** → leave it exactly as it is.

Rewording a comment that is already correct and minimal only creates diff noise; don't.
"Correct but wordy" is not minimal, so it goes to step 5.

## References

Ask: can the target change while the comment still points at it?

| Stable, fine to cite | Unstable, don't cite |
|---|---|
| Ticket/issue IDs and links | "the spec", "the requirements" with no address |
| Permalinks, wiki pages at permanent URLs | Design docs (they get superseded) |
| Docs at a fixed path in this repo | Section/heading numbers in any editable document |
| Published, versioned standards, section numbers included (`RFC 9110 §8.3`, `ISO 8601`, `EIP-1559`, `CIP-30`) | |

- A citation backs up the content; it is never the content. Every surviving comment must
  still be useful with its link deleted.
  - `// see PAY-88` → `// refunds over the original charge are rejected by the processor (PAY-88)`
- If the code needed no comment in the first place (e.g. "conforms to the spec"), delete it
  rather than rewriting it.
- Apply this inside docstrings too.

## Tightening

Keeping a comment and keeping its wording are separate decisions. For every survivor:

- Cut it down to the one fact the reader needs at that line, in the fewest words that
  carry it. Drop what the code already shows, knock-on consequences, restated names, and
  context the reader already has. Sometimes the right size is zero.
- Break a long header block that covers several points into short comments, each placed
  beside the lines it governs.
- System-level narrative (architecture rationale, benchmark numbers, lifecycle overviews):
  - A stable doc already covers it → delete it inline. No "see the doc" pointer.
  - No doc covers it → keep it and flag it as a candidate to move into a doc. Do not write the doc.
  - Inline, keep only the local constraint ("must stay under the health-check interval",
    "keep in sync with X"). A comment that is only a pointer to a doc gets deleted.
- Length is fine when every part carries weight, e.g. a workaround, the bug behind it, and
  when it can go. Never shorten by dropping information.

Example: a six-line block above a startup hook explaining the service's boot sequence,
the history of a past outage, and that this hook must run before the connection pool opens
becomes:

```python
# must run before the connection pool opens
```

## Protected: looks deletable, keep it

- **Sync links:** "mirrors X", "same rule as Y", "keep in sync with Z". This is what stops
  copies drifting apart.
- **Meaning of literals:** units, ordering, encoding. `RETRY_DELAY = 250  # ms`,
  `origin = (51.5, -0.12)  # (lat, lon)`, `HEADER = 0x0A0B  # big-endian`.
- **Output contracts:** formats an outside consumer depends on
  ("column order matches the billing import").
- **Section dividers** (`// ===== Serialisation =====`): navigation aids. Keep unless the
  user says otherwise; any rationale inside them is protected like any other.

## TODO / FIXME

- Keep by default. An issue ID is not required.
- The work is visibly already done → the TODO is false → delete.
- The TODO describes work that plausibly belongs in the current change (e.g.
  `# TODO: handle timeouts` above a bare HTTP call) → keep and flag it. Deleting it would
  hide unfinished work.

## Docstrings

Everything above applies, plus:

- Only restates the name and parameter types → delete or cut down.
- States a contract (units, valid ranges, side effects, failure behaviour, invariants,
  preconditions, what has already been done to the inputs) → keep. Collapse to one line only
  if no contract is lost.
- A one-line summary on a public function, endpoint or API is always fine: callers read it
  without reading the body.

## Quick reference

| Before | After |
|---|---|
| `total += price  # add price to total` | comment deleted |
| `# now batches writes` | deleted |
| `# per the requirements doc, section 3` above a non-obvious limit | `# vendor API rejects batches over 500 rows` |
| `# Per RFC 6749 §4.1.2, state must round-trip unchanged` | unchanged |
| `# same validation as parse_invoice() in billing/io.py` | unchanged |
| `# TODO: memoise` above a function already wrapped in `@cache` | deleted |
| `x = (n & -n)  # lowest set bit` | unchanged; line flagged as refactor candidate |

## Report

After editing, reply with:

1. **Per file:** counts of comments removed / rewritten / kept.
2. **Flags** (out of scope for this skill):
   - refactor candidates
   - TODOs that may hide unfinished work
   - comments kept because you could not verify they are still true
   - narrative worth moving into a doc
3. **Judgement calls:** one line per borderline decision, so the user can override it.

Don't list individual deleted narration comments.
