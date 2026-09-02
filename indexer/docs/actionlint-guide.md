# Actionlint Guide

## What is Actionlint?

[Actionlint](https://github.com/rhysd/actionlint) is a static checker for GitHub Actions workflow files that validates syntax, checks for common mistakes, and integrates with shellcheck for bash script validation. It runs automatically on PRs that modify workflow files.

## Configuration

### Workflow File
`.github/workflows/actionlint.yaml` - Runs on every PR that modifies workflow files

### Config File
`.github/actionlint.yaml` - Contains custom runner labels and ignore patterns:

```yaml
self-hosted-runner:
  labels:
    - ubuntu-latest-8-core-x64
    - ubuntu-latest-16-core-x64

paths:
  .github/workflows/*.{yml,yaml}:
    ignore:
      - 'default value.*but.*also required'
      - 'property.*is not defined in object type'
```

**Why these patterns?**
- `default value.*but.*also required` - Workflow_call inputs with defaults that are also required (intentional design)
- `property.*is not defined in object type` - Secret references that actionlint cannot verify

## Running Actionlint Locally

### Install
```bash
# macOS
brew install actionlint

# Linux
curl -s https://raw.githubusercontent.com/rhysd/actionlint/main/scripts/download-actionlint.bash | bash
```

### Run
```bash
actionlint                                    # Check all workflows
actionlint .github/workflows/ci-cloud.yaml   # Check specific file
actionlint -verbose                           # Verbose output
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

**Unknown Runner Label** - Add custom runner labels to `.github/actionlint.yaml` under `self-hosted-runner.labels`

**Property Not Defined** - Add ignore patterns for secrets (runtime values actionlint can't verify)

**Shellcheck SC2086** - Quote variables or use ignore patterns for GitHub Actions variables that don't require quoting

## Adding Ignore Patterns

Add patterns to `.github/actionlint.yaml` under `paths..github/workflows/*.{yml,yaml}.ignore`:
```yaml
ignore:
  - 'your regex pattern here'  # Uses RE2 regex syntax
```

Common patterns: `.*` (any chars), `property.*is not defined`, `SC2086:.*`

## Resources

- [Actionlint Documentation](https://github.com/rhysd/actionlint)
- [RE2 Regex Syntax](https://github.com/google/re2/wiki/Syntax)
- [Shellcheck Wiki](https://www.shellcheck.net/wiki/)

For issues, run `actionlint -verbose` and check the error message against ignore patterns.
