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

The initial candidates are preparing Rust integration-test binaries alongside
node-image compilation and partitioning slow local bridge tests onto independent
chain state. Keep the existing shared-state locks within each test process.
Runner-capacity changes require separate infrastructure configuration; a node
workflow change cannot reserve runner slots across other repositories.
