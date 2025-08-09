VERSION 0.8

# ================ Local Targets START ================
# If you add a new one here, prefix it with "local-"
# Add the target name to the doc string so it shows up
# in `$ earthly doc`

# local-build-node-release Build the node binary
local-build-node-release:
    LOCALLY
    RUN cargo build --release --package midnight-node

# ================ Local Targets END ================

# ================ ================ ================ ================
# ================ SEED GENERATION UTILS ================
# ================ ================ ================ ================

# A common target to generate genesis seeds.
generate-seeds:
    ARG NETWORK
    ARG OUTPUT_FILE
    FROM python:3.12
    RUN mkdir -p secrets
    COPY scripts/generate-genesis-seeds.py .
    # If a previous version of the file exists, bring it in.
    COPY --if-exists secrets/${OUTPUT_FILE} secrets/${OUTPUT_FILE}
    RUN python3 generate-genesis-seeds.py -c 4 -o secrets/${OUTPUT_FILE}
    SAVE ARTIFACT secrets/${OUTPUT_FILE} AS LOCAL secrets/${OUTPUT_FILE}

# Network-specific targets using the common seed generator:
generate-testnet-02-genesis-seeds:
    BUILD +generate-seeds --NETWORK=testnet-02 --OUTPUT_FILE=testnet-02-genesis-seeds.json


# generate-testnet-02-keys generates node keys and seeds and outputs a mock file + aws secret files
generate-testnet-02-keys:
    BUILD +generate-keys \
        --NETWORK=testnet-02 \
        --NUM_REGISTRATIONS=4 \
        --NUM_PERMISSIONED=12 \
        --D_REGISTERED=100 \
        --D_PERMISSIONED=1100 \
        --NUM_BOOT_NODES=3 \
        --NUM_VALIDATOR_NODES=12

# generate-qanet-keys generates node keys and seeds and outputs a mock file + aws secret files
generate-qanet-keys:
    BUILD +generate-keys \
        --DEV=true \
        --NETWORK=qanet \
        --NUM_REGISTRATIONS=4 \
        --NUM_PERMISSIONED=12 \
        --D_REGISTERED=100 \
        --D_PERMISSIONED=1100 \
        --NUM_BOOT_NODES=3 \
        --NUM_VALIDATOR_NODES=12

generate-keys:
    # D_PERMISSIONED + D_REGISTERED should be at least as large as slotsPerEpoch
    ARG DEV=false
    ARG NETWORK
    ARG NUM_REGISTRATIONS # Used for mock ariadne
    ARG NUM_PERMISSIONED
    ARG D_REGISTERED
    ARG D_PERMISSIONED
    ARG NUM_BOOT_NODES
    ARG NUM_VALIDATOR_NODES
    FROM earthly/dind:alpine-3.20-docker-26.1.5-r0
    RUN apk add --no-cache python3
    COPY scripts/generate-keys.py .
    COPY --if-exists secrets/$NETWORK-seeds-aws.json secrets/seeds-aws.json
    COPY --if-exists secrets/$NETWORK-keys-aws.json secrets/keys-aws.json
    COPY --if-exists res/$NETWORK/partner-chains-cli-chain-config.json partner-chains-cli-chain-config.json

    ENV SUBKEY_IMAGE=parity/subkey:3.0.0
    WITH DOCKER
        RUN docker pull $SUBKEY_IMAGE && \
            python3 generate-keys.py \
                -r $NUM_REGISTRATIONS \
                -p $NUM_PERMISSIONED \
                -dr $D_REGISTERED \
                -dp $D_PERMISSIONED \
                -b $NUM_BOOT_NODES \
                -v $NUM_VALIDATOR_NODES \
                $(if [ "$DEV" = "true" ]; then echo "--dev"; fi)
    END

    SAVE ARTIFACT artifacts/initial-authorities.json AS LOCAL res/$NETWORK/initial-authorities.json
    SAVE ARTIFACT artifacts/partner-chains-cli-chain-config.json AS LOCAL res/$NETWORK/partner-chains-cli-chain-config.json
    SAVE ARTIFACT artifacts/mock.json AS LOCAL res/mock-bridge-data/$NETWORK-mock.json
    SAVE ARTIFACT --if-exists secrets/seeds-aws.json AS LOCAL secrets/$NETWORK-seeds-aws.json
    SAVE ARTIFACT --if-exists secrets/keys-aws.json AS LOCAL secrets/$NETWORK-keys-aws.json

