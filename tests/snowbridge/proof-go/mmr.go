// This file is part of midnight-node.
// Copyright (C) Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// You may not use this file except in compliance with the License.
// You may obtain a copy of the License at
// http://www.apache.org/licenses/LICENSE-2.0
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

// Generate real PoS proofs using the pinned Snowbridge SSZ and receipt tooling.
// Include tx_index, required by the pinned SDK but absent from upstream MessageV2.
package main

import (
	"encoding/hex"
	"encoding/json"
	"fmt"
	"github.com/snowfork/go-substrate-rpc-client/v4/types"
	"github.com/snowfork/snowbridge/relayer/crypto/merkle"
	"os"
	"path/filepath"
	"strings"
)

func simplifyMMR(input, out string) error {
	var data struct {
		LeafIndex uint64
		LeafCount uint64
		Items     []string
		LeafHash  string
		Root      string
	}
	bytes, err := os.ReadFile(input)
	if err != nil {
		return err
	}
	if err = json.Unmarshal(bytes, &data); err != nil {
		return err
	}
	hash := func(s string) (types.H256, error) {
		b, e := hex.DecodeString(strings.TrimPrefix(s, "0x"))
		if e != nil || len(b) != 32 {
			return types.H256{}, fmt.Errorf("invalid hash")
		}
		return types.NewH256(b), nil
	}
	items := []types.H256{}
	for _, s := range data.Items {
		h, e := hash(s)
		if e != nil {
			return e
		}
		items = append(items, h)
	}
	leaf, e := hash(data.LeafHash)
	if e != nil {
		return e
	}
	root, e := hash(data.Root)
	if e != nil {
		return e
	}
	proof, err := merkle.ConvertToSimplifiedMMRProof(types.H256{}, data.LeafIndex, types.MMRLeaf{}, data.LeafCount, items)
	if err != nil {
		return err
	}
	if merkle.CalculateMerkleRoot(&proof, leaf) != root {
		return fmt.Errorf("simplified MMR proof does not match signed root")
	}
	paths := []string{}
	for _, h := range proof.MerkleProofItems {
		paths = append(paths, "0x"+hex.EncodeToString(h[:]))
	}
	result, _ := json.MarshalIndent(map[string]interface{}{"items": paths, "order": fmt.Sprint(proof.MerkleProofOrder)}, "", "  ")
	return os.WriteFile(filepath.Join(out, "simplified.json"), result, 0644)
}
