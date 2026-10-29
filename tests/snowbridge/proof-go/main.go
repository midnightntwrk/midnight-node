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
	"context"
	"encoding/hex"
	"encoding/json"
	"errors"
	"flag"
	"fmt"
	"github.com/ethereum/go-ethereum/common"
	etypes "github.com/ethereum/go-ethereum/core/types"
	"github.com/ethereum/go-ethereum/ethclient"
	"github.com/snowfork/go-substrate-rpc-client/v4/types"
	"github.com/snowfork/snowbridge/relayer/chain/ethereum"
	"github.com/snowfork/snowbridge/relayer/relays/beacon/cache"
	"github.com/snowfork/snowbridge/relayer/relays/beacon/config"
	"github.com/snowfork/snowbridge/relayer/relays/beacon/header/syncer"
	"github.com/snowfork/snowbridge/relayer/relays/beacon/header/syncer/api"
	bscale "github.com/snowfork/snowbridge/relayer/relays/beacon/header/syncer/scale"
	"github.com/snowfork/snowbridge/relayer/relays/beacon/protocol"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"time"
)

type eventLog struct {
	Address types.H160
	Topics  []types.H256
	Data    types.Bytes
	TxIndex types.U64
}
type proof struct {
	ReceiptProof   []types.Bytes
	ExecutionProof bscale.HeaderUpdatePayload
}
type eventProof struct {
	EventLog eventLog
	Proof    proof
}
type loader struct{ client *ethclient.Client }

