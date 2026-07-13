---
name: format-release-notes
description: Format auto-generated release notes into structured developer-facing release notes
---

Format release notes for a midnight-node release. The user provides $ARGUMENTS — a release tag (e.g. `node-0.21.0`), bare version (`0.21.0`), or GitHub release URL.

Upstream template (**v5**): <https://github.com/midnightntwrk/midnight-network-ops/blob/main/releases/templates/release-note-component-template.md>. A component release note lives in the component's own repo (GitHub Releases), is mirrored into network-ops `releases/components/`, and is composed into a **bundle** (`releases/bundles/<line>/`) whose dependency matrix is the canonical source of truth for cross-component interop. The structure below mirrors the v5 component template, with node-specific additions (an `Artifacts` section holding Docker images + git tree hash, an `Upgrade Type` column on `What Changed`, and an `Other Changes` appendix). If the upstream template changes, re-check this skill for drift. Fetch the template with `gh api "repos/midnightntwrk/midnight-network-ops/contents/releases/templates/release-note-component-template.md" --jq .content | base64 -d` (the repo is private — plain `curl` 404s).

## 1. Normalize Input

- If `$ARGUMENTS` is a URL, extract the tag from it (last path segment).
- If the tag does not start with `node-`, prepend `node-`.
- Store as `TAG`. Extract the bare version (without `node-` prefix) as `VERSION`.
- Determine if this is a **pre-release** (tag contains a pre-release suffix: `-rc.`, `-alpha.`, or `-beta.`). All pre-release kinds are handled the same way for dedup (step 3), known issues (step 6), and testing evidence (step 8).

## 2. Fetch This Release

```bash
gh release view $TAG --repo midnightntwrk/midnight-node --json body,publishedAt,tagName
```

If that fails, try without `--repo` (assumes current repo). Store the body, date, and tag.

Also fetch the git tree hash for the tag:

```bash
git rev-parse "$TAG^{tree}"
```

If the tag is not available locally, fetch it first with `git fetch origin tag $TAG --no-tags`. Store the tree hash.

## 3. Fetch Prior Releases for Deduplication

**Performance:** Fetch the release list (to find prior releases) in parallel with the step 2 fetch, since they are independent.

List releases with `gh release list --repo midnightntwrk/midnight-node --limit 50 --json tagName` and identify prior releases to deduplicate against:

- **If this is a pre-release** (e.g. `node-0.22.0-rc.6`, `node-2.0.0-alpha.1`): collect **all** earlier pre-releases for the same version (any `-alpha.`/`-beta.`/`-rc.` of that version that precede this one — e.g. `node-0.22.0-rc.1` through `-rc.5`). Each pre-release's formatted notes only contain its own delta, so checking only the immediately prior one misses entries announced earlier. For the **first** pre-release of a version (e.g. `node-2.0.0-alpha.1`), there are no earlier pre-releases, so the prior release is the previous **final** release (e.g. `node-1.0.0`).
- **If this is a final release** (e.g. `node-0.22.0`): the prior release is the previous final release (`node-0.21.0`). Skip any pre-releases.

Fetch **all** prior release bodies in parallel:

```bash
gh release view $PRIOR_TAG --repo midnightntwrk/midnight-node --json body
```

Extract all PR numbers (pattern: `#\d+` or `/pull/\d+`) from **every** prior release body and combine them into a single set. These PRs will be **excluded** from the current notes — they were already announced in a previous pre-release or release.

## 4. Parse Auto-Generated Body

Extract entries from `## Added` and `## Changed` sections of the current release body. **Stop parsing at `# Tagged Changes`** — that section duplicates entries by tag and should be ignored.

Each entry is typically a `<details>` block with a summary line containing tags and a PR link.

Remove any entries whose PR numbers appeared in the prior release.

## 5. Classify Each Change

Use the tags present in each entry as the primary signal:

| Tags present | Classification |
| ------------ | -------------- |
| `#runtime`, `#cnight-observation` | Runtime upgrade |
| `#ledger` with **new ledger major version** (e.g. ledger 7 to 8) | Runtime upgrade (spec_version gates which version is active) |
| `#ledger` with **patch/minor to same major** (e.g. 8.0.0-rc.3 to rc.4) | Node upgrade (ledger accessed via host calls; new binary = new behaviour) |
| `#node`, `#client`, `#rpc`, `#networking` only | Node upgrade (binary restart) |
| `#toolkit` only | Toolkit (separate binary) |
| Both runtime and node/client/ledger tags | Mixed — note both |
| `#audit` | Runtime upgrade (pallet storage/logic changes from audit findings) |
| `#hardfork` | Runtime upgrade (hard fork related) |
| `#infra`, `#changed` only | Infrastructure/build |
| No tags or ambiguous | Inspect PR files via `gh pr view <number> --json files` and classify by path |

