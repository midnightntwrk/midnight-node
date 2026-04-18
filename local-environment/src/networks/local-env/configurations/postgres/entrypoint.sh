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

set -e

# Generate a self-signed TLS certificate for PostgreSQL if one does not already exist.
# This allows the midnight-node to connect with PgSslMode::Require (encrypted, no cert validation)
# or PgSslMode::VerifyFull (with the CA cert mounted into the node container).
# NOTE: SSL files must live OUTSIDE PGDATA (/pgdata) because the official PostgreSQL
# docker-entrypoint.sh runs initdb, which requires PGDATA to be empty on first start.
SSL_DIR="/etc/ssl/postgres"

if [ ! -f "$SSL_DIR/server.key" ]; then
    mkdir -p "$SSL_DIR"
    openssl req -new -x509 -days 3650 -nodes \
        -text \
        -out "$SSL_DIR/server.crt" \
        -keyout "$SSL_DIR/server.key" \
        -subj "/CN=postgres" \
        -addext "subjectAltName=DNS:postgres,DNS:localhost,IP:127.0.0.1"
    # Make key readable by the postgres user (the entrypoint runs as root,
    # but docker-entrypoint.sh drops privileges to the postgres user).
    chown postgres:postgres "$SSL_DIR/server.key" "$SSL_DIR/server.crt"
    chmod 600 "$SSL_DIR/server.key"
    chmod 644 "$SSL_DIR/server.crt"
    echo "Generated self-signed TLS certificate for PostgreSQL in $SSL_DIR"
fi

chmod +x /docker-entrypoint-initdb.d/init.sh
exec docker-entrypoint.sh "$@"
