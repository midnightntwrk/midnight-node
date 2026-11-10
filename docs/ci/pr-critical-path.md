# Measuring the PR critical path

Measure the time from a PR workflow starting until its last required check
finishes. Job runtime alone does not measure the delay before the PR can merge.
Record dependency completion, runner start, test execution and check completion
separately. Advisory checks and metadata-triggered reruns are separate from the
required compute path.

## Comparison procedure

1. Capture a baseline with the existing workflows and unchanged product code.
2. Run the candidate workflows against the same product code. Keep test coverage,
   test inputs, image selection and required check names intact.
3. Confirm that test steps actually ran; tree-cache hits are not performance
   evidence. Record image/build cache hits separately from test-result cache hits.
4. Compare the complete dependency path, including preparation, runner waits and
   test startup. Identify concurrent runner load and cache warmth as confounders.
5. Keep the change draft if the candidate fails, skips required work or does not
   demonstrate an improvement before merge.

Local Rust integration-test binaries are prepared alongside node-image compilation.
The full local suite runs concurrently on one chain. Bridge tests match their own
Cardano transactions, and root calls coordinate signer nonces within the process
so governance approvals can wait for finality concurrently.
Runner-capacity changes require separate infrastructure configuration; a node
workflow change cannot reserve runner slots across other repositories.

## Implementation and runner scope

Existing jobs keep their runner selectors. The preparation job uses the existing
self-hosted pool for trusted PRs and runs alongside node-image compilation. It
warms the exact Earthly target later used by Local Environment Tests. This
avoids publishing build images or transferring large artifacts.
Toolkit compilation selects `--package midnight-node-toolkit` as well as the
existing Nextest execution filter, so unrelated workspace test binaries are not
compiled before the runtime test filter is applied. Toolkit compilation remains
inside its test container; precompiled intermediates are not transported in the
image. Validate both the selected test count
and test outcomes when changing Cargo package selection.

Preparation is skipped for external fork PRs; those retain inline compilation
on GitHub-hosted runners. Missing or evicted cache entries rebuild normally.
A shared cache is required for this particular preparation-job optimization;
isolated GitHub-hosted jobs would need explicit artifact transfer. No hosted
runner saving or new ARC migration is claimed.

Local Environment Tests runs the complete suite in one isolated nested Docker
daemon, with 16 libtest threads and 16 faucet workers. The same execution settings
apply to fork PRs and the local Earthly targets. The local node allows 1,000 RPC
connections to accommodate concurrent test clients and their fetch workers.
Startup, finality, faucet funding and post-suite liveness checks remain required.
Bridge tests filter observations by their own transaction hashes; treasury unlocks
use distinct amounts because their events do not include a transaction hash.
cNIGHT observation waits require the processing cursor to pass the target Cardano
block, since a cost-bounded batch may stop partway through that block.

The GitHub run ID and attempt invalidate Earthly's test execution layer without
invalidating the preparation layers. The existing workflow tree-cache guards
still apply; a guarded skip cannot be counted as evidence of faster execution.

## Validation and rollback

CI must execute the complete suite with no unexpected failures or ignored tests,
then pass the post-suite liveness check. Compare complete required-check latency
against the baseline, accounting for changes to product code and test inventory.
Record preparation time, queueing, node-build slowdown from concurrent compilation,
and stack startup separately. More parallel work can increase shared-host contention.
The earlier two-chain benchmark does not validate this single-chain implementation.

Revert this PR to restore the original workflow and test implementation. No runner
infrastructure, branch protection or separate deployment change is needed.
