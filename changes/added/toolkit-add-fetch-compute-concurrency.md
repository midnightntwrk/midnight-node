#toolkit
# Add a fetch-compute-concurrency option for toolkit

Every time the toolkit.db is fetched, it uses by default 20 workers for fetching and MAX for computing. When the toolkit is used in multi-threaded applications this leads to resource competition and wrong behavior. We propose to limit the number of threads used for computing.

PR: https://github.com/midnightntwrk/midnight-node/pull/675
JIRA: https://shielded.atlassian.net/browse/PM-21786