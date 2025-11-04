#!/bin/bash

# Default base path from Docker ENV
DEFAULT_BASE_PATH="$BASE_PATH"

# Parse arguments to find --base-path
PARSED_BASE_PATH=""
prev_arg=""
for arg in "$@"; do
    if [[ "$arg" == --base-path=* ]]; then
        # Extract value after = sign
        PARSED_BASE_PATH="${arg#*=}"
    elif [[ "$prev_arg" == "--base-path" ]]; then
        # Handle --base-path <value> format
        PARSED_BASE_PATH="$arg"
    fi
    prev_arg="$arg"
done

# Use default if not specified
if [ -z "$PARSED_BASE_PATH" ]; then
    FINAL_BASE_PATH="$DEFAULT_BASE_PATH"
else
    FINAL_BASE_PATH="$PARSED_BASE_PATH"
fi

# Create directories and set permissions if they don't exist
if [ ! -d "$FINAL_BASE_PATH" ]; then
    mkdir -p "$FINAL_BASE_PATH"
fi

# Now run as appuser
exec /midnight-node -- "$@"
