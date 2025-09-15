# Configuration System

#### status: accepted
#### date: 2024-07-17
#### deciders: Oscar Bailey, Justin Frevert, Jegor Sidorenko, Giles Cope

## Context and Problem Statement

I proposed a change to our configuration system as detailed
[here](../proposals/0001-configuration-overhaul.md). In summary, the proposal
addresses the configuration complexity of the node. This ADR documents the
difference between the proposed solution and the implementation.

## Decision Drivers

(For more detail, see the proposal)

* Several recent deployment failures occurred due to configuration errors
* Too many sources of truth for configuration - a config variable can be set in too many places
* No consolidated list of configuration variables can be found in the repo

## Considered Options

* 12 Factor application approach - everything via env-vars, no defaults
* Well defined defaults for common use-cases
* Keep the existing substrate CLI, configure everything else with environment variables

## Decision Outcome

Chosen option: "Keep the existing substrate CLI, configure everything else with
environment variables", because while configuring everything with environment
variables is cleaner it would require too much maintenance to maintain
functional equivalence with the Substrate CLI.

The work was attempted, but after review it was abandoned. This is because it
required a duplication of the Substrate `RunCmd` struct including all nested
types, validation rules, and default values. This was assessed as too much
maintenance cost.

We also decided not to go with using too many defaults, since this is another
source of possible complexity. Ideally, we should reduce the number of
variables we need to configure in the first place. If variables vary between
deployments, they should not have a default.

In future, we will open a pull request on the polkadot SDK to enable using
Substrate in a more 12-Factor compatible way. This would involve a small change
to their code; we'd need to add an `env` attribute to each of their `clap`
arguments.

The configuration crate we used is `config-rs`, combined with `serde` and
`serde_valid` for deserialization and validation. This provides us with a
process for adding new configuration variables, a new CLI command that lists
where each config variable has come from, and validation of config variables at
start-up.

### Positive Consequences

* Added a process for adding new configuration variables
* Added a CLI command to show current configuration and source of configuration
* Validation of configuration variables at start-up

### Negative Consequences

* Additional code complexity in the repository

## Validation

Measurable outcomes

- A reduction in the number of bugs caused by configuration issues
- Quicker support cycle for SPOs (using the show-config feature)

## Pros and Cons of the Options

### 12 Factor application approach - everything via env-vars, no defaults

[Description](https://12factor.net/config)

* Good, because it's a standard approach
* Bad, because Substrate does not natively support configuration with environment variables

### Well defined defaults for common use-cases

Idea: Aggressively set defaults for common use-cases
Example: SPOs can start-up a node image with a single command

* Good, because it reduces complexity for SPOs
* Bad, because it mixes deployment config and application code
* Bad, because it's another source of configuration, and therefore increases configuration complexity

### Keep the existing substrate CLI, configure everything else with environment variables

Idea: Configure everything with 12 Factor methodology in mind, but reduce the maintenance cost by keeping the substrate CLI

* Good, because the Substrate CLI is well-tested
* Neutral, because we can change this approach in future if Substrate adds compatibility
* Bad, because there's no unified way of setting environment variables
