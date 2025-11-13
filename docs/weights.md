---
title: Block Weights
---

This document describes the concepts of transaction and block weights.
Substrate defines one unit of weight as one picosecond of execution time 
on reference hardware. So each transaction is subject to an estimation of 
how long it will run on some upfront specified reference hardware. 
On the other hand, the total limit on a maximal weight of a single block 
defines how long it should take to compute all transactions in it. 
This upper limit and a predefined timeout achieve the desired average block time.


## Transaction dispatch classes
Substrate distinguishes between three categories of transactions:
- Normal: external user blockchain transactions,
- Operational: internal operational transactions,
- Mandatory: high-priority transactions which are are always included regardless of their weight.

Defining a specific upper limit of the sum of transaction weights is possible for 
each dispatch class. Though it's highly not recommended to set such a limit on the 
Mandatory class. However, the total maximal block weight precedes the class weight limits. 
To prevent that one class (most notably Normal) exhausts the allowed block weight, 
reserving some minimal space for each dispatch class (mainly Operational) is possible.

Example configuration of `BlockWeights` with a fixed amount of Midnight transactions
and a 2 seconds upper bound for the total block execution time:
```rust
// WEIGHT_REF_TIME_PER_SECOND = 1s
// NORMAL_DISPATCH_RATIO = 75%
// Replace the ref_time with a fixed weight from the Midnight pallet
fn block_weights_fixed_size() -> frame_system::limits::BlockWeights {
	let expected_block_weight: Weight =
		Weight::from_parts(2u64 * WEIGHT_REF_TIME_PER_SECOND, u64::MAX);
	let normal_weight = NORMAL_DISPATCH_RATIO * expected_block_weight;
	normal_weight.set_ref_time(pallet_midnight::FIXED_MN_BLOCK_WEIGHT);
	frame_system::limits::BlockWeights::builder()
		.for_class(frame_support::dispatch::DispatchClass::Normal, |weights| {
			weights.max_total = normal_weight.into();
		})
		.for_class(frame_support::dispatch::DispatchClass::Operational, |weights| {
			weights.max_total = expected_block_weight.into();
			weights.reserved = (expected_block_weight - normal_weight).into();
		})
		.avg_block_initialization(Perbill::from_percent(10))
		.build()
		.expect("Sensible defaults are tested to be valid; qed")
}
```

## Transaction weights
In each Substrate pallet, it's possible to define multiple entry points (Calls) to 
interact with the blockchain. In the simplest case, one would specify a single entry 
point for submitting any (pallet-specific) transactions. Providing more than one entry 
point allows the node to distinguish between different kinds of transactions at the top 
level instead of encoding this information into the transactions and dispatching it elsewhere. 
Each (entry point) transaction type can be assigned a static or weight of execution time in 
picoseconds. Adding a storage overhead to the weight regarding the anticipated reads and 
writes is possible. On the other hand, it's possible to define a pallet-specific custom 
weight model and calculate the weight of each transaction dynamically. Though the calculation 
of weights should remain very lightweight and constant time because it will be performed 
for each transaction.

To calculate an appropriate weight for a transaction, one can use benchmark parameters to 
measure the time it takes to execute the function calls on different hardware, using different
variable values, and repeated multiple times. One can then use the results of the benchmarking 
tests to establish an approximate worst case weight to represent the resources required to execute 
each function call and each code path.

## External references
- [Unit of weight in Substrate](https://docs.substrate.io/reference/glossary/#weight)
- [Default weight annotations](https://docs.substrate.io/build/tx-weights-fees/#default-weight-annotations)
- [BlockWeights API](https://docs.rs/frame-system/latest/frame_system/limits/struct.BlockWeights.html)
- [BlockWeightsBuilder API](https://docs.rs/frame-system/latest/frame_system/limits/struct.BlockWeightsBuilder.html)
- [Custom Weights](https://docs.substrate.io/reference/how-to-guides/weights/use-custom-weights/)
- [Benchmarks](https://docs.substrate.io/test/benchmark/)