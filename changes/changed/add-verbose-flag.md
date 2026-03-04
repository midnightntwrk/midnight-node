#toolkit
# Add --verbose flag to toolkit CLI

Added a `--verbose` / `-v` global flag to the toolkit CLI that sets the log level
to debug. Default log level is now info. Per-batch fetch log messages have been
demoted from info to debug level, reducing noise while keeping high-level progress
visible.

PR: TBD
