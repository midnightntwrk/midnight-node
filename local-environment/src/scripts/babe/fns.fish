# This file is part of midnight-node.
# Copyright (C) Midnight Foundation
# SPDX-License-Identifier: Apache-2.0
# Licensed under the Apache License, Version 2.0 (the "License");
# You may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#	http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

# Source from local-environment/: source src/scripts/babe/fns.fish
# BASE / V3 default to the images used in the 2026-10 rehearsal; set them first to override.
set -q BASE; or set -gx BASE ghcr.io/midnight-ntwrk/midnight-node:2.1.0-rc.4
set -gx BABE_SCRIPTS (path resolve (path dirname (status filename)))
set -q V3; or set -gx V3 ghcr.io/midnight-ntwrk/midnight-node:3.0.0-47fd53bccbd1-arm64

function bstate --description "bstate [ws] | bstate [ws] digests N — wraps state.ts / digests.ts"
    if test "$argv[2]" = digests
        npx ts-node -T $BABE_SCRIPTS/digests.ts $argv[1] $argv[3] 2>/dev/null
    else
        npx ts-node -T $BABE_SCRIPTS/state.ts $argv[1] 2>/dev/null
    end
end

function babe --description "babe <script> [args] — run any check in this dir, e.g. babe forks ws://localhost:9945 3600"
    npx ts-node -T $BABE_SCRIPTS/$argv[1].ts $argv[2..] 2>/dev/null
end

function savelogs -a tag
    mkdir -p $HOME/babe-run/$tag
    for n in 1 2 3 4 5 6
        docker logs midnight-node-$n > $HOME/babe-run/$tag/node-$n.log 2>&1
    end
end

function roll -a n img
    # roll <n> [image]  — image defaults to $V3
    set -q img[1]; and test -n "$img"; or set img $V3
    # Same env the CLI gives compose: .env.default + generated secrets (shell/direnv vars win).
    docker logs midnight-node-$n > $HOME/babe-run/pre-roll-node-$n-(date +%H%M%S).log 2>&1
    env MIDNIGHT_NODE_IMAGE=$img \
        LOCALENV_POSTGRES_PASSWORD=(cat localenv_postgres.password) \
        APP__INFRA__STORAGE__PASSWORD=(cat localenv_app_storage.password) \
        APP__INFRA__PUB_SUB__PASSWORD=(cat localenv_pubsub.password) \
        APP__INFRA__SECRET=(cat localenv_app_infra_secret.password) \
        docker compose -p local-env --env-file .env.default \
        -f src/networks/local-env/docker-compose.yml up -d --no-deps midnight-node-$n
end
