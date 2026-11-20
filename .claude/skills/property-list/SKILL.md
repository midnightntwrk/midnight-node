---
name: property-list
description: "Draft a property list for property-based testing of a component, after Hughes' How to Specify It!: agree a validity invariant with the user, then propose kind-labelled properties in pseudo-code. Use when the user asks what to property-test, for a property list or property-testing plan, or to open an issue or post a comment about property testing a component."
---

# Property list

A **property list** is the spec a property-based test suite is written from: definitions, then
numbered properties in pseudo-code, each labelled with its kind. The reference output is
<https://github.com/midnightntwrk/midnight-node/issues/2270#issuecomment-7042318271>; read it
(`gh api repos/midnightntwrk/midnight-node/issues/comments/7042318271 --jq .body`) and match its
register. Its layout predates the format in step 4; follow step 4.

The method is Hughes, *How to Specify It!: A Guide to Writing Properties of Pure Functions*, LNCS
12053 (2020), <https://doi.org/10.1007/978-3-030-47147-7_4>.
Hughes writes for pure functions and generalises the method the same way this skill does (§7): every
operation takes an implicit state argument and returns an invisible state result, so model each
call as a pure function `(state, input) -> (result, state')`. State includes caches, storage,
statics, config and flags. A test case is a **history**, a sequence of calls; a state is
**reachable** when some history from the initial state produces it.

## 1. Model the component

Read the component's code end to end: each public entry point, every input it reads (explicit
arguments and implicit ones: config, flags, process-wide statics, time, which storage backend),
what it returns, and what it writes. Draft the **Definitions** block from this.

Done when every input and every observable output of every entry point is a named term in
Definitions, and every term states which equality properties compare it with: structural, or an
equivalence (e.g. "same error variant", "same contents, any order"). A property can only vary what
Definitions names.

## 2. Agree the validity property

The **validity property** is Hughes' first property: a predicate `valid(x)` that holds for every
value the system can produce. For a stateful component it is an invariant over every reachable
state. All other properties rest on it: they assume generated inputs are valid, and when they are
not, unrelated properties fail and point at the wrong code.

Propose one to three candidates. For each give:

- the predicate in pseudo-code over Definitions terms, written the obvious way, not the efficient
  way (a clever `valid` needs its own proof)
- one concrete invalid state it rejects
- where the code relies on it (`file:line`)

Then discuss it with the user. They know invariants the code never states, and the agreed
property is usually a sharpening of a candidate, not one of them unchanged. Revise and ask again
until the user accepts one exact wording.

**Gate:** step 3 starts when the user has confirmed the exact text of the validity property in
their own words. If they confirmed an earlier draft, or the wording changed after they confirmed
it, ask again.

The confirmed predicate expands into Hughes' full validity set:

```
every operation preserves validity       invariant (validity)
every generated value is valid           test-code check: the test-case generator is right
every shrink of a valid value is valid   test-code check: shrinking is right
```

## 3. Propose the properties

Go through the kinds in order. Each kind is a question to ask of every entry point:

| Kind | Shape | Ask |
|---|---|---|
| **postcondition** | one call, result checked against its inputs | What must be true right after this call? Pick inputs whose outcome is easy to predict, then add a property showing those inputs cover every case. |
| **metamorphic** | two related calls, related results | If I change the input, the history or the state this way, how must the result change? Each operation can act as the modifier for every other one. |
| **inductive** | a call related to calls on smaller inputs | Do a base case and a step pin the behaviour down completely? This only holds if every value can be built by the step, so add a property that checks this with structural equality. |
| **model-based** | implementation vs a simpler reference through an abstraction function | Is there a simpler version (a map, a table, the uncached path, a full re-check) this must agree with? |
| **test-case generator requirement** | not a property: a constraint on the test code that widens another property | Which states does a naive test-case generator never reach (forks, reorgs, collisions, boundaries)? |

Hughes' bug-hunting results in the paper tell you where to spend effort. Model-based properties
found every bug in the operation they covered, and found it in the fewest tests. Postconditions and
metamorphic properties each caught every bug in combination. Validity alone missed five of eight
bugs. If a model is cheap and does not just repeat the implementation, lead with model-based
properties. Otherwise use metamorphic ones.

Then test the set as a whole. Fix the set whenever a check fails:

- **Vacuity**: name the do-nothing implementation (a cache that never hits, an operation that
  always errors, a store that drops writes). At least one property must fail on it.
- **Distinctness**: for each property, name a bug that it catches and no other property in the set
  catches. If you cannot name one, merge the property into the one that subsumes it.
- **Coverage**: every entry point appears in some property, and every term a property uses is in
  Definitions.
- **Preconditions**: each `implies` says how the test-case generator satisfies the precondition. Otherwise
  most generated cases are discarded, and a weak precondition lets bugs through.
- **Collisions**: where two generated values must coincide to reach a case (same key, same state,
  same tx), the property says how the test-case generator makes that happen often enough.

## 4. Write it up

Sections, in order:

1. **What changed** (revisions only, see below)
2. **Definitions**: one fenced block; terms aligned, with sub-fields indented
3. **Kinds**: one bullet per kind the list uses, with a one-line meaning; cite the paper by its DOI
   link
4. **Properties**: one H4 section per property, format below
5. **Notes on what the properties can and cannot see**: blind spots inside the set, generator
   preconditions, assumptions the set relies on
6. **Not covered by these properties**: what the set cannot see at all, and what covers it instead

Each property is an H4 section:

````markdown
#### P<n>: <Short name> [(<other component>) when it lives outside this one]

Kind: **<kind>**[ (<detail>)]

<One sentence: what the property says, in plain domain words.>

```
for all <terms>[, where <constraint>]:
  <statement using ==, implies, let, and Definitions terms only>

note: <optional caveat>
```
````

Write for a reader who knows the component but not property testing:

- **Say which code a property checks.** A property about the test code rather than the component
  has a kind that starts `test-code check` or `test-case generator requirement`, and a description
  that opens "This checks the test code, not <the component>:".
- **Explain property-testing terms where they first appear, or leave them out.** Keep a term only
  when the list needs it. Some terms are clear with a qualifier, such as "test-case generator"
  rather than "generator". Others need a one-line explanation in Definitions: shrinking always
  does.

Number properties `P1`, `P2`, ..., keeping related properties next to each other. Properties that split one idea
share a number with letter suffixes (`P8a`, `P8b`). The confirmed validity property's kind is
`invariant (validity)`.

**Revising an existing list:** keep the numbers stable. A retired number stays unused. New
properties get new numbers or suffixes and are marked `(new)`. Open with a **What changed** list,
and add a short section explaining each new property.

## 5. Deliver

Write the draft to a file in the scratchpad and show it to the user. Where it goes depends on the
request:

| Request | Command |
|---|---|
| comment on a PR | `gh pr comment <n> -F <file>` |
| comment on an issue | `gh issue comment <n> -F <file>` |
| new issue ("open an issue for property testing X") | `gh issue create --type Task --title "<component>: property tests" -l test -l bot:ai-assisted -F <file>` |
| none named | leave it in chat |

Pass the body with `-F`, because the pseudo-code contains backticks and `$`. For a target in
another repo (`-R <owner/repo>`), write references as `midnightntwrk/midnight-node#<n>`, because a
bare `#<n>` links to that repo's own issue. Posting is public:
post only after the user has approved this draft and named the target. Report the URL back.
