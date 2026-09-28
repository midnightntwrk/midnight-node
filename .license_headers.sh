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

set -euo pipefail

# find, minus trees we must never headerize: git internals, submodules,
# dependency/build output, compactc-generated contracts, and vendored upstream
# partner-chains code (not Midnight Foundation copyright).
src_find() {
	find . \( -path ./.git \
		-o -path ./compact -o -path ./indexer -o -path ./midnight-reserve-contracts \
		-o -path ./partner-chains -o -path ./static/contracts -o -path ./target \
		-o -name node_modules -o -name dist \) -prune \
		-o -type f "$@" -print
}

# Prepend the license header (comment prefix $2) to $1, keeping any shebang on
# line 1; $3 (optional) is a shebang to add when the file has none.
# Returns 1 if the file already has a header.
add_header() {
	local file=$1 prefix=$2 default_shebang=${3:-}
	grep -q "SPDX-License-Identifier" "$file" && return 1
	local tmpfile
	tmpfile=$(mktemp)
	if head -n1 "$file" | grep -q '^#!'; then
		head -n1 "$file" > "$tmpfile"
		sed "s|^|$prefix |" .midnight.txt >> "$tmpfile"
		echo "" >> "$tmpfile"
		tail -n +2 "$file" >> "$tmpfile"
	else
		if [ -n "$default_shebang" ]; then
			echo "$default_shebang" > "$tmpfile"
		fi
		sed "s|^|$prefix |" .midnight.txt >> "$tmpfile"
		echo "" >> "$tmpfile"
		cat "$file" >> "$tmpfile"
	fi
	mv "$tmpfile" "$file"
	echo "headed: $file"
}

src_find -name '*.rs' \( -path '*/build.rs' -o -path '*/src/*' -o -path '*/tests/*' \) |
	while IFS= read -r file; do
		add_header "$file" "//" || true
	done

src_find \( -name '*.js' -o -name '*.ts' \) \
	\( -path '*/src/*' -o -path '*/tests/*' -o -path '*/test/*' -o -path '*/scripts/*' \) |
	while IFS= read -r file; do
		add_header "$file" "//" || true
	done

src_find -name '*.sh' |
	while IFS= read -r file; do
		if add_header "$file" "#" "#!/usr/bin/env bash"; then
			chmod +x "$file"
		fi
	done
