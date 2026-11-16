#!/usr/bin/env python3
# This file is part of midnight-node.
# Copyright (C) Midnight Foundation
# SPDX-License-Identifier: Apache-2.0

import json
import os
from pathlib import Path
import re


def funding_ready(utxos, workers, minimum, fee_buffer):
    amounts = [utxo['value']['ada']['lovelace'] for utxo in utxos]
    if any(type(amount) is not int or amount < 0 for amount in amounts):
        raise ValueError('Invalid lovelace amount from Ogmios')
    return (sum(amount >= minimum for amount in amounts) >= workers
            or sum(amounts) >= workers * minimum + fee_buffer), sum(amounts)


def settings():
    config = Path('tests/e2e/src/config.rs').read_text()
    faucet = Path('tests/e2e/src/faucet.rs').read_text()
    address = re.search(r'funded_address:\s*"([^"]+)"', config).group(1)
    endpoint = re.search(r'base_url:\s*"(wss://ogmios.devnet[^" ]+)"', config).group(1)
    # Match the numeric shared-network constant; local networks use REQUEST_CAP_LOVELACE.
    minimum = re.search(r'const MIN_WORKER_LOVELACE: u64 = ([\d_]+);', faucet).group(1)
    buffer = re.search(r'const PRIME_FEE_BUFFER: u64 = ([\d_]+);', faucet).group(1)
    return endpoint, address, int(minimum.replace('_', '')), int(buffer.replace('_', ''))


def main():
    import websocket
    workers = int(os.environ['E2E_FAUCET_WORKERS'])
    if workers < 1:
        raise ValueError('E2E_FAUCET_WORKERS must be positive')
    endpoint, address, minimum, buffer = settings()
    connection = websocket.create_connection(endpoint, timeout=30)
    try:
        connection.send(json.dumps({
            'jsonrpc': '2.0', 'id': 'faucet-preflight',
            'method': 'queryLedgerState/utxo', 'params': {'addresses': [address]},
        }))
        response = json.loads(connection.recv())
    finally:
        connection.close()
    if response.get('id') != 'faucet-preflight' or not isinstance(response.get('result'), list):
        raise ValueError('Ogmios did not return the requested faucet UTXOs')
    ready, total = funding_ready(response['result'], workers, minimum, buffer)
    print(f'Faucet balance: {total / 1_000_000:g} ADA; {workers} workers require '
          f'{minimum / 1_000_000:g} ADA each or {(workers * minimum + buffer) / 1_000_000:g} ADA to prime')
    if not ready:
        raise SystemExit('::error::Devnet faucet needs funding before observation tests can run')


if __name__ == '__main__':
    main()
