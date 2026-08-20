#!/bin/bash

# Example usage script for Bun-based block scanner

echo "Bun-based Block Scanner - Example Usage"
echo "======================================"

# Check if Bun is installed
if ! command -v bun &> /dev/null; then
    echo "Error: Bun is not installed. Please install Bun first:"
    echo "curl -fsSL https://bun.sh/install | bash"
    exit 1
fi

echo "✓ Bun is installed"

# Install dependencies
echo "Installing dependencies..."
bun install

# Example 1: Run with default environment (undeployed)
echo ""
echo "Example 1: Running with default environment (undeployed)"
echo "TARGET_ENV=undeployed bun run src/scanner.ts"

# Example 2: Run with devnet environment
echo ""
echo "Example 2: Running with devnet environment"
echo "TARGET_ENV=devnet bun run src/scanner.ts"

# Example 3: Run with test data folder
echo ""
echo "Example 3: Running with test data folder"
echo "TARGET_ENV=devnet bun run src/scanner.ts /path/to/test/data"

# Example 4: Build and run
echo ""
echo "Example 4: Build and run"
echo "bun run build:scanner"
echo "bun run run:scanner"

echo ""
echo "Available environments:"
echo "- undeployed (default)"
echo "- devnet"
echo "- qanet"

echo ""
echo "For more information, see README.md"