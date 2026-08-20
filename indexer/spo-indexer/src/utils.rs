// This file is part of midnight-indexer.
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

pub fn remove_hex_prefix(s: &str) -> &str {
    s.strip_prefix("0x").unwrap_or(s)
}

pub fn hex_to_bytes(s: &str) -> Vec<u8> {
    let hex = remove_hex_prefix(s);
    const_hex::decode(hex).expect("input should be valid hex string")
}
