# Ariadne Selection Emergency Override

#### status: accepted
#### date: 2024-06-28
#### deciders: Justin Frevert, Oscar Bailey, Giles Cope

## Context and Problem Statement
### Problem
Midnight devnet has an unhealthy validator set as of the time of writing, which has lead to consensus related issues affecting block production and finalization. This set is determined by the "D-Parameter" set in the sidechains contracts that govern a given sidechain. The cli tool designated to be used for updating the d parameter was also unusable in this situation, due to a somewhat related Cardano node outage. Specifically, this tool needed to be updated to match a Cardano node update.

### Risk
Arguably, the dependence on this tool is an unmitigated risk, where the potential impact of not managing the risk is the inability to fix the validator set in a critical moment. Additionally, given an update to the d-parameter, there is still an uncomfortably long waiting period for the change to enact and finally reflect in the Midnight validator set.

It might be argued that the above represents a bottle-neck of tooling, and that there need to be additional options to resolve outage scenarios quickly. There should be a tool that is at least active in devnet, and is managed on the Midnight side for reacting to these scenarios by quickly overriding the Ariadne selection algorithm and D-Parameter by falling back to a selected committee.

Some additional context is that there is a more formal set of planning which will be done around a process for potential critical scenarios in the future. 

## Considered Options

* Emergency Ariadne selection override mode
* Hard-coded d-parameter in node
* Constant manual invocation of a set change
* Ignore block finality
* Emergency D Parameter override mode

## Consequences of the Options
### Emergency Ariadne selection override mode

- Resolution of consensus issues stemming from a weak validator set
- Faster changes to validator set
- Additional tool for testing Ariadne and its effects on Midnight
- Centralization risk around override transaction
- Potential unexpected state in validator set
- Potential source of clashing information(onchain validator set versus publicly available d parameter)

### Hard-coded d-parameter in node

- Simple change to pass and deploy
- Potential unexpected state in validator set
- Potential source of clashing information(onchain validator set versus publicly available d parameter)

### Ignore block finality

- Assumes that finality is not very critical for PoA consensus
- Arguably avoids the issue and valuable exercise of debugging finality in devnet
- Is complex to do in downstream components at this time.

### Constant manual invocation of a set change
- Error-prone
- Relies on infrastructure to run, which is complex and a massive centralization risk

## Decision Outcome

Chosen option: "Emergency Ariadne selection override mode", because of a need for timely functioning of the network, and the ability to alter that setting if needed. This decision involves the implementation of the mechanism described onchain. The mechanism will later be superceded by a more formal solution for resolving Cardano communication issue scenarios. 

## Decision Drivers
* Midnight suffered an outage on devnet when the D-parameter could not be changed to a healthier ratio, due to the sidechains release cycle. The only options for resolution were manual calls of elevated privilege, and this.
