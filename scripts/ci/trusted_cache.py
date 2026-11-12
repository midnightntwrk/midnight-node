#!/usr/bin/env python3
# This file is part of midnight-node.
# Copyright (C) Midnight Foundation
# SPDX-License-Identifier: Apache-2.0

import json
import os
import re
import subprocess
import sys


def trusted_hit(entries, key, repository, pr_repository):
    checked = {}
    for entry in entries:
        if entry.get('key') != key:
            continue
        ref = entry.get('ref', '')
        if ref.startswith('refs/heads/'):
            return True
        match = re.fullmatch(r'refs/pull/(\d+)/merge', ref)
        if match:
            number = match.group(1)
            if number not in checked:
                checked[number] = pr_repository(number)
            if checked[number] == repository:
                return True
    return False


def main():
    key = sys.argv[1]
    repository = os.environ['GITHUB_REPOSITORY']
    entries = json.loads(subprocess.check_output([
        'gh', 'cache', 'list', '--repo', repository, '--key', key,
        '--json', 'key,ref', '--limit', '100',
    ], text=True))

    def pr_repository(number):
        return subprocess.check_output([
            'gh', 'api', f'repos/{repository}/pulls/{number}',
            '--jq', '.head.repo.full_name // ""',
        ], text=True).strip()

    print(int(trusted_hit(entries, key, repository, pr_repository)))


if __name__ == '__main__':
    main()
