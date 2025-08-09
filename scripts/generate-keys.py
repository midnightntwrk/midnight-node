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
import shlex
import os
import subprocess
import json
from collections import namedtuple

DEV_SEEDS_PERM = [
    "//Alice",
    "//Bob",
    "//Charlie",
    "//Dave",
    "//Eve",
    "//Ferdie",
    "//One",
    "//Two",
    "//Three",
    "//Four",
    "//Five",
    "//Six",
    "//Seven",
    "//Eight",
    "//Nine",
    "//Ten",
    "//Eleven",
    "//Twelve",
    "//Thirteen",
    "//Fourteen",
    "//Fifteen",
    "//Sixteen",
    "//Seventeen",
    "//Eighteen",
    "//Nineteen",
    "//Twenty",
]
DEV_SEEDS_REG = [f"//{i}" for i in range(1, 100)]
DEV_SEEDS_BOOT = [f'0x{i:064x}' for i in range(1, 100)]
DEV_SEEDS_VALIDATOR = [f'0x{i + (1 << 252):064x}' for i in range(1, 100)]

# Names for the nodes - species of trees
names = {
    "registrations": "Ash,Baobab,Cedar,Deodar,Elm,Fir,Greenheart,Hawthorn,Ilex,Jacaranda,Kapok,Linden,Magnolia,Nyssa,Olive,Poplar,Quebracho,Rowan,Sycamore,Tamarind,Ulmus,Vitex,Willow,Xylosma,Yellowwood,Zebrawood".split(","),
    "permissioned": "Aspen,Birch,Cherry,Dogwood,Eucalyptus,Fig,Ginkgo,Hemlock,Ironwood,Juniper,Kentia,Locust,Mahogany,Neem,Oak,Pine,Quince,Redwood,Sequoia,Teak,Umbrella,Viburnum,Walnut,Xanthoceras,Yew,Zelkova".split(","),
    "permissioned_dev": "Alice,Bob,Charlie,Dave,Eve,Ferdie,One,Two,Three,Four,Five,Six,Seven,Eight,Nine,Ten,Eleven,Twelve,Thirteen,Fourteen,Fifteen,Sixteen,Seventeen,Eighteen,Nineteen,Twenty".split(","),
}

static_mock = {
    "mainchain_pub_key": "0xd5f64925e8722583ab9f8bb633a6938780873cf59504b3d12527719d3310b0ff",
    "mainchain_signature": "0x9e417ede1ab0d710ce3d5854627baef9b32d4dd69ec75f20d1f28234f167b3f679dadafce995fc63a05eefefa322ebe84b5ffccf526481710d086a16aef61506",
    "sidechain_signature": "0x568e305af7f6d51668a76f3aaa72aa3f0f3554be20f150dcabb759f8bff66b484bf66cf248916a40bda649b3ce302ecaf689b9c7b4abd66b44b2aeb039ff85a5",
    "input_utxo": "ff428f8f916a832d146e58f3656c17c769a6bbc44bba1693fe2a4f9c605b8f16#0",
    "status": "Active"
}

SUBKEY_IMAGE = os.environ.get("SUBKEY_IMAGE", "parity/subkey:3.0.0")

def execute_subkey(script: str):
    cmd = shlex.split(f"docker run --rm --entrypoint sh {SUBKEY_IMAGE} -c") + [script]
    return subprocess.run(cmd, capture_output=True, text=True, check=True)

def take(l, n):
    return [l.pop(0) for _ in range(n)]

def take_until(l, fn):
    lines = []
    while l:
        line = l.pop(0)
        lines.append(line)
        if fn(line):
            return lines
    
    return lines

def take_next_obj(l):
    obj_str = "".join(take_until(l, lambda x: x.startswith("}")))
    return json.loads(obj_str)

