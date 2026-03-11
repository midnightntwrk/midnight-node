#node
# Fix cNIGHT observation failing on Byron and enterprise Cardano addresses

The cNIGHT observation data source now handles Byron (base58) and enterprise
(no delegation part) Cardano addresses when scanning for NIGHT token UTXOs.
Previously these address types were silently skipped, causing token movements
to/from such addresses to be missed.

PR:
JIRA:
