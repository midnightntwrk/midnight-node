#node
# Fix cNIGHT observation failing on Byron and enterprise Cardano addresses

The cNIGHT observation data source now handles Byron (base58) and enterprise
(no delegation part) Cardano addresses when scanning for NIGHT token UTXOs.
Previously these address types were silently skipped, causing token movements
to/from such addresses to be missed.

PR: https://github.com/midnightntwrk/midnight-node/pull/901
JIRA: https://shielded.atlassian.net/browse/PM-22277