def gen_keys(num: int, seeds: list = None):
    if seeds is None:
        # Get random seed phrases
        script = ""
        for i in range(num):
            script += f"subkey generate --output-type json" 
            if i < num - 1:
                script += " && "

        seeds = []
        lines = execute_subkey(script).stdout.split("\n")
        for i in range(num):
            obj = take_next_obj(lines)
            seeds.append(obj["secretPhrase"])

    script = ""
    for i, seed in enumerate(seeds):
        script += f"subkey inspect --scheme sr25519 --output-type json \"{seed}\" && " 
        script += f"subkey inspect --scheme ed25519 --output-type json \"{seed}\" && " 
        script += f"subkey inspect --scheme ecdsa --output-type json \"{seed}\"  " 
        if i < num - 1:
            script += " && "

    out = execute_subkey(script)
    lines = out.stdout.split("\n")

    keys = []
    for seed in seeds:
        sr25519 = take_next_obj(lines)
        ed25519 = take_next_obj(lines)
        ecdsa = take_next_obj(lines)

        keys.append({
            "seed": seed,
            "ss58": sr25519["ss58Address"],
            "secret_key": sr25519["secretSeed"],
            "sr25519": sr25519["publicKey"],
            "ed25519": ed25519["publicKey"],
            "ecdsa": ecdsa["publicKey"],
        })

    return keys

def init_argparse() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description='Generate random mock files',
    )
    parser.add_argument(
        '--dev',
        help='Use dev mode',
        action='store_true'
    )
    parser.add_argument(
        '-r',
        '--num-registrations',
        help='Number of registrations to generate',
        required=True,
        type=int
    )
    parser.add_argument(
        '-p',
        '--num-permissioned',
        help='Number of records to generate',
        required=True,
        type=int
    )
    parser.add_argument(
        '-dr',
        '--d-registered',
        help='Number of registrations to generate',
        required=True,
        type=int
    )
    parser.add_argument(
        '-dp',
        '--d-permissioned',
        help='Number of registrations to generate',
        required=True,
        type=int
    )
    parser.add_argument(
        '-b',
        '--num-boot-nodes',
        help='Number of bootnodes',
        required=True,
        type=int
    )
    parser.add_argument(
        '-v',
        '--num-validator-nodes',
        help='Number of validator nodes',
        required=True,
        type=int
    )
    return parser

Secrets = namedtuple("Secrets", ["reg_secrets", "perm_secrets", "boot_keys", "validator_keys"])

def try_load_keys(args):
    print("Using existing secrets from: secrets/keys-aws.json and secrets/seeds-aws.json")
    with open("secrets/keys-aws.json") as f:
        keys = json.load(f)
    with open("secrets/seeds-aws.json") as f:
        seeds = json.load(f)

    reg_seeds = [v for k, v in seeds.items() if k.startswith("mock-registered-node")]
    perm_seeds = [v for k, v in seeds.items() if k.startswith("node")]
    boot_seeds = [v for k, v in keys.items() if k.startswith("boot-node")]
    validator_seeds = [v for k, v in keys.items() if k.startswith("node")]
    if len(reg_seeds) != args.num_registrations:
        raise ValueError(f"Expected {args.num_registrations} registrations, got {len(reg_seeds)}")
    if len(perm_seeds) != args.num_permissioned:
        raise ValueError(f"Expected {args.num_permissioned} permissioned nodes, got {len(perm_seeds)}")
    if len(boot_seeds) != args.num_boot_nodes:
        raise ValueError(f"Expected {args.num_boot_nodes} boot nodes, got {len(boot_seeds)}")
    if len(validator_seeds) != args.num_validator_nodes:
        raise ValueError(f"Expected {args.num_validator_nodes} validator nodes, got {len(validator_seeds)}")

    seeds = reg_seeds + perm_seeds + boot_seeds + validator_seeds

    keys = gen_keys(len(seeds), seeds)

    return keys


def get_all_secrets(args) -> Secrets:
    # Check if secrets file exists
    keys = None
    if args.dev:
        seeds = DEV_SEEDS_REG[:args.num_registrations] + DEV_SEEDS_PERM[:args.num_permissioned] + DEV_SEEDS_BOOT[:args.num_boot_nodes] + DEV_SEEDS_VALIDATOR[:args.num_validator_nodes]
        keys = gen_keys(len(seeds), seeds)
    else:
        if os.path.exists("secrets/keys-aws.json") and os.path.exists("secrets/seeds-aws.json"):
            try:
                keys = try_load_keys(args)
            except Exception as e:
                print(f"Error loading keys: {e}")

        if keys is None:
            print("Regenerating keys...")
            keys = gen_keys(args.num_registrations + args.num_permissioned + args.num_boot_nodes + args.num_validator_nodes)

    print(keys)

    reg_secrets = take(keys, args.num_registrations)
    perm_secrets = take(keys, args.num_permissioned)
    boot_keys = take(keys, args.num_boot_nodes)
    validator_keys = take(keys, args.num_validator_nodes)
    return Secrets(reg_secrets, perm_secrets, boot_keys, validator_keys)

