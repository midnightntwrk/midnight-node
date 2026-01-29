#toolkit
# Add ledger parameters config to genesis generation

Adds `--ledger-parameters-config` option to `generate-genesis` command, allowing custom ledger parameters to be set at genesis instead of using the default `INITIAL_PARAMETERS`. Config files with default values are available in `res/<network>/ledger-parameters-config.json`.

PR: https://github.com/midnightntwrk/midnight-node/pull/542
JIRA: https://shielded.atlassian.net/browse/PM-20852
