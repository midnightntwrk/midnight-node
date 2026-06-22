#toolkit
# Bound intent file reads to 64 MB maximum size

Add a file size check before reading intent files in IntentCustom::new_from_file,
rejecting files that exceed 64 MB. Prevents unbounded memory allocation from
oversized intent files.

PR: https://github.com/midnightntwrk/midnight-node/pull/874
