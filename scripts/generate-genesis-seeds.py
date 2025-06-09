# This file is part of midnight-node.
# Copyright (C) 2025 Midnight Foundation
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

import argparse
import json
import os

def init_argparse() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description='Generate random genesis seeds',
    )
    parser.add_argument(
        '-c',
        '--count',
        help='Number of seeds to generate',
        required=True,
        type=int
    )
    parser.add_argument(
        '-o',
        '--out',
        help='Output file',
        required=True,
        type=str
    )
    return parser


def main():
    parser = init_argparse()
    args = parser.parse_args()
    if os.path.exists(args.out):
        print(f"File {args.out} already exists, skipping.")
        return
    # Generate random 32 bytes seeds
    seeds = dict([[f"wallet-seed-{i+1}", os.urandom(32).hex()] for i in range(args.count)])
    with open(args.out, "w") as f:
        json.dump(seeds, f, indent=2)
    print(f"Generated {args.count} seeds to {args.out}")


if __name__ == "__main__":
    main()
