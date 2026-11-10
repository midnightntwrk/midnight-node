#!/usr/bin/env bash

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

# Turn ROOT, holding only the node binaries at its top level, into the whole
# filesystem of a `FROM scratch` image.
#
# Run it in the image the binaries were built in: the shared libraries are copied
# from there, so they are exactly the versions the binaries were linked against.
#
# Usage: node-rootfs.sh ROOT

set -euo pipefail

root="$1"

bins=()
for f in "$root"/*; do
    [ -f "$f" ] && [ -x "$f" ] && bins+=("$f")
done
[ "${#bins[@]}" -gt 0 ] || { echo "no binaries in $root" >&2; exit 1; }

# ldd prints the ELF interpreter and every library as an absolute path.
ldd "${bins[@]}" | grep -o '/[^ ]*\.so[^ ]*' | sort -u | while read -r lib; do
    cp --parents -L "$lib" "$root"
done

cp --parents -L /etc/pki/tls/certs/ca-bundle.crt "$root"

mkdir -p "$root/etc" "$root/tmp" "$root/node/chain"
chmod 1777 "$root/tmp"
printf 'root:x:0:0:root:/root:/sbin/nologin\nappuser:x:10001:10001::/nonexistent:/sbin/nologin\n' >"$root/etc/passwd"
printf 'root:x:0:\nappuser:x:10001:\n' >"$root/etc/group"
# Without this file glibc asks DNS before /etc/hosts.
echo 'hosts: files dns' >"$root/etc/nsswitch.conf"

# Fails here, not at container start, if a library is missing.
chroot "$root" /midnight-node --version
