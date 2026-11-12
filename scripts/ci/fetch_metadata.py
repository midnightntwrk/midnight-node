#!/usr/bin/env python3
# This file is part of midnight-node.
# Copyright (C) Midnight Foundation
# SPDX-License-Identifier: Apache-2.0

import http.client
import json
from pathlib import Path
import sys
from urllib.parse import urlsplit


def compact(data):
    if not data:
        raise ValueError('Missing SCALE length')
    mode = data[0] & 3
    if mode == 3:
        size = (data[0] >> 2) + 4
        prefix = 1
    else:
        size = 1 << mode
        prefix = 0
    if len(data) < size + prefix:
        raise ValueError('Truncated SCALE length')
    value = int.from_bytes(data[prefix:prefix + size], 'little')
    return (value if mode == 3 else value >> 2), data[prefix + size:]


def decode_metadata(data, version):
    if data[:1] != b'\x01':
        raise ValueError('Requested metadata version is unavailable')
    length, payload = compact(data[1:])
    if len(payload) != length or payload[:5] != b'meta' + bytes([version]):
        raise ValueError('Invalid versioned metadata response')
    return payload


def rpc(url, method, data):
    endpoint = urlsplit(url)
    if (endpoint.scheme != 'http' or endpoint.hostname != '127.0.0.1'
            or endpoint.username or endpoint.password or endpoint.path not in ('', '/')
            or endpoint.query or endpoint.fragment or endpoint.port is None):
        raise ValueError('Metadata RPC requires an explicit loopback HTTP port')
    connection = http.client.HTTPConnection('127.0.0.1', endpoint.port, timeout=30)
    try:
        connection.request('POST', '/', json.dumps({
            'jsonrpc': '2.0', 'id': 1, 'method': 'state_call', 'params': [method, data],
        }), {'Content-Type': 'application/json'})
        response = connection.getresponse()
        if response.status != 200:
            raise ValueError(f'Metadata RPC returned HTTP {response.status}')
        result = json.load(response)
    finally:
        connection.close()
    value = result.get('result')
    if not isinstance(value, str) or not value.startswith('0x'):
        raise ValueError(f'{method} did not return encoded data')
    return bytes.fromhex(value[2:])


def fetch(url):
    # Match Subxt's latest-stable selection; state_getMetadata returns legacy v14.
    count, encoded = compact(rpc(url, 'Metadata_metadata_versions', '0x'))
    if len(encoded) != count * 4:
        raise ValueError('Invalid metadata version list')
    versions = [int.from_bytes(encoded[i:i + 4], 'little') for i in range(0, len(encoded), 4)]
    version = max(v for v in versions if v != 0xffffffff)
    if version > 255:
        raise ValueError('Invalid stable metadata version')
    data = rpc(url, 'Metadata_metadata_at_version', '0x' + version.to_bytes(4, 'little').hex())
    return decode_metadata(data, version)


if __name__ == '__main__':
    Path(sys.argv[2]).write_bytes(fetch(sys.argv[1]))
