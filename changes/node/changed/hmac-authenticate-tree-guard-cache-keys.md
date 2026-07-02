#ci #security
# HMAC-authenticate tree-cache-guard cache keys

The tree-cache-guard skip check used `gh cache list`, which sees cache
entries from every branch scope — so a fork PR (which can write caches
scoped to its own ref, but never sees repo secrets) could mint a
`passed-...` key and make trusted runs skip their checks. Cache keys now
carry an HMAC-SHA256 signature over the check name and tree hash, keyed
by the `TREE_GUARD_HMAC` repo secret. Runs without secrets access always
miss and never save; `no-cache-*` sentinel keys are no longer saved.

PR:
