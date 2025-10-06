use std::collections::HashMap;

type LeafIndex = usize;

struct Branch {
	indices: Vec<LeafIndex>,
}

fn recurse_proof(
	layer: &[usize],
	chosen_indices: &[usize],
	leaves_len: usize,
	previous_layer_leaves: &[usize],
) {
	let mut needed_leaves = neighbor_indices(chosen_indices, leaves_len);
	// remove duplicate leaves that existed on the previous layer
	needed_leaves.retain(|leaf| !previous_layer_leaves.contains(leaf));

	let layer = 0;
	let next_needed: Vec<usize> = needed_leaves.iter().map(|leaf| leaf / 2).collect();
	let next_layer = layer + 1;

	// let next_needed = needed_leaves.iter().
}

fn neighbor_indices(chosen_indices: &[usize], leaves_len: usize) -> Vec<usize> {
	let mut neighbors = vec![];

	for index in chosen_indices {
		// for every even-numbered index
		if index % 2 == 0 {
			if *index != leaves_len - 1 {
				// push the neighbor on the right
				neighbors.push(*index + 1);
			}
		}
		// for every odd-numbered index
		else {
			neighbors.push(*index - 1);
		}
	}

	neighbors
}

#[cfg(test)]
mod test {
	use crate::merkle::neighbor_indices;

	#[test]
	fn test_neighbor_indices() {
		// EVEN-NUMBERED INDICES
		let even_max_leaves = 10;
		let chosen_indices = [0, 7, 9];

		let expected_indices = [1, 6, 8];
		let actual_indices = neighbor_indices(&chosen_indices, even_max_leaves);
		assert_eq!(actual_indices, expected_indices);

		// ODD-NUMBERED INDICES
		let odd_max_leaves = 11;

		let actual_indices = neighbor_indices(&chosen_indices, odd_max_leaves);
		assert_eq!(actual_indices, expected_indices);
	}
}