def load_cli_chain_config_or_default():
    try:
        with open("partner-chains-cli-chain-config.json") as f:
            data = json.load(f)
    except FileNotFoundError:
        data = {}
    data["initial_permissioned_candidates"] = []
    return data

def main():
    parser = init_argparse()
    args = parser.parse_args()

    secrets = get_all_secrets(args)
    partner_chains_cli_chain_config = load_cli_chain_config_or_default()

    seeds = {
        "registrations": [],
        "permissioned": [],
    }
    mock = {
        "registrations": [],
        "permissioned": [],
        "nonce": "0x1234",
        "d_parameter": {
            "registered": args.d_registered,
            "permissioned": args.d_permissioned
        }
    }
    initial_authorities = []
    for i, k in enumerate(secrets.reg_secrets):
        mock["registrations"].append({
            "name": names["registrations"][i],
            "aura_pub_key": k["sr25519"],
            "grandpa_pub_key": k["ed25519"],
            "sidechain_pub_key": k["ecdsa"],
            **static_mock
        })
        seeds["registrations"].append({
            "name": names["registrations"][i],
            "seed_phrase": k["seed"]
        })

    permissioned_names = names["permissioned"]
    if args.dev:
        permissioned_names = names["permissioned_dev"]

    for i, k in enumerate(secrets.perm_secrets):
        mock["permissioned"].append({
            "name": permissioned_names[i],
            "aura_pub_key": k["sr25519"],
            "grandpa_pub_key": k["ed25519"],
            "sidechain_pub_key": k["ecdsa"],
            **static_mock
        })
        seeds["permissioned"].append({
            "name": permissioned_names[i],
            "seed_phrase": k["seed"]
        })
        initial_authorities.append({
            "name": permissioned_names[i],
            "ss58": k["ss58"],
            "aura_pub_key": k["sr25519"],
            "grandpa_pub_key": k["ed25519"],
            "crosschain_pub_key": k["ecdsa"],
        })
        partner_chains_cli_chain_config["initial_permissioned_candidates"].append({
            "sidechain_pub_key": k["ecdsa"],
            "aura_pub_key": k["sr25519"],
            "grandpa_pub_key": k["ed25519"],
        })

    seeds_aws = dict(
        [[f"mock-registered-node-{i+1:02d}", s["seed_phrase"]] for i, s in enumerate(seeds["registrations"])] +
        [[f"node-{i+1:02d}", s["seed_phrase"]] for i, s in enumerate(seeds["permissioned"])]
    )

    keys_aws = dict(
        [[f"boot-node-{i+1:02d}", k["secret_key"]] for i, k in enumerate(secrets.boot_keys)] +
        [[f"node-{i+1:02d}", k["secret_key"]] for i, k in enumerate(secrets.validator_keys)]
    )

    os.makedirs("artifacts", exist_ok=True)
    os.makedirs("secrets", exist_ok=True)

    with open("artifacts/mock.json", "w") as f:
        f.write(json.dumps([mock], indent=2))
    print("Mock file saved:               artifacts/mock.json")

    with open("artifacts/initial-authorities.json", "w") as f:
        f.write(json.dumps(initial_authorities, indent=2))
    print("Initial Authorities saved:     artifacts/initial-authorities.json")

    with open("artifacts/partner-chains-cli-chain-config.json", "w") as f:
        f.write(json.dumps(partner_chains_cli_chain_config, indent=2))
    print("Partner Chains Config saved:   artifacts/partner-chains-cli-chain-config.json")

    if not args.dev:
        with open("secrets/seeds-aws.json", "w") as f:
            f.write(json.dumps(seeds_aws, indent=2))
        print("Seeds (AWS secret) saved:     secrets/seeds-aws.json")

        with open("secrets/keys-aws.json", "w") as f:
            f.write(json.dumps(keys_aws, indent=2))
        print("Keys (AWS secret) saved:      secrets/keys-aws.json")


if __name__ == '__main__':
    main()
