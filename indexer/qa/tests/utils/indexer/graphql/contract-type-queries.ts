// This file is part of midnightntwrk/midnight-indexer
// Copyright (C) Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// You may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

// GraphQL document for the top-level Contract type and contract(address, offset)
// query (#1275): the contract as the topmost concept, with point-in-time state,
// maintenance authority and a bounded recent-actions sub-query.
export const GET_CONTRACT = `
query Contract($ADDRESS: HexEncoded!, $OFFSET: BlockOffset, $ACTIONS_LIMIT: Int, $ACTIONS_TYPE: ContractActionType) {
  contract(address: $ADDRESS, offset: $OFFSET) {
    address
    state
    maintenanceAuthority {
      committee {
        kind
        key
      }
      threshold
      counter
    }
    actions(limit: $ACTIONS_LIMIT, type: $ACTIONS_TYPE) {
      __typename
      address
    }
  }
}`;
