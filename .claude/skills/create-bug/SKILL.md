---
name: create-bug
description: "File a bug issue in midnight-node with GitHub's `type: Bug` field (not the `bug` label). Use when the user asks to create/file/report a bug or open a bug issue, or wants a bug write-up from a failure they just hit. Security vulnerabilities are routed to private reporting, never a public issue."
---

# Create a bug issue

## STOP — is it a security vulnerability?

Answer this before running anything. If the bug lets someone steal or mint funds, forge or censor
transactions, break a privacy guarantee (deanonymize a user, leak witness data), halt or fork the
chain, escalate privileges, or expose keys and secrets — **it does not go in a public issue.**

`SECURITY.md` requires GitHub private vulnerability reporting. Filing publicly *is* the disclosure.

Do **not** run `gh issue create`. Instead:

1. Tell the user plainly that this looks like a vulnerability and the public tracker is the wrong
   channel.
2. Draft the write-up to a local file — it maps onto the advisory form. Note that a private report
   *wants* the source pointers, root cause, PoC and impact analysis that a public bug report omits;
   `SECURITY.md` lists the fields it asks for.
3. Point the user at
   <https://github.com/midnightntwrk/midnight-node/security/advisories/new>
   (fallback: <security@midnight.foundation>).

**The user files it, never you.** If it is unclear whether the bug is exploitable, treat it as a
vulnerability and use the private channel.

## Draft, or file?

This skill is also triggered by "write up that failure as a bug" — which asks for **text**, not a
GitHub issue. Filing is public and notifies watchers; it is not yours to decide.

- **Explicit filing verb** ("file", "create", "open", "raise an issue") → do the whole flow.
- **Anything else** — "write up", "draft", "what would the issue look like" → produce the body
  file, show it, **stop**. Do not run `gh issue create`.
- **Unclear** → ask before filing. Drafting first costs nothing; an unwanted issue has to be
  closed and explained.

## Filing a public bug

Issue **types** are enabled on `midnightntwrk/midnight-node` (`Task`, `Bug`, `Feature`).
Set the type — do **not** add the legacy `bug` label.

```bash
gh issue create --type Bug --title "…" -F body.md \
  -l component:<area> -l origin:<who-found-it> -l bot:ai-assisted
```

`--type` needs gh ≥ 2.68 (`gh --version`). Verify available types with:
`gh api graphql -f query='{repository(owner:"midnightntwrk",name:"midnight-node"){issueTypes(first:20){nodes{name isEnabled}}}}'`

## Report observations, not diagnosis

A bug report describes **what was observed** plus **how the reporter got around it**. Leave out:

- root-cause analysis or theories about why it happens
- suggested fixes ("the fix is to…", "this should instead…")
- pointers into suspected source (`path/file.rs:123`) — a reporter's guess at where the problem
  lives biases whoever picks it up

Stack traces, panic locations, and log lines the system itself emitted are evidence, not
analysis — include them verbatim. Diagnosis and fix design belong in triage.

**Workarounds are in scope** and always worth stating — they are the difference between an
annoyance and a blocker. Always include the section, even to say `None known`.

## 1. Collect evidence before writing

Never invent reproduction steps, versions, or output. Pull them from the session (commands run,
logs, test output) or ask. Minimum bar:

- **Versions**: node/runtime `specVersion`, ledger version, commit SHA, image tags, client lib versions.
- **Environment**: local-env / qanet / preview / preprod / mainnet; validator count; config preset.
- **Exact commands** that reproduce, copy-pasteable.
- **Verbatim** error text or log excerpt (trimmed, not paraphrased).
- **Frequency**: deterministic vs intermittent (say "N of M runs").
- **Workaround**: what unblocks the user, or that nothing does.

If something is unknown, write "unknown" in the issue rather than guessing.

**Redact before you draft, not before you file.** "Verbatim" applies to the error, not to whatever
else shared the line. The STOP gate catches bugs *about* secrets; it does not catch a credential
that happened to be in an otherwise ordinary log. Strip from both commands and output:

