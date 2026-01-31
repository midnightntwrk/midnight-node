#local-environment
# Fix contract-compiler docker volume path to be project-local

Change the contract-compiler service volume mount default from an external relative
path to a project-local path, improving portability across developer environments.

PR: https://github.com/midnightntwrk/midnight-node/pull/497
JIRA: https://shielded.atlassian.net/browse/PM-21404
