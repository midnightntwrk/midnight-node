#node #security
# Honor rotated cNIGHT observation configuration in the sliding-window cache

Invalidate the cNIGHT observation sliding-window cache when runtime governance changes the mapping validator address, auth token asset name, or cNIGHT asset identifier. Refreshes now use the current runtime configuration and discard in-flight results if the configuration changes before they commit.

PR: TBD