func (l loader) GetBlock(ctx context.Context, hash common.Hash) (*etypes.Block, error) {
	return l.client.BlockByHash(ctx, hash)
}
func (l loader) GetAllReceipts(ctx context.Context, block *etypes.Block) (etypes.Receipts, error) {
	receipts := make(etypes.Receipts, len(block.Transactions()))
	for i, tx := range block.Transactions() {
		receipt, err := l.client.TransactionReceipt(ctx, tx.Hash())
		if err != nil {
			return nil, err
		}
		if receipt.BlockHash != block.Hash() || receipt.TransactionIndex != uint(i) {
			return nil, errors.New("receipt block/index mismatch")
		}
		receipts[i] = receipt
	}
	return receipts, nil
}
func save(out, name string, wire, value interface{}) error {
	encoded, err := types.EncodeToBytes(wire)
	if err != nil {
		return err
	}
	if err = os.WriteFile(filepath.Join(out, name+".scale"), encoded, 0644); err != nil {
		return err
	}
	data, err := json.MarshalIndent(value, "", "  ")
	if err != nil {
		return err
	}
	return os.WriteFile(filepath.Join(out, name+".json"), append(data, '\n'), 0644)
}
func run() error {
	mode := flag.String("mode", "checkpoint", "checkpoint, receipt, or mmr")
	mmrInput := flag.String("mmr-input", "", "JSON MMR proof returned by the live node")
	beacon := flag.String("beacon", "http://127.0.0.1:19596", "beacon REST URL")
	execution := flag.String("execution", "http://127.0.0.1:18545", "execution RPC URL")
	out := flag.String("out", "", "new output directory")
	txHex := flag.String("tx", "", "transaction hash containing the gateway event")
	logIndex := flag.Uint("log-index", 0, "block-global log index")
	flag.Parse()
	if *out == "" {
		return errors.New("--out is required")
	}
	if err := os.MkdirAll(*out, 0755); err != nil {
		return err
	}
	entries, err := os.ReadDir(*out)
	if err != nil {
		return err
	}
	if len(entries) != 0 {
		return errors.New("output directory must be empty")
	}
	if *mode == "mmr" {
		return simplifyMMR(*mmrInput, *out)
	}
	settings := config.SpecSettings{SyncCommitteeSize: 512, SlotsInEpoch: 32, EpochsPerSyncCommitteePeriod: 256, ForkVersions: config.ForkVersions{Deneb: 0, Electra: 0, Fulu: 100000000}}
	clientAPI := api.NewBeaconClient(*beacon)
	parameters := protocol.New(settings, 1024)
	s := syncer.New(clientAPI, parameters, &localStateProofs{client: clientAPI, protocol: parameters})
	if *mode == "checkpoint" {
		checkpoint, err := s.GetCheckpoint()
		if err != nil {
			return err
		}
		if uint64(checkpoint.Header.Slot) == 0 {
			return errors.New("wait for a non-genesis finalized checkpoint")
		}
		return save(*out, "checkpoint", checkpoint, checkpoint.ToJSON())
	}
	if *mode != "receipt" {
		return errors.New("unknown mode")
	}
	raw, err := hex.DecodeString(strings.TrimPrefix(*txHex, "0x"))
	if err != nil || len(raw) != 32 {
		return errors.New("--tx must be a 32-byte transaction hash")
	}
	ctx, cancel := context.WithTimeout(context.Background(), 15*time.Minute)
	defer cancel()
	client, err := ethclient.DialContext(ctx, *execution)
	if err != nil {
		return err
	}
	defer client.Close()
	receipt, err := client.TransactionReceipt(ctx, common.BytesToHash(raw))
	if err != nil {
		return err
	}
	if receipt.Status != 1 {
		return errors.New("transaction reverted")
	}
	var event *etypes.Log
	for _, log := range receipt.Logs {
		if log.Index == *logIndex {
			event = log
			break
		}
	}
	if event == nil || event.Removed {
		return errors.New("log not found or removed")
	}
	var update bscale.Update
	var finalizedSlot uint64
	for {
		update, err = s.GetFinalizedUpdate()
		if errors.Is(err, syncer.ErrSyncCommitteeNotSuperMajority) {
			select {
			case <-ctx.Done():
				return ctx.Err()
			case <-time.After(10 * time.Second):
				continue
			}
		}
		if err != nil {
			return fmt.Errorf("finalized update: %w", err)
		}
		finalizedSlot = uint64(update.Payload.FinalizedHeader.Slot)
		block, err := s.Client.GetBeaconBlockBySlot(finalizedSlot)
		if err != nil {
			return err
		}
		number, err := strconv.ParseUint(block.Data.Message.Body.ExecutionPayload.BlockNumber, 10, 64)
		if err != nil {
			return err
		}
		if number >= event.BlockNumber {
			break
		}
		fmt.Fprintf(os.Stderr, "Waiting for finalized execution block %d (currently %d)\n", event.BlockNumber, number)
		select {
		case <-ctx.Done():
			return ctx.Err()
		case <-time.After(10 * time.Second):
		}
	}
	var foundSlot uint64
	for slot := finalizedSlot; slot > 0; slot-- {
		block, err := s.Client.GetBeaconBlockBySlot(slot)
		if errors.Is(err, api.ErrNotFound) {
			continue
		}
		if err != nil {
			return err
		}
		payload := block.Data.Message.Body.ExecutionPayload
		if common.HexToHash(payload.BlockHash) == event.BlockHash {
			foundSlot = slot
			break
		}
		number, err := strconv.ParseUint(payload.BlockNumber, 10, 64)
		if err != nil {
			return err
		}
		if number < event.BlockNumber {
			return errors.New("event is not in the finalized canonical chain")
		}
	}
	if foundSlot == 0 {
		return errors.New("execution block not found in beacon chain")
	}
	checkpoint := cache.Proof{FinalizedBlockRoot: update.FinalizedHeaderBlockRoot, BlockRootsTree: update.BlockRootsTree, Slot: finalizedSlot}
	executionProof, err := s.GetHeaderUpdateBySlotWithCheckpoint(foundSlot, &checkpoint)
	if err != nil {
		return err
	}
	headers, err := ethereum.NewHeaderBlockCache(loader{client})
	if err != nil {
		return err
	}
	trie, err := headers.GetReceiptTrie(ctx, event.BlockHash)
	if err != nil {
		return err
	}
	message, err := ethereum.MakeMessageFromEvent(event, trie)
	if err != nil {
		return err
	}
	wire := eventProof{eventLog{message.EventLog.Address, message.EventLog.Topics, message.EventLog.Data, types.U64(event.TxIndex)}, proof{message.Proof.ReceiptProof.Values, executionProof}}
	if err = save(*out, "update", update.Payload, update.Payload.ToJSON()); err != nil {
		return err
	}
	if err = save(*out, "event", wire, wire); err != nil {
		return err
	}
	evidence := map[string]interface{}{"transaction": event.TxHash.Hex(), "block": event.BlockHash.Hex(), "executionBlock": event.BlockNumber, "transactionIndex": event.TxIndex, "logIndex": event.Index, "beaconSlot": foundSlot, "finalizedSlot": finalizedSlot, "finalizedRoot": update.FinalizedHeaderBlockRoot.Hex(), "runtimeReceiptVerified": false, "completeBridgeE2E": false}
	data, err := json.MarshalIndent(evidence, "", "  ")
	if err != nil {
		return err
	}
	return os.WriteFile(filepath.Join(*out, "evidence.json"), append(data, '\n'), 0644)
}
func main() {
	if err := run(); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}
