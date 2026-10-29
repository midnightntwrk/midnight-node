// This file is part of midnight-node.
// Copyright (C) Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// You may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

package main

import (
	"bytes"
	"fmt"
	ssz "github.com/ferranbt/fastssz"
	"github.com/snowfork/go-substrate-rpc-client/v4/types"
	"github.com/snowfork/snowbridge/relayer/relays/beacon/header/syncer/api"
	bscale "github.com/snowfork/snowbridge/relayer/relays/beacon/header/syncer/scale"
	"github.com/snowfork/snowbridge/relayer/relays/beacon/protocol"
	"github.com/snowfork/snowbridge/relayer/relays/beacon/state"
	"strconv"
)

// The small local validator set lets us build the complete SSZ tree directly.
// State is checked against its beacon header before generating any proof.
type localStateProofs struct {
	client   *api.BeaconClient
	protocol *protocol.Protocol
}

func (l *localStateProofs) Health() error { _, err := l.client.GetGenesis(); return err }
func (l *localStateProofs) load(slot uint64) (state.BeaconState, *ssz.Node, error) {
	data, err := l.client.GetBeaconState(strconv.FormatUint(slot, 10))
	if err != nil {
		return nil, nil, err
	}
	if l.protocol.ForkVersion(slot) != protocol.Electra {
		return nil, nil, fmt.Errorf("local proof helper requires Electra")
	}
	st := &state.BeaconStateElectra{}
	if err = st.UnmarshalSSZ(data); err != nil {
		return nil, nil, err
	}
	if st.GetSlot() != slot {
		return nil, nil, fmt.Errorf("beacon state slot mismatch")
	}
	tree, err := st.GetTree()
	if err != nil {
		return nil, nil, err
	}
	header, err := l.client.GetHeaderBySlot(slot)
	if err != nil {
		return nil, nil, err
	}
	if !bytes.Equal(tree.Hash(), header.StateRoot[:]) {
		return nil, nil, fmt.Errorf("beacon state root mismatch")
	}
	return st, tree, nil
}
func hashes(values [][]byte) []types.H256 {
	result := make([]types.H256, len(values))
	for i, v := range values {
		copy(result[i][:], v)
	}
	return result
}
func (l *localStateProofs) GetBlockRootProof(slot uint64) (*bscale.BlockRootProof, error) {
	st, tree, err := l.load(slot)
	if err != nil {
		return nil, err
	}
	branch, err := tree.Prove(l.protocol.BlockRootGeneralizedIndex(slot))
	if err != nil {
		return nil, err
	}
	container := &state.BlockRootsContainerMainnet{BlockRoots: st.GetBlockRoots()}
	roots, err := container.GetTree()
	if err != nil {
		return nil, err
	}
	if !bytes.Equal(roots.Hash(), branch.Leaf) {
		return nil, fmt.Errorf("block roots mismatch")
	}
	var leaf types.H256
	copy(leaf[:], branch.Leaf)
	return &bscale.BlockRootProof{Leaf: leaf, Proof: hashes(branch.Hashes), Tree: roots}, nil
}
func (l *localStateProofs) GetFinalizedHeaderProof(slot uint64) ([]types.H256, error) {
	_, tree, err := l.load(slot)
	if err != nil {
		return nil, err
	}
	branch, err := tree.Prove(l.protocol.FinalizedCheckpointGeneralizedIndex(slot))
	if err != nil {
		return nil, err
	}
	return hashes(branch.Hashes), nil
}
func (l *localStateProofs) GetSyncCommitteeProof(slot uint64, period string) (*bscale.SyncCommitteeProof, error) {
	st, tree, err := l.load(slot)
	if err != nil {
		return nil, err
	}
	index := l.protocol.CurrentSyncCommitteeGeneralizedIndex(slot)
	committee := st.GetCurrentSyncCommittee()
	if period == "next" {
		index = l.protocol.NextSyncCommitteeGeneralizedIndex(slot)
		committee = st.GetNextSyncCommittee()
	} else if period != "current" {
		return nil, fmt.Errorf("invalid committee period")
	}
	branch, err := tree.Prove(index)
	if err != nil {
		return nil, err
	}
	keys := make([][48]byte, len(committee.PubKeys))
	for i, key := range committee.PubKeys {
		copy(keys[i][:], key)
	}
	return &bscale.SyncCommitteeProof{Pubkeys: keys, AggregatePubkey: committee.AggregatePubKey, Proof: hashes(branch.Hashes)}, nil
}