subxt:
    FROM rust:1.88-bookworm
    RUN rustup component add rustfmt
    # Install cargo binstall:
    # RUN curl -L --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash
    RUN cargo install cargo-binstall --version 1.6.9
    COPY Cargo.toml deps.toml
    LET SUBXT_VERSION = "$(cat deps.toml | grep -m 1 subxt | sed 's/subxt *= *"\([^\"]*\)".*/\1/')"
    RUN cargo binstall -y subxt-cli@$SUBXT_VERSION
    RUN cp /usr/local/cargo/bin/subxt /usr/local/bin/subxt
    ENTRYPOINT ["subxt"]
    SAVE IMAGE localhost/subxt

# Grabs metadata.scale file from the latest node
get-metadata:
    FROM +subxt
    WITH DOCKER --load ghcr.io/midnight-ntwrk/midnight-node:latest-halo2=+node-image
      RUN docker run --env CFG_PRESET=dev -p 9944:9944 ghcr.io/midnight-ntwrk/midnight-node:latest-halo2 & \
          sleep 5 && \
          subxt metadata -f bytes > /metadata.scale && \
          docker kill $(docker ps -q --filter ancestor=ghcr.io/midnight-ntwrk/midnight-node:latest-halo2)
    END
    SAVE ARTIFACT /metadata.scale

# Rebuild the metadata for Rust
rebuild-metadata-rust:
    FROM +subxt
    COPY +get-metadata/metadata.scale /metadata.scale
    RUN subxt codegen --file /metadata.scale > /subxt_metadata.rs
    COPY rustfmt.toml .
    RUN rustfmt /subxt_metadata.rs
    SAVE ARTIFACT /subxt_metadata.rs AS LOCAL res/src/subxt_metadata.rs

# Rebuild the metadata for nodejs
rebuild-metadata-nodejs:
    FROM node:22-bookworm
    COPY tests/ tests/
    COPY +get-metadata/metadata.scale tests/metadata.scale
    WORKDIR tests
    RUN corepack enable
    RUN yarn install
    RUN ./node_modules/.bin/papi --config polkadot-api.json
    SAVE ARTIFACT /tests/.papi/descriptors AS LOCAL tests/.papi/descriptors

# rebuild-metadata rebuilds the metadata for Rust and NodeJS e2e tests
rebuild-metadata:
    BUILD +rebuild-metadata-nodejs
    BUILD +rebuild-metadata-rust

# rebuild-sqlx rebuilds the subxt offline data for compile-time query checking
rebuild-sqlx:
    ARG USEROS
    FROM +prep
    CACHE --sharing shared --id cargo-git /usr/local/cargo/git
    CACHE --sharing shared --id cargo-reg /usr/local/cargo/registry
    CACHE /target
    RUN cargo install sqlx-cli --no-default-features --features rustls,postgres
    COPY localenv_postgres.password .
    RUN --mount type=secret,id=netrc,target=/root/.netrc \
        DATABASE_URL=postgres://postgres:$(cat localenv_postgres.password)@$([ "$USEROS" = "linux" ] && echo "172.17.0.1" || echo "host.docker.internal"):5432/cexplorer \
        cargo sqlx prepare --workspace
    SAVE ARTIFACT .sqlx AS LOCAL .sqlx

# rebuild-redemption-skeleton rebuilds the redemption skeleton contract using aiken
rebuild-redemption-skeleton:
    FROM node:22-bookworm
    RUN npm install -g @aiken-lang/aiken
    COPY tests/redemption-skeleton .
    RUN aiken build --trace-level verbose
    SAVE ARTIFACT plutus.json AS LOCAL tests/src/plutus.json

# Rebuild funding wallets
rebuild-funding-wallets:
    ARG NETWORK
    ARG SUFFIX=${NETWORK}
    ARG TOOLKIT_IMAGE=+toolkit-image
    FROM ${TOOLKIT_IMAGE}

    # Install jq
    RUN apt-get update -qq && apt-get install -y --no-install-recommends -qq jq

    RUN mkdir -p /res/genesis/
    RUN mkdir -p /secrets/

    RUN if [ "${NETWORK}" = "undeployed" ]; then \
            echo '[ \
                "0000000000000000000000000000000000000000000000000000000000000001", \
                "0000000000000000000000000000000000000000000000000000000000000002", \
                "0000000000000000000000000000000000000000000000000000000000000003", \
                "0000000000000000000000000000000000000000000000000000000000000004" \
            ]' > /secrets/undeployed-genesis-seeds.json; \
        fi

    # Wallet addresses generation
    COPY --if-exists secrets/${SUFFIX}-genesis-seeds.json /secrets/${SUFFIX}-genesis-seeds.json

    RUN if [ -f /secrets/${SUFFIX}-genesis-seeds.json ]; then \
            jq -r '.[]' /secrets/${SUFFIX}-genesis-seeds.json \
            | xargs -L1 -I{} /midnight-node-toolkit show-address \
                --network ${NETWORK} --seed {} --path "m/44'/2400'/0'/3/0" \
            > /res/genesis/genesis_funding_wallets_shielded_${SUFFIX}.txt; \
            \
            jq -r '.[]' /secrets/${SUFFIX}-genesis-seeds.json \
            | xargs -L1 -I{} /midnight-node-toolkit show-address \
                --network ${NETWORK} --seed {} --path "m/44'/2400'/0'/0/0" \
            > /res/genesis/genesis_funding_wallets_unshielded_${SUFFIX}.txt; \
    else \
        echo "No genesis seeds file found for ${SUFFIX}, skipping generation."; \
    fi

    SAVE ARTIFACT --if-exists /res/genesis/genesis_funding_wallets_* AS LOCAL res/genesis/

rebuild-genesis-state:
    ARG NETWORK
    ARG SUFFIX=${NETWORK}
    ARG GENERATE_TEST_TXS=true
    ARG RNG_SEED=0000000000000000000000000000000000000000000000000000000000000037
    ARG TOOLKIT_IMAGE=+toolkit-image
    FROM ${TOOLKIT_IMAGE}
    ENV RUST_BACKTRACE=1
    COPY res/genesis/genesis_funding_wallets_shielded_${SUFFIX}.txt funding_wallets_shielded.txt
    COPY res/genesis/genesis_funding_wallets_unshielded_${SUFFIX}.txt funding_wallets_unshielded.txt

    RUN mkdir -p /res/genesis
    RUN /midnight-node-toolkit generate-genesis \
        --network ${NETWORK} \
        --suffix ${SUFFIX} \
        --shielded-addresses $(cat funding_wallets_shielded.txt) \
        --unshielded-addresses $(cat funding_wallets_unshielded.txt)
    RUN cp out/genesis_*.mn /res/genesis/

    RUN mkdir -p /res/test-contract
    RUN mkdir -p out /res/test-contract \
        && if [ "$GENERATE_TEST_TXS" = "true" ]; then \
            /midnight-node-toolkit generate-txs \
                --src-files out/genesis_tx_${SUFFIX}.mn \
                --dest-file out/contract_tx_1_deploy_${SUFFIX}.mn \
                --to-bytes \
                contract-calls deploy \
                --rng-seed "$RNG_SEED" \
            && /midnight-node-toolkit contract-address \
                --network ${NETWORK} \
                --src-file out/contract_tx_1_deploy_${SUFFIX}.mn \
                --dest-file out/contract_address_${SUFFIX}.mn \
            && /midnight-node-toolkit generate-txs \
                --src-files out/genesis_tx_${SUFFIX}.mn out/contract_tx_1_deploy_${SUFFIX}.mn \
                --dest-file out/contract_tx_2_store_${SUFFIX}.mn \
                --to-bytes \
                contract-calls call \
                --call-key store \
                --rng-seed "$RNG_SEED" \
                --contract-address out/contract_address_${SUFFIX}.mn \
            && /midnight-node-toolkit generate-txs \
                --src-files out/genesis_tx_${SUFFIX}.mn out/contract_tx_1_deploy_${SUFFIX}.mn out/contract_tx_2_store_${SUFFIX}.mn \
                --dest-file out/contract_tx_3_check_${SUFFIX}.mn \
                --to-bytes \
                contract-calls call \
                --call-key check \
                --rng-seed "$RNG_SEED" \
                --contract-address out/contract_address_${SUFFIX}.mn \
            && /midnight-node-toolkit generate-txs \
                --src-files out/genesis_tx_${SUFFIX}.mn out/contract_tx_1_deploy_${SUFFIX}.mn out/contract_tx_2_store_${SUFFIX}.mn out/contract_tx_3_check_${SUFFIX}.mn \
                --dest-file out/contract_tx_4_change_authority_${SUFFIX}.mn \
                --to-bytes \
                contract-calls maintenance \
                --rng-seed "$RNG_SEED" \
                --contract-address out/contract_address_${SUFFIX}.mn \
            && cp out/contract*.mn /res/test-contract \
        ; fi

    RUN mkdir -p /res/test-zswap
    RUN mkdir -p out /res/test-zswap \
        && if [ "$GENERATE_TEST_TXS" = "true" ]; then \
            /midnight-node-toolkit generate-txs \
                --src-files out/genesis_tx_${SUFFIX}.mn \
                --dest-file out/zswap_undeployed.mn \
                --to-bytes batches \
                -n 1 \
                -b 1 \
                --rng-seed "$RNG_SEED" \
            && cp out/zswap_*.mn /res/test-zswap \
        ; fi

    SAVE ARTIFACT /res/genesis/* AS LOCAL res/genesis/
    SAVE ARTIFACT --if-exists /res/test-contract/* AS LOCAL res/test-contract/
    SAVE ARTIFACT --if-exists /res/test-zswap/* AS LOCAL res/test-zswap/

# rebuild-genesis-state-undeployed rebuilds the genesis ledger state for undeployed network - this MUST be followed by updating the chainspecs for CI to pass!
rebuild-genesis-state-undeployed:
    WAIT
        BUILD +rebuild-funding-wallets --NETWORK=undeployed
    END
    BUILD +rebuild-genesis-state \
        --NETWORK=undeployed

# rebuild-genesis-state-devnet rebuilds the genesis ledger state for devnet network - this MUST be followed by updating the chainspecs for CI to pass!
rebuild-genesis-state-devnet:
    WAIT
        BUILD +rebuild-funding-wallets --NETWORK=devnet --SUFFIX=devnet
    END
    BUILD +rebuild-genesis-state \
        --NETWORK=devnet \
        --GENERATE_TEST_TXS=false

# rebuild-genesis-state-qanet rebuilds the genesis ledger state for devnet network - this MUST be followed by updating the chainspecs for CI to pass!
rebuild-genesis-state-qanet:
    WAIT
        BUILD +rebuild-funding-wallets --NETWORK=devnet --SUFFIX=qanet
    END
    BUILD +rebuild-genesis-state \
        --NETWORK=devnet \
        --SUFFIX=qanet \
        --GENERATE_TEST_TXS=false

# rebuild-genesis-state-testnet-02 rebuilds the genesis ledger state for testnet network - this MUST be followed by updating the chainspecs for CI to pass!
rebuild-genesis-state-testnet-02:
    WAIT
        BUILD +rebuild-funding-wallets --NETWORK=testnet --SUFFIX=testnet-02
    END
    BUILD +rebuild-genesis-state \
        --NETWORK=testnet \
        --SUFFIX=testnet-02 \
        --GENERATE_TEST_TXS=false

# rebuild-all-genesis-states rebuilds the genesis ledger state for all networks - this MUST be followed by updating the chainspecs for CI to pass!
rebuild-all-genesis-states:
    BUILD +rebuild-genesis-state-undeployed
    BUILD +rebuild-genesis-state-devnet
    BUILD +rebuild-genesis-state-qanet
    BUILD +rebuild-genesis-state-testnet-02

# rebuild-chainspec for a given NETWORK
rebuild-chainspec:
    ARG NETWORK
    ARG NODE_IMAGE=+node-image
    FROM ${NODE_IMAGE}

    RUN CFG_PRESET=$NETWORK /midnight-node build-spec --disable-default-bootnode > res/$NETWORK/chain-spec.json

    # create abridge chain-spec that is diff tools and github friendly:
    RUN cat res/$NETWORK/chain-spec.json | \
      jq '.genesis.runtimeGenesis.code = "<snipped>" | .properties.genesis_tx = "<snipped>"' > res/$NETWORK/chain-spec-abridged.json

    RUN /midnight-node build-spec --chain=res/$NETWORK/chain-spec.json --raw --disable-default-bootnode > res/$NETWORK/chain-spec-raw.json

    SAVE ARTIFACT /res/$NETWORK/*.json AS LOCAL res/$NETWORK/

# rebuild-chainspecs Rebuild all chainspecs. No secrets required.
rebuild-chainspecs:
    BUILD +rebuild-chainspec --NETWORK=qanet
    BUILD +rebuild-chainspec --NETWORK=devnet
    BUILD +rebuild-chainspec --NETWORK=testnet-02

# rebuild-genesis Rebuild the initial ledger state genesis and chainspecs. Secrets required to rebuild prod/preprod geneses.
rebuild-genesis:
    LOCALLY
    WAIT
        BUILD +rebuild-all-genesis-states
    END
    BUILD +rebuild-chainspecs
    RUN echo "Rebuilt genesis and chainspecs"

# ci runs a quick aproximation of the ci targets
ci:
    BUILD +scan
    BUILD +audit
    BUILD +test

# Precompiled midnight contracts for use in testing and for the toolkit.
contract-precompile-image:
    # The results of this image is platform independent so we don't need to build for all platforms.
    BUILD +contract-precompile-image-single-platform

contract-precompile-image-single-platform:
    LET IMAGE_TAG="v0.22.0"
    FROM ghcr.io/midnight-ntwrk/compactc:$IMAGE_TAG
    COPY ledger/test-data/simple-merkle-tree.compact simple-merkle-tree.compact
    RUN /bin/ls /nix/store && /nix/store/z0w6z0q5vn0pkjsr1n8waiyklq049cc1-compactc/bin/compactc simple-merkle-tree.compact simple-merkle-tree
    # Keys should not have 0 size (but will have if we ran out of memory):
    RUN [ -s /simple-merkle-tree/keys/check.prover ]
    RUN [ -s /simple-merkle-tree/keys/check.verifier ]
    RUN [ -s /simple-merkle-tree/keys/store.prover ]
    RUN [ -s /simple-merkle-tree/keys/store.verifier ]

    ENV PATH=$PATH:/bin
    ENTRYPOINT [ "/bin/sh" ]

    ENV GHCR_REGISTRY=ghcr.io/midnight-ntwrk
    ENV IMAGE_TAG=$IMAGE_TAG
    LABEL org.opencontainers.image.source=https://github.com/midnight-ntwrk/artifacts
    LABEL org.opencontainers.image.title=node-test-contract-precompiles
    LABEL org.opencontainers.image.description="Midnight Test Contract Precompiles"
    SAVE IMAGE --push $GHCR_REGISTRY/midnight-test-contract-precompiles:$IMAGE_TAG

use-contract-precompile-image:
#    FROM +contract-precompile-image
    FROM ghcr.io/midnight-ntwrk/midnight-test-contract-precompiles:v0.22.0
    SAVE ARTIFACT /simple-merkle-tree AS LOCAL target/contracts/simple-merkle-tree

# a common setup of the build environment (not designed to be called directly)
node-ci-image:
    BUILD --platform=linux/arm64 +node-ci-image-single-platform
    BUILD --platform=linux/amd64 +node-ci-image-single-platform

node-ci-image-single-platform:
    ARG NATIVEARCH
    FROM rust:1.88

    # Install build dependencies
    RUN apt-get update -qq && \
        apt-get install -y --no-install-recommends -qq \
        build-essential \
        libssl-dev \
        libpq-dev \
        libsqlite3-dev \
        openssl \
        protobuf-compiler \
        pkg-config \
        grcov \
        openssh-client \
        gcc-aarch64-linux-gnu \
        libc6-dev-arm64-cross \
        gcc-x86-64-linux-gnu \
        crossbuild-essential-amd64 \
        libc6-amd64-cross

    RUN rustup target add wasm32v1-none aarch64-unknown-linux-gnu x86_64-unknown-linux-gnu
    RUN rustup component add rust-src rustfmt clippy llvm-tools-preview

    RUN git config --global url."https://github.com/".insteadOf "git@github.com:" \
      && mkdir .cargo \
      && touch .cargo/config.toml \
      && echo "[net]" >> .cargo/config.toml \
      && echo "git-fetch-with-cli = true" >> .cargo/config.toml

    # Install cargo binstall:
    # RUN curl -L --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash
    RUN cargo install cargo-binstall --version 1.6.9
    RUN cargo binstall --no-confirm cargo-nextest cargo-llvm-cov cargo-audit

    # subwasm can be used to diff between runtimes
    RUN cargo install --locked --git https://github.com/chevdor/subwasm --tag v0.21.3

    ENV CARGO_PROFILE_RELEASE_BUILD_OVERRIDE_DEBUG=true
    ENV CARGO_TERM_COLOR=always

    # SAVE IMAGE under the rust version used.
    # We rebuild the image weekly to apply security patches.
    ENV IMAGE_TAG="1.88"
    LABEL org.opencontainers.image.source=https://github.com/midnight-ntwrk/artifacts
    LABEL org.opencontainers.image.title=node-ci
    LABEL org.opencontainers.image.description="Midnight Node CI Image"
    SAVE IMAGE --push \
        ghcr.io/midnight-ntwrk/midnight-node-ci:$IMAGE_TAG-$NATIVEARCH

# a common setup of the build environment (not designed to be called directly)
prep:
    # FROM --platform=$NATIVEPLATFORM +node-ci-image-single-platform
    ARG NATIVEARCH
    FROM ghcr.io/midnight-ntwrk/midnight-node-ci:1.88-$NATIVEARCH

    RUN apt-get update -qq \
      && apt-get upgrade -y -qq \
      && apt-get install -y -qq clang \
      && rm -rf /var/lib/apt/lists/*
    RUN cargo --version

    COPY --keep-ts --dir \
        Cargo.lock Cargo.toml .config .sqlx docs \
        ledger node pallets primitives README.md res runtime \
        rustfmt.toml util .

    RUN rustup show
    # This doesn't seem to prevent the downloading at a later point, but
    # for now this is ok as there's only one compile task dependent on this.
    # RUN --mount type=secret,id=netrc,target=/root/.netrc cargo fetch --locked \
    #   --target aarch64-unknown-linux-gnu \
    #   --target x86_64-unknown-linux-gnu \
    #   --target wasm32v1-none
    SAVE IMAGE --cache-hint

# check-deps checks for unused dependencies
check-deps:
    FROM +prep
    RUN curl -L --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash
    RUN cargo binstall --no-confirm cargo-shear

    # shear
    RUN --mount type=secret,id=netrc,target=/root/.netrc cargo shear

# check-rust runs cargo fmt and clippy.
check-rust:
    FROM +prep
    CACHE --sharing shared --id cargo-git /usr/local/cargo/git
    CACHE --sharing shared --id cargo-reg /usr/local/cargo/registry
    CACHE /target
    RUN cargo fmt --all -- --check

    # clippy
    RUN --mount type=secret,id=netrc,target=/root/.netrc cargo clippy --workspace --all-targets -- -D warnings

# check-nodejs lints any nodejs projects
check-nodejs:
    FROM node:22-bookworm
    RUN corepack enable
    WORKDIR /tests
    COPY --dir tests/package.json tests/.yarnrc.yml tests/yarn.lock tests/.papi/ ./
    RUN yarn install --immutable
    COPY tests/ ./
    RUN yarn lint

# check lints/format checks for entire repo
check:
    BUILD +check-rust
    BUILD +check-nodejs

# test runs the tests in parallel with code coverage.
test:
    ARG NATIVEARCH
    FROM +prep
    CACHE --sharing shared --id cargo-git /usr/local/cargo/git
    CACHE --sharing shared --id cargo-reg /usr/local/cargo/registry
    CACHE /target

    # Test
    RUN mkdir /test-artifacts
    # Compile the tests to go as fast as possible on this machine:
    ENV RUSTFLAGS="-C target-cpu=native"
    COPY .envrc ./bin/.envrc
    COPY static/contracts/simple-merkle-tree /test-static/simple-merkle-tree
    ENV MIDNIGHT_LEDGER_TEST_STATIC_DIR=/test-static

    RUN --mount type=secret,id=netrc,target=/root/.netrc cargo llvm-cov nextest --profile ci --release --workspace --locked
    RUN cargo llvm-cov report --html --release --output-dir /test-artifacts-$NATIVEARCH/html
    RUN cargo llvm-cov report --lcov --release --fail-under-regions 14 --ignore-filename-regex res/src/subxt_metadata.rs --output-path /test-artifacts-$NATIVEARCH/tests.lcov

    # AS /target is a temp cache, copy the results to /test-artifacts, otherwise earthly won't find them later
    SAVE ARTIFACT ./test-artifacts-$NATIVEARCH AS LOCAL ./test-artifacts

# build creates production ready binaries
build:
    FROM +prep
    CACHE --sharing shared --id cargo-git /usr/local/cargo/git
    CACHE --sharing shared --id cargo-reg /usr/local/cargo/registry
    CACHE /target
    ARG EARTHLY_GIT_SHORT_HASH
    ARG NATIVEARCH
    ENV SUBSTRATE_CLI_GIT_COMMIT_HASH=$EARTHLY_GIT_SHORT_HASH
    ENV CARGO_PROFILE_RELEASE_BUILD_OVERRIDE_DEBUG=true

    # Should we need to cross compile again, these need to be set:
    # ENV CC_aarch64_unknown_linux_gnu=aarch64-linux-gnu-gcc
    # ENV CXX_aarch64_unknown_linux_gnu=aarch64-linux-gnu-g++
    # ENV CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc
    # ENV CC_x86_64_unknown_linux_gnu=x86_64-linux-gnu-gcc
    # ENV CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=x86_64-linux-gnu-gcc
    # ENV AR_X86_64_UNKNOWN_LINUX_GNU=ar
    # ENV CXX_X86_64_UNKNOWN_LINUX_GNU=x86_64-unknown-linux-gnu-g++=g++

    RUN mkdir -p /artifacts-$NATIVEARCH/ && mkdir -p /artifacts-$NATIVEARCH/test && mkdir -p /artifacts-$NATIVEARCH/rollback

    # Default build (no hardfork)
    RUN --mount type=secret,id=netrc,target=/root/.netrc \
        CARGO_TARGET_DIR=/target/default \
        cargo build --workspace --locked --release \
        && mv /target/default/release/midnight-node /artifacts-$NATIVEARCH \
        && mv /target/default/release/midnight-node-toolkit /artifacts-$NATIVEARCH \
        && mv /target/default/release/upgrader /artifacts-$NATIVEARCH \
        && cp -r /target/default/release/wbuild/midnight-node-runtime/ /artifacts-$NATIVEARCH

    # Hardfork build
    RUN CARGO_TARGET_DIR=/target/hardfork HARDFORK_TEST=1 \
        cargo build -p midnight-node-runtime --locked --release
    RUN mv /target/hardfork/release/wbuild/midnight-node-runtime/*.wasm \
        /artifacts-$NATIVEARCH/test

    # Rollback build
    RUN CARGO_TARGET_DIR=/target/rollback HARDFORK_TEST_ROLLBACK=1 \
        cargo build -p midnight-node-runtime --locked --release
    RUN mv /target/rollback/release/wbuild/midnight-node-runtime/midnight_node_runtime.compact.compressed.wasm \
        /artifacts-$NATIVEARCH/rollback/midnight_node_runtime_rollback.compact.compressed.wasm

    SAVE ARTIFACT /artifacts-$NATIVEARCH AS LOCAL artifacts

subwasm:
    ARG NATIVEARCH
    FROM +build
    # Saves testnet runtime as runtime_000.wasm
    RUN subwasm get wss://rpc.testnet.midnight.network/ \
        && subwasm diff ./runtime_000.wasm /artifacts-$NATIVEARCH/rollback/midnight_node_runtime_rollback.compact.compressed.wasm

base-image:
    FROM debian:bookworm-slim
    # ntp to keep correct time
    # curl to enable compose healthchecks
    RUN apt-get update -qq \
      && apt-get upgrade -y -qq \
      && apt-get install -y -qq ca-certificates curl procps strace gdb vim jq tree \
      && rm -rf /var/lib/apt/lists/*

# node-image creates the Midnight Substrate Node's image
node-image:
    ARG NATIVEARCH
    ARG EARTHLY_GIT_SHORT_HASH
    FROM +base-image

    COPY .envrc ./bin/.envrc
    COPY --dir res .

    ENV BASE_PATH=/node/chain
    ENV RUST_BACKTRACE=1
    COPY +build/artifacts-$NATIVEARCH/midnight-node /

    RUN apt-get update -qq && \
        apt-get install -y --no-install-recommends \
            build-essential git cmake wget \
            libtool autoconf automake \
            && \
        # Build libfaketime with FORCE_MONOTONIC_FIX and FORCE_PTHREAD_NONVER
        git clone https://github.com/wolfcw/libfaketime.git && \
        cd libfaketime/src && \
        make clean && \
        CFLAGS="-DFORCE_MONOTONIC_FIX -DFORCE_PTHREAD_NONVER" make && \
        make install && \
        cd ../.. && rm -rf libfaketime && \
        # Download bytehound
        wget -q https://github.com/koute/bytehound/releases/download/0.11.0/bytehound-x86_64-unknown-linux-gnu.tgz && \
        [ -f bytehound-x86_64-unknown-linux-gnu.tgz ] && \
        tar -xzvf bytehound-x86_64-unknown-linux-gnu.tgz && \
        mv libbytehound.so /usr/lib/libbytehound.so && \
        rm -rf bytehound* && \
        # Cleanup
        apt-get clean && rm -rf /var/lib/apt/lists/*

    RUN mkdir -p /artifacts-$NATIVEARCH
    COPY +build/artifacts-$NATIVEARCH/midnight-node-runtime/*.wasm /artifacts-$NATIVEARCH/

    EXPOSE 30333 9933 9944 9615
    ENTRYPOINT ["./midnight-node"]

    # TODO if git source version is picked up by substrate then we can just split by space and take second.
    RUN ./midnight-node --version | awk '{print $2}' | awk -F- '{print $1}' | head -1 > /version

    ENV GHCR_REGISTRY=ghcr.io/midnight-ntwrk
    ENV IMAGE_TAG="$(cat /version)-$EARTHLY_GIT_SHORT_HASH-$NATIVEARCH"
    ENV IMAGE_TAG_DEV="$(cat /version)-dev-$EARTHLY_GIT_SHORT_HASH-$NATIVEARCH"

    RUN echo image tag=midnight-node:$IMAGE_TAG | tee /artifacts-$NATIVEARCH/node_image_tag
    SAVE IMAGE --push \
        $GHCR_REGISTRY/midnight-node:latest-$NATIVEARCH \
        $GHCR_REGISTRY/midnight-node:$IMAGE_TAG \
        $GHCR_REGISTRY/midnight-node:$IMAGE_TAG_DEV

    # Re-export build artifacts which contain wasm
    COPY +build/artifacts-$NATIVEARCH /artifacts-$NATIVEARCH
    SAVE ARTIFACT /artifacts-$NATIVEARCH/* AS LOCAL artifacts-$NATIVEARCH/

# toolkit-image creates an image to run the midnight toolkit
toolkit-image:
    ARG NATIVEARCH
    ARG EARTHLY_GIT_SHORT_HASH
    FROM +base-image

    COPY .envrc ./bin/.envrc
    COPY static/contracts/simple-merkle-tree /test-static/simple-merkle-tree

    ENV MIDNIGHT_LEDGER_TEST_STATIC_DIR=/test-static

    COPY +build/artifacts-$NATIVEARCH/midnight-node-toolkit /
    ENTRYPOINT ["/midnight-node-toolkit"]

    # Get node version for the image tag
    COPY node/Cargo.toml /node/
    RUN cat /node/Cargo.toml | grep -m 1 version | sed 's/version *= *"\([^\"]*\)".*/\1/' > node_version
    RUN rm -rf /node
    LET NODE_VERSION = "$(cat node_version)"

    ENV GHCR_REGISTRY=ghcr.io/midnight-ntwrk
    ENV IMAGE_TAG="${NODE_VERSION}-${EARTHLY_GIT_SHORT_HASH}-${NATIVEARCH}"
    LABEL org.opencontainers.image.source=https://github.com/midnight-ntwrk/artifacts
    SAVE IMAGE --push \
        $GHCR_REGISTRY/midnight-node-toolkit:latest-$NATIVEARCH \
        $GHCR_REGISTRY/midnight-node-toolkit:$IMAGE_TAG

# hardfork-test-upgrader-image creates the hardfork test upgrader tool image
hardfork-test-upgrader-image:
    ARG NATIVEARCH
    ARG EARTHLY_GIT_SHORT_HASH
    FROM +base-image

    COPY +build/artifacts-$NATIVEARCH/upgrader /
    COPY +build/artifacts-$NATIVEARCH/test/* /
    COPY +build/artifacts-$NATIVEARCH/rollback/* /

    ENV RUNTIME_PATH=/midnight_node_runtime.compact.compressed.wasm
    ENTRYPOINT ["/upgrader"]

    # Get node version for the image tag
    COPY node/Cargo.toml /node/
    RUN cat /node/Cargo.toml | grep -m 1 version | sed 's/version *= *"\([^\"]*\)".*/\1/' > node_version
    RUN rm -rf /node
    LET NODE_VERSION = "$(cat node_version)"

    ENV GHCR_REGISTRY=ghcr.io/midnight-ntwrk
    ENV IMAGE_NAME=midnight-hardfork-test-upgrader
    ENV IMAGE_TAG="$NODE_VERSION-$EARTHLY_GIT_SHORT_HASH-$NATIVETARCH"

    RUN mkdir -p /artifacts-$NATIVEARCH
    RUN echo image tag=$IMAGE_NAME:$IMAGE_TAG | tee /artifacts-$NATIVEARCH/hardfork_test_upgrader_image_tag
    LABEL org.opencontainers.image.source=https://github.com/midnight-ntwrk/artifacts
    SAVE IMAGE --push \
        $GHCR_REGISTRY/$IMAGE_NAME:latest-$NATIVEARCH \
        $GHCR_REGISTRY/$IMAGE_NAME:$IMAGE_TAG

    SAVE ARTIFACT /artifacts-$NATIVEARCH/* AS LOCAL artifacts-$NATIVEARCH/

audit-including-ignores:
    FROM +prep
    # Run with no ignores so someone looking through the output can see the warnings
    RUN --no-cache cargo audit -c always || true

# audit checks for rust security vulnerabilities
audit:
    FROM +audit-including-ignores
    # No known fix yet:
    # RUSTSEC-2023-0071 rsa crate indirectly used by partner-chains-db-sync-data-sources
    #
    # Things that should be fixed by an upcoming sidechains/substrate upgrade:
    # RUSTSEC-2024-0336 rustls 0.20.9 used by libp2p. newer libp2p fixes this.
    #
    # Unmaintained crates (no known vulnerabilites):
    # RUSTSEC-2021-0139 ansi_term unmaintained: no fix available yet (Aug24).
    # RUSTSEC-2020-0168 mach unmaintained: longterm mitigation: switching wasm to risc.
    # RUSTSEC-2022-0061 parity-wasm unmaintained: longterm mitigation: switching wasm to risc.
    # RUSTSEC-2024-0320 yaml-rust crate used by config unmaintained.
    #
    # False positives:
    # RUSTSEC-2023-0071 rsa sidechannel. False positive: in a feature we don't use: `cargo tree | rg rsa` 0 hits.
    RUN --no-cache cargo audit -c always \
      --ignore RUSTSEC-2023-0071 \
      --ignore RUSTSEC-2024-0336 \
      --ignore RUSTSEC-2023-0071 \
      --ignore RUSTSEC-2022-0061 \
      --ignore RUSTSEC-2021-0139 \
      --ignore RUSTSEC-2020-0168 \
      --ignore RUSTSEC-2023-0033 \
      --ignore RUSTSEC-2024-0320
    RUN echo https://input-output.atlassian.net/browse/PM-10374 has been rised for fixing warning RUSTSEC-2023-0033

# partnerchains-dev contains tools for working with partner chains contracts on Cardano
partnerchains-dev:
    LET PARTNER_CHAINS_VERSION=1.5.0
    LET CARDANO_VERSION=10.1.4

    ARG EARTHLY_GIT_SHORT_HASH

    FROM ubuntu:24.04
    # Get node version for the image tag
    COPY node/Cargo.toml /node/
    RUN cat /node/Cargo.toml | grep -m 1 version | sed 's/version *= *"\([^\"]*\)".*/\1/' > node_version
    RUN rm -rf /node
    LET NODE_VERSION = "$(cat node_version)"
    LET IMAGE_TAG_SEMVER=$NODE_VERSION-$EARTHLY_GIT_SHORT_HASH
    # Install necessary packages
    RUN apt-get update -qq && apt-get install -y \
        curl \
        unzip \
        nodejs \
        bash \
        jq \
        socat \
        && rm -rf /var/lib/apt/lists/*

    # Download cardano node (for cardano-cli)
    RUN curl -L https://github.com/IntersectMBO/cardano-node/releases/download/${CARDANO_VERSION}/cardano-node-${CARDANO_VERSION}-linux.tar.gz -o cardano-node.tar.gz && \
        mkdir cardano-node && \
        tar -xzf cardano-node.tar.gz -C cardano-node --strip-components=1 && \
        mv cardano-node/bin/cardano-cli . && \
        rm -rf cardano-node cardano-node.tar.gz

    # Download partner chains node
    RUN curl -L https://github.com/input-output-hk/partner-chains/releases/download/v${PARTNER_CHAINS_VERSION}/partner-chains-node-v${PARTNER_CHAINS_VERSION}-x86_64-linux  -o partner-chains-node && \
        chmod +x partner-chains-node

    COPY +node-image/midnight-node /midnight-node
    COPY scripts/partnerchains-dev/* /

    ENV CARDANO_NODE_SOCKET_PATH=/node.socket
    ENV CARDANO_NODE_NETWORK_ID=2
    ENV AS_INIT=1
    ENV NODE_HOST=host.docker.internal

    ENTRYPOINT ["/bin/bash", "--init-file", "serve.sh"]
    LABEL org.opencontainers.image.source=https://github.com/midnight-ntwrk/artifacts
    LET IMAGE_TAG=${PARTNER_CHAINS_VERSION}-${CARDANO_VERSION}
    SAVE IMAGE --push ghcr.io/midnight-ntwrk/partnerchains-dev:$IMAGE_TAG_SEMVER ghcr.io/midnight-ntwrk/partnerchains-dev:$IMAGE_TAG ghcr.io/midnight-ntwrk/partnerchains-dev:latest

# run-node-mocked Run a local node against a mock ariadne bridge.
run-node-mocked:
    FROM +node-image
    ENV SIDECHAIN_BLOCK_BENEFICIARY="04bcf7ad3be7a5c790460be82a713af570f22e0f801f6659ab8e84a52be6969e"
    RUN CFG_PRESET=dev /midnight-node

# testnet-sync-e2e tries to sync the node with the first 7000 blocks of testnet
testnet-sync-e2e:
    LOCALLY
    ENV SYNC_UNTIL=7000
    # Explicitly load +node-image here to let earthly know that it's a dependency
    WITH DOCKER --load localhost/midnight-node:latest=+node-image
        RUN NODE_IMAGE=localhost/midnight-node:latest ./sync-with-testnet.sh
    END

# local-env-e2e executes any tests that depend on a running local-env
local-env-e2e:
    ARG USEROS
    FROM node:22-bookworm
    COPY tests/ tests/
    WORKDIR tests
    RUN corepack enable
    RUN yarn install --immutable
    RUN yarn run build
    WORKDIR /
    COPY local-environment/ local-environment/
    COPY scripts/cnight-generates-dust scripts/cnight-generates-dust
    WORKDIR tests
    RUN --no-cache HOST_ADDR=$([ "$USEROS" = "linux" ] && echo "172.17.0.1" || echo "host.docker.internal") \
        yarn run start

# compares chain parameters with testnet-02
chain-params-check:
    FROM alpine
    RUN apk add --no-cache curl jq

    COPY res/testnet-02/testnet-02.json ./

    RUN --no-cache \
        RPC_PAYLOAD='{ "jsonrpc": "2.0", "id": 1, "method": "sidechain_getParams", "params": [] }' && \
        RESPONSE=$(curl -X POST https://rpc.testnet-02.midnight.network:443 \
            -H "Content-Type: application/json" \
            -d "$RPC_PAYLOAD" | jq -r '.result') && \
        RES_FILE="$(cat testnet-02.json | jq -r '.genesis.runtimeGenesis.config.sidechain.params')" && \
        if [ "$RESPONSE" != "$RES_FILE" ]; then \
            echo "Chain params differ from testnet-02" && \
            echo "testnet-02: $RESPONSE" && \
            echo "current PR: $RES_FILE" && \
            exit 1; \
        fi

# compares addresses with testnet-02
addresses-check:
    FROM node:iron-alpine3.21
    RUN apk add --no-cache nodejs yarn
    COPY res/testnet-02/addresses.json /addresses.json
    COPY --dir scripts /
    WORKDIR /scripts/js
    RUN yarn install
    RUN ./src/checkTestnetAddresses.mjs

# start-local-env-latest starts up the local environment with the latest node image
start-local-env-latest:
    LOCALLY
    WITH DOCKER --load localhost/midnight-node:latest=+node-image
        # Ugly nested earthly call, but earthly complains if we use BUILD here
        RUN earthly +start-local-env --NODE_IMAGE=localhost/midnight-node:latest
    END

start-local-env:
    LOCALLY
    ARG NODE_IMAGE
    ARG USERARCH
    WORKDIR local-environment
    RUN npm ci
    RUN ARCHITECTURE=$USERARCH MIDNIGHT_NODE_IMAGE=$NODE_IMAGE npm run stop:local-env
    RUN ARCHITECTURE=$USERARCH MIDNIGHT_NODE_IMAGE=$NODE_IMAGE npm run run:local-env

stop-local-env:
    LOCALLY
    ARG NODE_IMAGE
    ARG USERARCH
    WORKDIR local-environment
    RUN npm ci
    RUN ARCHITECTURE=$USERARCH MIDNIGHT_NODE_IMAGE=$NODE_IMAGE npm run stop:local-env

# node-e2e-test runs the node E2E tests using Earthly's container management
# 
# Usage:
#   earthly +node-e2e-test
#
# This target:
# 1. Runs the Playwright E2E tests against the local environment
# 2. Saves all test artifacts to test-artifacts/e2e/node/
#
# Outputs:
#   - test-artifacts/e2e/node/ - All test results, logs, and reports
node-e2e-test:
    ARG NATIVEARCH
    
    LOCALLY
    
    RUN echo "🧪 Running Node E2E tests with Earthly:"
    
    # Setup test environment
    WORKDIR ui/tests
    
    # Install dependencies
    RUN yarn config set -H enableImmutableInstalls false
    RUN yarn install
    
    # Create test artifacts directory first
    RUN mkdir -p test-artifacts/e2e/node
    
    # Run the tests from the test artifacts directory to generate CTRF report there
    RUN echo "🎯 Running Playwright + Testcontainers tests..." \
      && NODE_PORT_WS=9933 DEBUG='testcontainers*' yarn test:node 2>&1 | tee reports/test-output.log || TEST_FAILED=true
    
    # Save test results
    RUN cp -r ./reports test-artifacts/e2e/node/ || true
    RUN cp -r ./logs test-artifacts/e2e/node/ || true
    # Check test results
    RUN if [ "${TEST_FAILED:-false}" = true ]; then \
        echo "❌ Tests failed"; \
        exit 1; \
    else \
        echo "✅ Node E2E tests complete."; \
    fi