- seed phrases, mnemonics, `--seed` values, keystore contents, session and signing keys
- AWS keys and session tokens, `.envrc.local` values, any environment dump
- GitHub tokens (`ghp_…`, `github_pat_…`)
- non-public RPC/node URLs, internal hostnames and IPs

Replace with shaped placeholders — `<redacted:aws-key>`, not deletion — so the repro still reads.
Well-known dev material (`//Alice`, the `dev` preset, `localhost:9944`) is not a secret; leaving it
intact is what makes the steps runnable.

Check for duplicates first. Search **plain words only** — 3–6 distinctive terms, no punctuation:

```bash
gh issue list --search "no rust panics register dust" --state all --limit 20
```

Never paste a raw log line in. GitHub tokenizes the query, so `$`, backticks and quotes add nothing
to recall — while a phrase lifted verbatim from a panic can carry `$(…)`, `` ` `` or `$VAR` that
the shell expands before `gh` ever sees it.

## 2. Title

`<area>: <what fails, in terms of the user action> — <distinguishing detail>`
Area is `node`, `toolkit`, `ledger`, `ci`, etc. Name the **action**, not the code location —
file names and line numbers are log detail and belong in the body.

Good: `toolkit: panic when registering a DUST address — fails 'No Rust Panics …'`
Good: `[HF v8→v9] Upgrade-block events cannot be decoded with post-upgrade metadata`
Bad: `toolkit: panic at register_dust_address.rs:176:40` (code location, not an action)
Bad: `Bug in toolkit` (no symptom)

`--title` is a shell argument too: keep `$`, backticks and double quotes out of it. Single quotes
around a quoted log fragment are fine — see the first Good example. The body is exempt because it
goes through `-F <file>` (§3).

## 3. Body

Follow `.github/ISSUE_TEMPLATE/bug-report.md` — its four sections, plus a workaround section:

````markdown
### Context & versions

- **Node:** `main` @ `<sha>` — `<version>`, `specVersion <n>`, ledger `<v>`
- **Network:** <local-env / qanet / …>, <n> validators
- **Frequency:** deterministic | intermittent — observed in N of M runs

### Steps to reproduce

1. …
2. ```
   <exact command>
   ```

### Expected behavior

<what should have happened>

### Actual behavior

<what happened, with the verbatim log/error in a fenced block — redacted per step 1>

### Workaround

<what unblocks the user, with exact commands/settings — or `None known.`>
````

Write the body to a temporary file and pass `-F <file>` rather than `--body`. Bug bodies contain
backticks, quotes and `$` from log output; quoting rules for those differ between shells, and a
file sidesteps all of it.

## 4. Labels

The type carries "this is a bug". Add only what you actually know:

| Label | When |
|---|---|
| `component:node`, `component:ledger`, `component:midnight-toolkit`, `component:contracts` | the affected component |
| `origin:shielded`, `origin:mnf`, `origin:public` | who found it |
| `bot:ai-assisted` | always, when you drafted the issue |
| `ci`, `test`, `breaking-change` | if applicable |
| `security` | hardening or dependency work that is **already public** — never for an unreported vulnerability (see the STOP gate) |

Do **not** add:

- `bug` — replaced by the type
- `priority:*` — priority is a **field**, not a label (see below)
- `triaged` / `untriaged` / status labels (`TO DO`, `IN PROGRESS`, …) — triage owns those

`gh label list --limit 200` for the current set.

## 5. Priority

Priority is a field on the issue, not a label. `gh issue create` cannot set it, and reading or
writing it needs a token with project permissions — so **leave it unset** and let triage assign it.

What the reporter owes triage is the input for that decision, and it belongs in the body:
impact and whether a workaround exists. No workaround plus severe impact (chain stalls, data
loss, release blocked) is what makes a bug a blocker — state the facts, don't assign the rating.

## 6. Verify

```bash
gh issue view <n> --json number,title,issueType,labels
```

Confirm `issueType.name == "Bug"`. If it came back `null`, the `--type` flag was dropped — set
it after the fact:

```bash
gh issue edit <n> --type Bug
```

Report the issue URL back to the user.
