# Actionlint Guide for Midnight Node

## What is Actionlint?

[Actionlint](https://github.com/rhysd/actionlint) is a static checker for GitHub Actions workflow files that validates syntax, expressions, shell scripts (via shellcheck), action inputs/outputs, and job dependencies. It runs automatically on PRs that modify workflow files.

## Configuration

### Workflow File
`.github/workflows/actionlint.yml` - Runs on every PR and push to main that modifies workflow files

### Config File
`.github/actionlint.yaml` - Contains custom runner labels and ignore patterns:

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

**Why these patterns?**
- **Workflow patterns**: False positives for workflow_call inputs and secret references that actionlint cannot verify
- **Shellcheck patterns**: Style suggestions or false positives in GitHub Actions context where many variables don't require quoting

**Note:** These errors are filtered (125 total across workflows) because they're either intentional design patterns or shellcheck being overly strict for GitHub Actions syntax.

## Running Actionlint Locally

### Install
```bash
# macOS
brew install actionlint

# Linux
bash <(curl https://raw.githubusercontent.com/rhysd/actionlint/main/scripts/download-actionlint.bash)
sudo mv ./actionlint /usr/local/bin/

# Using Go
go install github.com/rhysd/actionlint/cmd/actionlint@latest
```

### Run
```bash
actionlint -verbose                        # Check all workflows with verbose output
actionlint .github/workflows/main.yml      # Check specific file
```

**Expected output when successful:**
```
verbose: Found 0 errors in 18 files
```

## Formatting Workflow Files with Prettier

**IMPORTANT**: All workflow files must be formatted with prettier before committing.

**Pre-Commit Steps:**
1. Edit workflow file
2. `npx prettier --write '.github/workflows/*.{yml,yaml}'`
3. `actionlint -verbose`
4. Commit

**Note:** Workflow files are NOT excluded from prettier (no `.prettierignore`).

## Common Issues

**SC2129**: Style suggestion for grouped redirects - ignored as individual redirects are more readable

**SC2155**: Declare and assign separately - ignored as the pattern is acceptable for workflow context

**SC2086**: Double quote to prevent globbing - ignored for GitHub Actions variables that work correctly unquoted (e.g., `$GITHUB_OUTPUT`, `$HOME`)

**Input has default but is required**: Intentional design pattern for documentation purposes - ignored

**Property is not defined**: False positive for dynamic workflow inputs or secrets - ignored

**Unknown runner label**: Add custom self-hosted runner labels to `.github/actionlint.yaml` under `self-hosted-runner.labels`

## Adding Ignore Patterns

Add patterns to `.github/actionlint.yaml`:

```yaml
paths:
  .github/workflows/*.{yml,yaml}:
    ignore:
      - 'your regex pattern here'  # Uses RE2 regex syntax
```

**Common patterns:**
- `.*` - matches zero or more of any character
- `property.*is not defined` - matches any property name
- `SC2086:.*` - matches all SC2086 shellcheck warnings
- `SC\d+` - matches any shellcheck code

**Tips:**
- Patterns are case-sensitive
- Use RE2 regex syntax (not PCRE or JavaScript regex)
- Test locally with `actionlint -verbose` to confirm patterns work

## Resources

- [Actionlint Documentation](https://github.com/rhysd/actionlint)
- [Shellcheck Wiki](https://www.shellcheck.net/wiki/)
- [RE2 Regex Syntax](https://github.com/google/re2/wiki/Syntax)
- [GitHub Actions Documentation](https://docs.github.com/en/actions)

For issues, run `actionlint -verbose` and check the error message against ignore patterns in `.github/actionlint.yaml`.