**Performance:** Run all `gh pr view --json files` lookups for untagged PRs in parallel with each other and with the known-issues query from step 6.

Path-based classification fallback:

- `pallets/`, `runtime/` files = Runtime upgrade
- `node/` files = Node upgrade
- `util/toolkit/` files = Toolkit
- `ledger/` files = check if major version bump (runtime) or patch (node)
- CI/build files only = Infrastructure

## 6. Present Classification and Known Issues for Review

**Pre-releases (`-rc.`/`-alpha.`/`-beta.`):** Skip the known-issues *query* — do not assess known issues at pre-release stage. The `## Known issues` heading still stays in the output with `None` under it (anchor stability per the v5 empty-section policy), unless the user supplies a specific known issue to record. Only present the classification summary table for confirmation.

**Final releases:** Run the known-issues query **in parallel** with the file-based classification lookups (step 5). Issues are tracked in GitHub (not JIRA). Prefer the local git-bug store for speed (per the user's CLAUDE.md, `~/git/org1/direct/midnightntwrk/midnight-node` has a synced git-bug store):

```bash
# Sync first if stale, then query locally:
git-bug -C ~/git/org1/direct/midnightntwrk/midnight-node bridge pull github 2>&1 | tail -1
git-bug -C ~/git/org1/direct/midnightntwrk/midnight-node bug status:open label:bug
```

Fallback (if git-bug is unavailable or stale):

```bash
gh issue list --repo midnightntwrk/midnight-node --state open --label bug --json number,title,labels --limit 200
```

Filter results to high-criticality issues only — known issues should be reserved for the most critical problems. Label conventions vary; common signals are labels like `priority/critical`, `priority-high`, `P0`, `P1`, `blocker`, or `critical`. If the repo's labels are unclear, present the full open-bug list to the user and let them pick.

Exclude any issues that are already being fixed in this release (i.e. appear in the Fixed defect list table).

Present **both** the classification summary table **and** the filtered known issues list in a single prompt. Ask the user to confirm/correct classifications and indicate which (if any) issues should be included as known issues. This avoids two separate roundtrips.

## 7. Generate Supporting Docs

Some `## Links and references` buckets require accompanying doc files that don't yet exist. Identify what's needed by walking the formatted notes:

| Trigger in release notes | Doc to create | Suggested path |
| ------------------------ | ------------- | -------------- |
| Any entry under `## Breaking changes` | Migration guide for that change | `docs/release-notes/<VERSION>/migration-<slug>.md` |
| Any entry under `## Deprecations` with non-trivial migration steps | Migration guide for the deprecation | `docs/release-notes/<VERSION>/migration-<slug>.md` |
| New or modified developer-facing API in `## New features` (RPC, runtime API, toolkit CLI surface) | SDK / API reference page | `docs/release-notes/<VERSION>/sdk-<slug>.md` |
| `## New features requiring configuration updates` entry | Operator config guide | `docs/release-notes/<VERSION>/config-<slug>.md` |
| Significant architecture / design change worth explaining | Engineering doc | `docs/release-notes/<VERSION>/eng-<slug>.md` |
| Any entry classified as **Runtime upgrade** in step 5 | Runtime metadata diff (subwasm) | `docs/release-notes/<VERSION>/runtime-diff.md` |

If no triggers fire, skip this step entirely.

### 7a. Runtime metadata diff (subwasm)

Only run this when at least one change was classified as a **Runtime upgrade**. Skip for node-only / toolkit-only / infra-only releases.

Tool: [`subwasm`](https://github.com/chevdor/subwasm). Confirm it is installed (`subwasm --version`); if not, ask the user to install it (`cargo install --locked --git https://github.com/chevdor/subwasm`) — do not silently skip.

**Locate the prior runtime WASM** (same `PRIOR_TAG` used in step 3 for final releases; for a pre-release, diff against the previous final release, **not** the previous pre-release, so the cumulative runtime delta is visible):

1. Try GitHub release assets first:

   ```bash
   gh release download "$PRIOR_TAG" --repo midnightntwrk/midnight-node --pattern '*runtime*.wasm' --dir /tmp/runtime-prev
   gh release download "$TAG"       --repo midnightntwrk/midnight-node --pattern '*runtime*.wasm' --dir /tmp/runtime-curr
   ```

2. If no WASM asset is published, fall back to extracting from the docker image (`docker create` + `docker cp` against the published node image), or — last resort — build via `earthly -P +runtime-wasm` at each tag. Building takes ~20 min per side; ask the user before going down that path.

**Run the diff** and capture both the human-readable and the machine-readable forms:

```bash
subwasm diff /tmp/runtime-prev/*.wasm /tmp/runtime-curr/*.wasm > /tmp/subwasm-diff.txt
subwasm diff /tmp/runtime-prev/*.wasm /tmp/runtime-curr/*.wasm --json > /tmp/subwasm-diff.json
subwasm info /tmp/runtime-curr/*.wasm > /tmp/subwasm-info-curr.txt
subwasm info /tmp/runtime-prev/*.wasm > /tmp/subwasm-info-prev.txt
```

**Write `docs/release-notes/<VERSION>/runtime-diff.md`** with this structure:

1. `# Runtime Diff: <PRIOR_VERSION> → <VERSION>`
2. Metadata header table — both runtimes' `spec_name`, `spec_version`, `impl_version`, `transaction_version`, `authoring_version`, blake2-256 of the WASM, compressed size. Pull from `subwasm info`.
3. `## Summary` — one or two sentences: did `spec_version` bump? `transaction_version`? Any pallets added/removed? (Bumping `transaction_version` means signed extrinsics from the previous runtime won't decode — call this out loudly.)
4. `## Pallet changes` — added / removed / modified pallets, each with their changed extrinsics, storage items, events, errors, and constants. Use the JSON diff to drive this; the text diff is fine to embed verbatim in a fenced block as a fallback.
5. `## Runtime APIs` — added / removed / modified runtime API methods.
6. `## Raw subwasm diff` — fenced block with the full text-mode output, for auditability.

Mention the file in the release notes by adding a row in `## What changed` — `Runtime metadata diff (subwasm)` with type `Runtime upgrade` and the doc path as the link — and include it under `**Engineering docs**:` in `## Links and references`.

If `transaction_version` bumped, also add a warning blockquote at the top of `## Breaking changes` pointing operators at the diff.

### 7b. Other supporting docs

**Before generating:** check whether a doc already exists for the topic (e.g. `docs/`, `midnight-docs` repo, or earlier release-notes folders). If so, reference it instead of regenerating.

**Confirm with the user** before writing — list the proposed files (path + one-line purpose) and ask for approval. The user may want them in `midnight-docs` instead of the current repo, or may want to skip some.

**Research and generate:**

- For **3 or more** docs, dispatch one `general-purpose` subagent per doc, **all in a single message** so they run in parallel. Each subagent's prompt must be self-contained: state the doc's purpose, the source PR(s), the relevant files to read, the audience, and the output path.
- For **1–2** docs, do the research and writing inline — subagent overhead isn't worth it.

Each subagent should:

1. Read the source PR(s) (`gh pr view <num> --json title,body,files`) and the changed files.
2. For migration guides: produce step-by-step before/after code examples, environment/version pinning, rollback notes.
3. For SDK / API pages: signature, parameters, return type, examples, error cases.
4. For config guides: each new flag/env var, default, when to set it, interaction with existing config.
5. For engineering docs: motivation, design alternatives considered, architectural impact, links to relevant code.
6. Cross-link back to the release notes file (`docs/release-notes/release-notes-<VERSION>.md`).
7. Pass `npx markdownlint-cli` cleanly — same disable rules as the release notes file when verbatim content is included.

**After subagents finish:** verify each file actually exists and has substantive content (subagent summaries describe intent, not output). Update the release notes' `## Links and references` buckets to point at the newly created files using relative paths (`docs/release-notes/<VERSION>/...`).

If any doc could not be filled in fully (missing context, unclear API surface), leave a clearly-marked `TODO:` placeholder rather than fabricating content, and flag it in the report to the user.

## 8. Generate Output

Write the formatted release notes to `release-notes-VERSION.md` in the repo root.

### Template

The output must follow this structure (shown as description, not as a nested code block):

Section order, naming, and per-item structure follow the v5 component template (linked at the top of this skill) as closely as possible. **Headings are sentence case** (`## New features`, not `## New Features`). Node-specific extras (Artifacts, the Upgrade Type column, Other Changes, the Issue field on Known issues) are kept in addition to the template structure, not in place of it.

**Empty-section policy (v5):** the canonical v5 sections — `New features requiring configuration updates`, `Improvements`, `Deprecations`, `Breaking changes`, `Known issues`, `Fixed defect list` — must **always be present**. When a section has no content, write `None` under it; **never delete the heading**. Empty-section anchors keep cross-doc deep links stable. Only the node-specific extras (`Artifacts` when <3 artifacts, `Other Changes` when nothing is residual, the conditional Sister-line note) may be omitted entirely.

1. Start with `<!-- markdownlint-disable MD012 MD013 MD014 MD022 MD031 MD032 MD033 MD034 MD060 -->` (MD012/MD022/MD031/MD032 are needed because verbatim upstream content contains multiple blank lines, headings without surrounding blanks, and fenced code blocks/lists without surrounding blanks)
2. `# Midnight Node VERSION` — match the template's `<component> <version>` form (no "Release Notes" suffix)
3. `## Metadata` — bullet list (blank line between heading and list):
   - `**Type of release**: <patch | minor | major>` — derive from the version delta vs the prior final release
   - `**Date**: YYYY-MM-DD`
   - `**Ships in bundle**: <[bundle-id](link to bundle RN in network-ops) | TBD — standalone <type> release (not bundled)>` — the v5 bundle pointer; the bundle's dependency matrix is the canonical interop source, do not duplicate matrix rows here
   - `**Git tag**: [TAG](https://github.com/midnightntwrk/midnight-node/tree/TAG)`
   - `**Environment**: All public networks at time of release. For the full compatibility matrix, see the [release notes overview](https://docs.midnight.network/relnotes/overview).`
   - `**Upgrade scope**: <binary only | binary + runtime>` — headline mirrored from Deployment information below
   - `**Reset required**: <No | Yes>`
   - `**Governance action required**: <No | Yes>`
   - `**Sister-line note**: …` — include **only** when a parallel release line exists (e.g. maintenance vs development); otherwise omit this line
4. `## High-level summary` — 1–3 sentences. State plainly whether this is a runtime upgrade or binary-only.
5. `## Audience` — checklist with a blank line before and after the list. Scope down to the audiences relevant to this release; phrase entries as "Operators who…", "Developers who…", "Administrators who…".
6. `## Dependencies` — pointer-only. List only hard, component-local incompatibilities a reader must see without clicking through (e.g. `requires Cardano node >= X`, `requires ledger >= Y`); write `—` if none. Then a **Downstream impact (cascading effects)** note: how upgrading the node affects components that depend on it; `—` if none. Close with: "For all other interop questions, see the bundle dependency matrix."
7. `## Deployment information` — **mandatory** operational triage. Answer every field; `No` / `None` is valid, blank is not:
   - `**Upgrade scope**: <binary only | binary + runtime>` — new image only, or a coordinated on-chain runtime upgrade?
   - `**Reset required**: <No | Yes — describe>` — state-wipe / re-sync / re-index?
   - `**Governance action required**: <No | Yes — describe>` — on-chain proposal / SPO vote needed to activate, and who triggers it?
   - `**Downtime / coordination**: <None | describe>` — hot-swappable, or needs a maintenance window / cross-operator timing?
8. `## Artifacts` — node-specific section absorbing the old Docker Images + Tree hash. List each tagged artifact, then a `shell` fenced block of `docker pull` commands (no `$` prefix). Include:
   - `**Docker**: midnightntwrk/midnight-node:VERSION` and `…/midnight-node-toolkit:VERSION`
   - `**Git tree hash**: <output of git rev-parse TAG^{tree}>`
   (The node ships ≥3 artifacts, so it always uses this section rather than the single-artifact metadata line.)
9. `## What changed` — first a **bullet list** of new features/improvements/fixes (per v5), then the node value-add **table** with columns: Change, Upgrade Type, PR. Separator row: `| --- | --- | --- |`. (The real published node RN keeps both forms.)
10. `## New features` — for each feature use a `### <feature name>` subheading, a `**Description**:` line covering **what it does**, **why it matters**, **where developers interact with it**, and the **motivation** (append the upgrade-type annotation — *Runtime upgrade* / *Node upgrade* / *Toolkit*), and a `**PR**: [#N](url)` line. Write `None` if empty.
11. `## New features requiring configuration updates` — for each: `### <feature name>`, then `**Required updates**:` (bullets) and `**Impact**:` (one-line). Write `None` if empty.
12. `## Improvements` — for each: `### <name>` (or `**Improvement N**:` when grouping related items), a `**Description**:` of the enhancement / UX lift / perf gain, and a `**PR**:` line. Write `None` if empty.
13. `## Deprecations` — write `None` if empty. For each deprecated item:
    - `**Deprecated item**: <API/function/feature>`
    - `**Starts**: <date/version>`
    - `**Full removal**: <planned removal date/version>`
    - `**Replacement**: <new API/method>`
    - `**Migration steps**: <explicit actions developers must take>`
14. `## Breaking changes` — write `None` if empty. Include the warning blockquote when runtime upgrade changes exist. For each breaking change use a `### <change name>` subheading with:
    - `**What changed**: <clear description>`
    - `**What breaks**: <exact scenarios>`
    - `**Required actions**:` (bullets, including environment/version pinning where relevant)
    - `**Code example**:` (migration snippet) — omit only if genuinely not applicable
15. `## Known issues` — write `None` if empty. **For pre-releases write `None`** (known issues are not assessed at pre-release stage — the heading still stays for anchor stability) unless the user supplies a specific known issue to record. For final releases include only critical/high-priority issues, each as `### <name>` with `**Description**:`, `**Issue**: [#N](url)` (GitHub issue link — node-specific addition), and `**Workaround (if any)**:` fields.
16. `## Links and references` — bucketed list; omit a bucket only when it genuinely has no entries (the section heading itself always stays):
    - `**QA test coverage / test evidence**:` — QA sign-off, test reports, soak-test dashboards (final releases only — for pre-releases, omit the bucket). If QA has not provided evidence yet, leave a `TODO: link QA sign-off` placeholder and flag it to the user so it is chased before publication.
    - `**PRs**:` — markdown links to PRs in this release
    - `**Engineering docs**:` — internal/external engineering documentation
    - `**Migration guides**:` — links for any breaking changes / deprecations above
    - `**SDK docs**:` — when SDK-facing surface area changed
    - `**Known issues board**:` — link to the GitHub issues view (e.g. `https://github.com/midnightntwrk/midnight-node/issues?q=is%3Aissue+is%3Aopen+label%3Abug`)
    - `**Public schema**:` — only when the component publishes one (GraphQL / OpenAPI / ABI); link the versioned schema file
    - `**API documentation**:` — only when the component exposes a public API (e.g. node RPC surface)
    - `**GitHub release**:` — link to the release tag page
    Never use bare URLs.
17. `## Fixed defect list` — table with columns: `Defect number`, `Description`. Separator row: `| --- | --- |`. Write `None` (or an empty-state note) if no defects were fixed.
18. `## Other Changes` — node-specific appendix replacing the old verbatim "Full Change Details" dump. List **only residual** Added/Changed entries **not already mentioned or linked** elsewhere in the notes (dedup against What Changed, New features, Improvements, and Fixed defect list). Render as a plain bullet list — `- <description> ([#N](url))`. **Omit the whole section if nothing is residual.**

After writing, run `npx markdownlint-cli release-notes-VERSION.md` to verify. Fix any violations before presenting to the user.

## 9. Offer to Update GitHub Release

After writing the file, ask the user if they want to update the GitHub release body with the formatted notes:

```bash
gh release edit $TAG --repo midnightntwrk/midnight-node --notes-file release-notes-VERSION.md
```

Do NOT run this without explicit user confirmation.

## Important Notes

- Do NOT include LLM watermarks or co-authored-by lines.
- The key insight: ledger changes are accessed via **host calls**. A new ledger major version is a runtime upgrade (gated by spec_version). A patch to the current ledger version is a node upgrade (binary provides the implementation).
- Check `runtime/src/lib.rs` for the current `spec_version` to understand version gating.
- If unsure about a classification, err on the side of asking the user rather than guessing.
