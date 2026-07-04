#toolkit #fetcher
# Fetcher no longer aborts when the fetch cache is ahead of the node

A shared fetch cache (e.g. Postgres) can be ahead of the finalized
height reported by the node — concurrent fetchers populate it through
the same RPC URL and load-balanced replicas don't agree
block-for-block. The fetcher's `max_height - min_height` job-count
computation underflowed in that case, inflating the job count to
~u64::MAX/100 and aborting the whole process (SIGABRT via
`handle_alloc_error`) when allocating the jobs vector. The height
delta now saturates at zero: an ahead-of-node cache simply means
"nothing new to fetch".

PR: <link>
