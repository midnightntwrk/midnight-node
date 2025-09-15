set shell := ["bash", "-uc"]

# Can be overridden on the command line: `just feature=standalone`.
feature := "cloud"
packages := "indexer-common chain-indexer wallet-indexer indexer-api indexer-standalone indexer-tests"
rust_version := `grep channel rust-toolchain.toml | sed -r 's/channel = "(.*)"/\1/'`
nightly := "nightly-2025-08-07"
node_version := `cat NODE_VERSION`

check:
    for package in {{packages}}; do \
        cargo check -p "$package" --tests; \
        cargo check -p "$package" --tests --features {{feature}}; \
    done

license-headers:
    ./license_headers.sh

fmt:
    cargo +{{nightly}} fmt

fmt-check:
    cargo +{{nightly}} fmt --check

fix:
    cargo fix --allow-dirty --allow-staged --features {{feature}}

lint:
    for package in {{packages}}; do \
        cargo clippy -p "$package" --no-deps --tests                        -- -D warnings; \
        cargo clippy -p "$package" --no-deps --tests --features {{feature}} -- -D warnings; \
    done

lint-fix:
    for package in {{packages}}; do \
        cargo clippy -p "$package" --no-deps --tests --fix --allow-dirty --allow-staged                       ; \
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
    cargo run -p indexer-api --bin indexer-api-cli print-api-schema-v1 > \
        indexer-api/graphql/schema-v1.graphql.check
    @if ! cmp -s indexer-api/graphql/schema-v1.graphql indexer-api/graphql/schema-v1.graphql.check; then \
        echo "schema-v1.graphql has changes!"; exit 1; \
    fi

doc:
    RUSTDOCFLAGS="-D warnings --cfg docsrs" cargo +{{nightly}} doc -p indexer-common --no-deps --all-features
    RUSTDOCFLAGS="-D warnings --cfg docsrs" cargo +{{nightly}} doc -p chain-indexer  --no-deps --features {{feature}}
    RUSTDOCFLAGS="-D warnings --cfg docsrs" cargo +{{nightly}} doc -p wallet-indexer --no-deps --features {{feature}}
    RUSTDOCFLAGS="-D warnings --cfg docsrs" cargo +{{nightly}} doc -p indexer-api    --no-deps --features {{feature}}
    if [ "{{feature}}" = "standalone" ]; then \
        RUSTDOCFLAGS="-D warnings --cfg docsrs" cargo +{{nightly}} doc -p indexer-standalone --no-deps --features standalone; \
    fi

all: license-headers check fmt lint test doc

all-all:
    just feature=cloud all
    just feature=standalone all

coverage:
    ./coverage.sh {{nightly}}

generate-indexer-api-schema:
    cargo run -p indexer-api --bin indexer-api-cli print-api-schema-v1 > \
        indexer-api/graphql/schema-v1.graphql

build-docker-image package profile="dev":
    tag=$(git rev-parse --short=8 HEAD) && \
    docker build \
        --build-arg "RUST_VERSION={{rust_version}}" \
        --build-arg "PROFILE={{profile}}" \
        -t ghcr.io/midnight-ntwrk/{{package}}:${tag} \
        -t ghcr.io/midnight-ntwrk/{{package}}:latest \
        -f {{package}}/Dockerfile \
        .

run-chain-indexer node="ws://localhost:9944" network_id="Undeployed":
    docker compose up -d --wait postgres nats
    RUST_LOG=chain_indexer=debug,indexer_common=debug,fastrace_opentelemetry=off,tracing::span=off,midnight_ledger=warn,info \
        CONFIG_FILE=chain-indexer/config.yaml \
        APP__APPLICATION__NETWORK_ID={{network_id}} \
        APP__INFRA__NODE__URL={{node}} \
        cargo run -p chain-indexer --features {{feature}}

run-wallet-indexer network_id="Undeployed":
    docker compose up -d --wait postgres nats
    RUST_LOG=wallet_indexer=debug,indexer_common=debug,fastrace_opentelemetry=off,info \
        CONFIG_FILE=wallet-indexer/config.yaml \
        APP__APPLICATION__NETWORK_ID={{network_id}} \
        cargo run -p wallet-indexer --features {{feature}}

run-indexer-api network_id="Undeployed":
    docker compose up -d --wait postgres nats
    RUST_LOG=indexer_api=debug,indexer_common=debug,info \
        CONFIG_FILE=indexer-api/config.yaml \
        APP__APPLICATION__NETWORK_ID={{network_id}} \
        cargo run -p indexer-api --bin indexer-api --features {{feature}}

run-indexer-standalone node="ws://localhost:9944" network_id="Undeployed":
    mkdir -p target/data
    RUST_LOG=indexer=debug,chain_indexer=debug,wallet_indexer=debug,indexer_api=debug,indexer_common=debug,fastrace_opentelemetry=off,info \
        CONFIG_FILE=indexer-standalone/config.yaml \
        APP__APPLICATION__NETWORK_ID={{network_id}} \
        APP__INFRA__NODE__URL={{node}} \
        APP__INFRA__STORAGE__CNN_URL=target/data/indexer.sqlite \
        cargo run -p indexer-standalone --features standalone

generate-node-data:
    ./generate_node_data.sh {{node_version}}

generate-txs:
    ./generate_txs.sh {{node_version}}

get-node-metadata:
    ./get_node_metadata.sh {{node_version}}

update-node: generate-node-data get-node-metadata

run-node:
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
        ghcr.io/midnight-ntwrk/midnight-node:{{node_version}}
