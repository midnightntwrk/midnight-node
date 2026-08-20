set shell := ["bash", "-uc"]

# Can be overridden on the command line: `just feature=standalone`.
feature := "cloud"
packages := "indexer-common chain-indexer wallet-indexer indexer-api spo-indexer indexer-standalone indexer-tests"
rust_version := `grep channel rust-toolchain.toml | sed -r 's/channel = "(.*)"/\1/'`
latest_node_version := `tail -n 1 NODE_VERSIONS`

check:
    for package in {{packages}}; do \
        cargo check -p "$package" --tests --features {{feature}}; \
    done

license-headers:
    ./license_headers.sh

fmt:
    cargo fmt

fmt-check:
    cargo fmt --check

fix:
    cargo fix --allow-dirty --allow-staged --features {{feature}} --tests

lint:
    for package in {{packages}}; do \
        cargo clippy -p "$package" --no-deps --tests --features {{feature}} -- -D warnings; \
    done

lint-fix:
    for package in {{packages}}; do \
        cargo clippy -p "$package" --no-deps --tests --fix --allow-dirty --allow-staged --features {{feature}}; \
    done

test:
    # We must build the executables needed by the e2e tests!
    if [ "{{feature}}" = "cloud" ]; then \
        cargo build -p chain-indexer -p wallet-indexer -p indexer-api --features cloud; \
    fi
    if [ "{{feature}}" = "standalone" ]; then \
        cargo build -p indexer-standalone --features standalone; \
    fi
    cargo nextest run --workspace --exclude indexer-standalone --features {{feature}}
    # Check indexer-api schema:
    cargo run -p indexer-api --features {{feature}} --bin indexer-api-cli print-api-schema-v4 > \
        indexer-api/graphql/schema-v4.graphql.check
    @if ! cmp -s indexer-api/graphql/schema-v4.graphql indexer-api/graphql/schema-v4.graphql.check; then \
        echo "schema-v4.graphql has changes!"; exit 1; \
    fi

# `doc_cfg` (feature badges) is unstable; RUSTC_BOOTSTRAP=1 lets the pinned stable toolchain accept it.
doc:
    RUSTC_BOOTSTRAP=1 RUSTDOCFLAGS="-D warnings --cfg docsrs" cargo doc -p indexer-common --no-deps --features {{feature}}
    RUSTC_BOOTSTRAP=1 RUSTDOCFLAGS="-D warnings --cfg docsrs" cargo doc -p chain-indexer  --no-deps --features {{feature}}
    RUSTC_BOOTSTRAP=1 RUSTDOCFLAGS="-D warnings --cfg docsrs" cargo doc -p wallet-indexer --no-deps --features {{feature}}
    RUSTC_BOOTSTRAP=1 RUSTDOCFLAGS="-D warnings --cfg docsrs" cargo doc -p indexer-api    --no-deps --features {{feature}}
    if [ "{{feature}}" = "standalone" ]; then \
        RUSTC_BOOTSTRAP=1 RUSTDOCFLAGS="-D warnings --cfg docsrs" cargo doc -p indexer-standalone --no-deps --features standalone; \
    fi

bench:
    cargo bench -p indexer-common -p chain-indexer --features standalone

all: license-headers check fmt lint test doc

all-all:
    just feature=cloud all
    just feature=standalone all

coverage:
    ./coverage.sh

generate-indexer-api-schema:
    cargo run -p indexer-api --features {{feature}} --bin indexer-api-cli print-api-schema-v4 > \
        indexer-api/graphql/schema-v4.graphql

generate-spo-api-schema:
    cargo run -p spo-api --features cloud --bin spo-api-cli print-api-schema-v1 > \
        spo-api/graphql/schema-v1.graphql

build-docker-image package profile="dev":
    tag=$(git rev-parse --short=8 HEAD) && \
    docker build \
        --build-arg "RUST_VERSION={{rust_version}}" \
        --build-arg "PROFILE={{profile}}" \
        -t midnightntwrk/{{package}}:${tag} \
        -t midnightntwrk/{{package}}:latest \
        -f {{package}}/Dockerfile \
        .

run-chain-indexer node="ws://localhost:9944" network_id="undeployed":
    docker compose up -d --wait postgres nats
    RUST_LOG=chain_indexer=debug,indexer_common=debug,fastrace_opentelemetry=off,tracing::span=off,midnight_ledger=warn,midnight_zswap=warn,info \
        CONFIG_FILE=chain-indexer/config.yaml \
        APP__APPLICATION__NETWORK_ID={{network_id}} \
        APP__INFRA__NODE__URL={{node}} \
        cargo run -p chain-indexer --features {{feature}}

run-wallet-indexer network_id="undeployed":
    docker compose up -d --wait postgres nats
    RUST_LOG=wallet_indexer=debug,indexer_common=debug,fastrace_opentelemetry=off,info \
        CONFIG_FILE=wallet-indexer/config.yaml \
        APP__APPLICATION__NETWORK_ID={{network_id}} \
        cargo run -p wallet-indexer --features {{feature}}

run-indexer-api network_id="undeployed":
    docker compose up -d --wait postgres nats
    RUST_LOG=indexer_api=debug,indexer_common=debug,info \
        CONFIG_FILE=indexer-api/config.yaml \
        APP__APPLICATION__NETWORK_ID={{network_id}} \
        cargo run -p indexer-api --bin indexer-api --features {{feature}}

run-spo-indexer node="ws://localhost:9944" network_id="undeployed":
    docker compose up -d --wait postgres
    RUST_LOG=spo_indexer=debug,indexer_common=debug,fastrace_opentelemetry=off,info \
        CONFIG_FILE=spo-indexer/config.yaml \
        APP__APPLICATION__NETWORK_ID={{network_id}} \
        APP__INFRA__NODE__URL={{node}} \
        cargo run -p spo-indexer --features {{feature}}

run-indexer-standalone node="ws://localhost:9944" network_id="undeployed":
    mkdir -p target/data
    RUST_LOG=indexer=debug,chain_indexer=debug,wallet_indexer=debug,indexer_api=debug,indexer_common=debug,fastrace_opentelemetry=off,info \
        CONFIG_FILE=indexer-standalone/config.yaml \
        APP__APPLICATION__NETWORK_ID={{network_id}} \
        APP__INFRA__NODE__URL={{node}} \
        APP__INFRA__STORAGE__CNN_URL=target/data/indexer.sqlite \
        APP__INFRA__LEDGER_DB__CNN_URL=target/data/ledger-db.sqlite \
        cargo run -p indexer-standalone --features standalone

update-node: generate-node-data get-node-metadata

generate-node-data:
    ./generate_node_data.sh {{latest_node_version}}

get-node-metadata:
    ./get_node_metadata.sh {{latest_node_version}}

generate-txs:
    ./generate_txs.sh {{latest_node_version}}

run-node node_version=latest_node_version:
    #!/usr/bin/env bash
    node_dir=$(mktemp -d)
    cp -r ./.node/{{node_version}}/ $node_dir
    # SIDECHAIN_BLOCK_BENEFICIARY specifies the wallet that receives block rewards and transaction fees (DUST).
    # This hex value is a public key that matches the one used in toolkit-e2e.sh.
    docker run \
        --name node \
        -p 9944:9944 \
        -e SHOW_CONFIG=false \
        -e CFG_PRESET=dev \
        -e SIDECHAIN_BLOCK_BENEFICIARY="04bcf7ad3be7a5c790460be82a713af570f22e0f801f6659ab8e84a52be6969e" \
        -v $node_dir:/node \
        midnightntwrk/midnight-node:{{node_version}}
