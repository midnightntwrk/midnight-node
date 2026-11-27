# This file is part of midnight-node.
# Copyright (C) Midnight Foundation
# SPDX-License-Identifier: Apache-2.0
# Licensed under the Apache License, Version 2.0 (the "License");
# You may not use this file except in compliance with the License.
# You may obtain a copy of the License at
# http://www.apache.org/licenses/LICENSE-2.0
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

"""A toolkit .mn transaction as the bare Midnight.send_mn_transaction extrinsic, in hex.

  extrinsic.py <tx.mn> [--corrupt]

Bare extrinsics carry no era, nonce or genesis hash, so the bytes stay the same across the
fork. --corrupt flips one byte in the middle of the ledger payload, past its header tag.
"""
import json
import sys

PALLET, CALL = 5, 0  # Midnight.send_mn_transaction in both runtimes


def compact(n):
    if n < 1 << 6:
        return bytes([n << 2])
    if n < 1 << 14:
        return (n << 2 | 1).to_bytes(2, "little")
    if n < 1 << 30:
        return (n << 2 | 2).to_bytes(4, "little")
    b = n.to_bytes((n.bit_length() + 7) // 8, "little")
    return bytes([(len(b) - 4) << 2 | 3]) + b


tx = bytearray.fromhex(json.load(open(sys.argv[1]))["tx"]["Midnight"])
if "--corrupt" in sys.argv[2:]:
    start = tx.index(b"):") + 2
    tx[start + (len(tx) - start) // 2] ^= 0xFF
body = bytes([4, PALLET, CALL]) + compact(len(tx)) + bytes(tx)
print((compact(len(body)) + body).hex())
