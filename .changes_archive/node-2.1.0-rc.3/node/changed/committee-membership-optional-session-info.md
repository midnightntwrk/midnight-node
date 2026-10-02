#node
# Skip committee-membership logs when SessionInfoApi is absent

The committee-membership watcher called `SessionInfoApi::current_session_index`
on every imported block. Runtimes from before that API was added do not
export it, so each such block logged an error and the watcher never
reported membership.

The watcher now checks `has_api` first. Blocks whose runtime does not
export `SessionInfoApi` are skipped, with a single info line the first
time, until a runtime that includes the API is enacted.

PR: https://github.com/midnightntwrk/midnight-node/pull/2204
Issue: https://github.com/midnightntwrk/midnight-node/issues/2203
