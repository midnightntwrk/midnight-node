# Actionlint Guide for Midnight Node

This guide explains how to use actionlint for validating GitHub Actions workflows in the midnight-node repository.

## Table of Contents

- [What is Actionlint?](#what-is-actionlint)
- [Configuration](#configuration)
- [Running Actionlint Locally](#running-actionlint-locally)
- [CI/CD Integration](#cicd-integration)
- [Common Errors and How to Fix Them](#common-errors-and-how-to-fix-them)
- [Adding New Ignore Patterns](#adding-new-ignore-patterns)
- [Troubleshooting](#troubleshooting)

## What is Actionlint?

[Actionlint](https://github.com/rhysd/actionlint) is a static checker for GitHub Actions workflow files. It validates:

- Workflow syntax and structure
- Expression syntax (e.g., `${{ }}`)
- Shell script syntax (via shellcheck)
- Python script syntax (via pyflakes)
- Action inputs and outputs
- Job dependencies
- Reusable workflow inputs

Actionlint helps catch errors before pushing to GitHub, preventing CI failures and saving development time.

## Configuration

The midnight-node repository uses two configuration files for actionlint:

### 1. Workflow File: `.github/workflows/actionlint.yml`

This workflow automatically runs actionlint on every PR that modifies workflow files.

```yaml
name: Validate GitHub Actions Workflows

on:
  pull_request:
    paths:
      - ".github/workflows/*.yml"
      - ".github/workflows/*.yaml"
  push:
    branches: [main]
    paths:
      - ".github/workflows/*.yml"
      - ".github/workflows/*.yaml"

jobs:
  actionlint:
    name: Lint workflows
    runs-on: ubuntu-latest
    steps:
      - name: Checkout repository
        uses: actions/checkout@08c6903cd8c0fde910a37f88322edcfb5dd907a8 # v5.0.0

      - name: Run actionlint
        uses: raven-actions/actionlint@3a24062651993d40fed1019b58ac6fbdfbf276cc # v2
        with:
          matcher: true # Enable GitHub annotations for errors
          fail-on-error: true # Fail CI when errors are found
```

### 2. Configuration File: `.github/actionlint.yaml`

This file defines custom runner labels and ignore patterns for known false positives:

```yaml
self-hosted-runner:
  labels:
    - ubuntu-latest-8-core-x64
    - ubuntu-latest-16-core-x64
    - ubuntu-latest-8-core-arm64

paths:
  .github/workflows/*.{yml,yaml}:
    ignore:
      # Workflow validation ignores
      - 'default value.*but.*also required'
      - 'property.*is not defined in object type'
      - 'property.*is not defined'
      - 'is not defined.*object type'
      - 'receiver of object dereference'
      # Shellcheck ignores
      - 'SC2129'  # Consider using { cmd1; cmd2; } >> file
      - 'SC2155'  # Declare and assign separately
      - 'SC2236'  # Use -n instead of ! -z
      - 'SC2046'  # Quote to prevent word splitting
      - 'SC2086'  # Double quote to prevent globbing
      - 'SC2002'  # Useless cat
      - 'SC2034'  # Variable appears unused
      - 'SC2091'  # Remove surrounding $()
      - 'SC1089'  # Parsing stopped here
```

**Important:** The configuration file approach is used instead of command-line flags because it's more reliable in GitHub Actions (command-line flags can have quote escaping issues).

## Running Actionlint Locally

### Installation

**macOS (Homebrew):**
```bash
brew install actionlint
```

**Linux:**
```bash
# Download the latest release
bash <(curl https://raw.githubusercontent.com/rhysd/actionlint/main/scripts/download-actionlint.bash)
sudo mv ./actionlint /usr/local/bin/
```

**Using Go:**
```bash
go install github.com/rhysd/actionlint/cmd/actionlint@latest
```

### Running Locally

From the repository root:

```bash
# Run with verbose output
actionlint -verbose

# Run on specific workflow files
actionlint .github/workflows/main.yml

# Show only error count
actionlint | wc -l
```

**Expected output when successful:**
```
verbose: Found 0 errors in 18 files
```

The tool will automatically use the `.github/actionlint.yaml` configuration file.

## CI/CD Integration

The actionlint workflow runs automatically on:

1. **Pull Requests** - When any workflow file (`.github/workflows/*.yml` or `.github/workflows/*.yaml`) is modified
2. **Pushes to main** - When workflow files are pushed to the main branch

**Build Status:**
- ✅ **Pass:** All workflows valid, or errors match ignore patterns
- ❌ **Fail:** New actionlint errors detected

If the build fails, check the CI logs to see which errors were found.

## Common Errors and How to Fix Them

### 1. Shellcheck Errors

#### SC2129: Consider using { cmd1; cmd2; } >> file

**Error:**
```
shellcheck reported issue in this script: SC2129:style:7:1:
Consider using { cmd1; cmd2; } >> file instead of individual redirects
```

**Why it's ignored:**
This is a style suggestion, not a bug. The code works correctly either way.

**If you want to fix it:**
```bash
# Before
echo "foo" >> file
echo "bar" >> file

# After
{
  echo "foo"
  echo "bar"
} >> file
```

#### SC2155: Declare and assign separately

**Error:**
```
shellcheck reported issue in this script: SC2155:warning:10:8:
Declare and assign separately to avoid masking return values
```

**Why it's ignored:**
This pattern is common in our workflows and the return value masking is acceptable for these use cases.

**If you want to fix it:**
```bash
# Before
local var=$(command)

# After
local var
var=$(command)
```

#### SC2086: Double quote to prevent globbing and word splitting

**Error:**
```
shellcheck reported issue in this script: SC2086:info:1:54:
Double quote to prevent globbing and word splitting
```

**Why it's ignored:**
Many of our workflow scripts use GitHub Actions variables that don't require quoting in their context.

**If you want to fix it:**
```bash
# Before
echo $GITHUB_ENV

# After
echo "$GITHUB_ENV"
```

### 2. Workflow Validation Errors

#### Input has default value but is also required

**Error:**
```
input "RESET" of workflow_call event has the default value "false",
but it is also required. if an input is marked as required,
its default value will never be used
```

**Why it's ignored:**
This is a design pattern used in some reusable workflows for documentation purposes - the default value serves as documentation even though it won't be used.

**If you want to fix it:**
```yaml
# Before
inputs:
  RESET:
    required: true
    default: "false"

# After - either remove required or remove default
inputs:
  RESET:
    required: false
    default: "false"
```

#### Property is not defined in object type

**Error:**
```
property "suffix" is not defined in object type {branch: string}
```

**Why it's ignored:**
This occurs when accessing workflow inputs that may not be defined in all contexts. The workflows handle this gracefully.

**If you want to fix it:**
Define the property in the workflow inputs, or use conditional expressions to check if it exists.

### 3. Expression Errors

#### Receiver of object dereference

**Error:**
```
receiver of object dereference "test_env" must be type of object but got "string"
```

**Why it's ignored:**
This is a false positive where actionlint misinterprets the type of a variable in certain contexts.

## Adding New Ignore Patterns

If you encounter a new actionlint error that is a false positive or an acceptable pattern for this project:

1. **Identify the error pattern** from the CI logs
2. **Add the pattern** to `.github/actionlint.yaml` under the `ignore` section

**Example - Adding a new shellcheck code:**

```yaml
paths:
  .github/workflows/*.{yml,yaml}:
    ignore:
      # ... existing patterns ...
      - 'SC2116'  # Useless echo
```

**Example - Adding a new expression pattern:**

```yaml
paths:
  .github/workflows/*.{yml,yaml}:
    ignore:
      # ... existing patterns ...
      - 'type of.*is not assignable'
```

**Important Notes:**
- Patterns use RE2 regex syntax (not PCRE or JavaScript regex)
- Patterns are case-sensitive
- Use `.*` for wildcard matching
- Test locally with `actionlint -verbose` to confirm the pattern works

### Regex Pattern Tips

Actionlint uses RE2 regex syntax. Common patterns:

- `.` - Match any single character
- `.*` - Match zero or more of any character
- `[abc]` - Match any character in the set
- `[^abc]` - Match any character NOT in the set
- `\d` - Match any digit
- `\s` - Match any whitespace
- `^` - Start of line
- `$` - End of line

**Example patterns:**
```yaml
- 'property.*is not defined'  # Matches "property X is not defined"
- 'SC\d+'                     # Matches any shellcheck code like SC2086
- '^Error:.*workflow'         # Matches lines starting with "Error:" containing "workflow"
```

## Troubleshooting

### Issue: Actionlint passes locally but fails in CI

**Possible causes:**
1. Different actionlint versions
2. Configuration file not committed
3. Workflow file has platform-specific issues

**Solution:**
- Check actionlint version: `actionlint --version`
- Ensure `.github/actionlint.yaml` is committed
- Review CI logs for specific errors

### Issue: Too many false positives

**Solution:**
Add appropriate ignore patterns to `.github/actionlint.yaml`. Our project uses ignore patterns for:
- Shellcheck style suggestions (SC2129, SC2155, etc.)
- Workflow design patterns (required inputs with defaults)
- Context-specific expression type mismatches

### Issue: New workflow file fails actionlint

**Steps to debug:**
1. Run `actionlint .github/workflows/your-file.yml -verbose`
2. Check if the error is legitimate or a false positive
3. If legitimate, fix the workflow
4. If false positive, add ignore pattern to `.github/actionlint.yaml`

### Issue: Shellcheck not installed

**Error:**
```
Rule "shellcheck" was disabled: exec: "shellcheck": executable file not found in $PATH
```

**Solution:**
Install shellcheck:
```bash
# macOS
brew install shellcheck

# Linux (Debian/Ubuntu)
apt-get install shellcheck

# Linux (Fedora)
dnf install shellcheck
```

Note: In CI, shellcheck is automatically installed by the actionlint GitHub Action.

## Best Practices

1. **Run actionlint locally** before pushing workflow changes
2. **Review CI errors carefully** - some may indicate real issues
3. **Don't ignore real errors** - only add ignore patterns for false positives or intentional design patterns
4. **Document why** you're adding an ignore pattern (use comments in the config file)
5. **Keep ignore patterns specific** - use precise regex patterns to avoid masking real errors
6. **Test workflow changes** in a PR before merging

## Resources

- [Actionlint Official Documentation](https://github.com/rhysd/actionlint)
- [Shellcheck Wiki](https://www.shellcheck.net/wiki/)
- [GitHub Actions Documentation](https://docs.github.com/en/actions)
- [RE2 Regex Syntax](https://github.com/google/re2/wiki/Syntax)

## Support

If you encounter actionlint issues:

1. Check this guide first
2. Review existing ignore patterns in `.github/actionlint.yaml`
3. Test locally with `actionlint -verbose`
4. If stuck, consult with the team or create an issue in the midnight-node repository
