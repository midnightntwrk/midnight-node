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
    # renovate: datasource=docker packageName=python
    ARG PYTHON_VERSION=3.12
    FROM python:$PYTHON_VERSION
    RUN mkdir -p secrets
    COPY scripts/generate-genesis-seeds.py .
    # If a previous version of the file exists, bring it in.
    COPY --if-exists secrets/${OUTPUT_FILE} secrets/${OUTPUT_FILE}
    RUN python3 generate-genesis-seeds.py -c 4 -o secrets/${OUTPUT_FILE}
    SAVE ARTIFACT secrets/${OUTPUT_FILE} AS LOCAL secrets/${OUTPUT_FILE}



# generate-qanet-keys generates node keys and seeds and outputs a mock file + aws secret files
generate-qanet-keys:
    BUILD +generate-keys \
        --DEV=true \
        --NETWORK=qanet \
        --NUM_REGISTRATIONS=4 \
        --NUM_PERMISSIONED=12 \
        --D_REGISTERED=25 \
        --D_PERMISSIONED=275 \
        --NUM_BOOT_NODES=3 \
        --NUM_VALIDATOR_NODES=12

generate-preview-keys:
    BUILD +generate-keys \
        --DEV=true \
        --NETWORK=preview \
        --NUM_REGISTRATIONS=4 \
        --NUM_PERMISSIONED=12 \
        --D_REGISTERED=25 \
        --D_PERMISSIONED=275 \
        --NUM_BOOT_NODES=3 \
        --NUM_VALIDATOR_NODES=12

generate-preview-genesis-seeds:
    BUILD +generate-seeds --NETWORK=preview --OUTPUT_FILE=preview-genesis-seeds.json

generate-devnet-genesis-seeds:
    BUILD +generate-seeds --NETWORK=devnet --OUTPUT_FILE=devnet-genesis-seeds.json

generate-preprod-keys:
    BUILD +generate-keys \
        --DEV=true \
        --NETWORK=preprod \
        --NUM_REGISTRATIONS=4 \
        --NUM_PERMISSIONED=12 \
        --D_REGISTERED=25 \
        --D_PERMISSIONED=275 \
        --NUM_BOOT_NODES=3 \
        --NUM_VALIDATOR_NODES=12

generate-preprod-genesis-seeds:
    BUILD +generate-seeds --NETWORK=preprod --OUTPUT_FILE=preprod-genesis-seeds.json

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
    FROM public.ecr.aws/amazonlinux/amazonlinux:2023-minimal@sha256:13bffb7de7ef4836742a6be2b09642e819aaec50ceed1d7961424e19a95da0de

    # Install curl for rust installation
    RUN microdnf -y install curl-minimal ca-certificates gcc gcc-c++ make jq docker && \
        microdnf clean all && rm -rf /var/cache/dnf /var/cache/yum

    # Install rust with complete profile for profiler runtime support
    RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain 1.93 --profile complete
    ENV PATH="/root/.cargo/bin:${PATH}"

    RUN rustup component add rustfmt
    # Install cargo binstall:
    # RUN curl -L --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash
    # RUN cargo install cargo-binstall --version 1.6.9
    COPY Cargo.toml deps.toml
    LET SUBXT_VERSION = "$(cat deps.toml | grep -m 1 subxt | sed 's/subxt *= *"\([^\"]*\)".*/\1/')"
    RUN cargo install subxt-cli@${SUBXT_VERSION}
    RUN cp /root/.cargo/bin/subxt /usr/local/bin/subxt
    ENTRYPOINT ["subxt"]
    SAVE IMAGE localhost/subxt

# build-node-only builds only the midnight-node binary
build-node-only:
    FROM +build-prepare
    COPY --keep-ts --dir Cargo.lock Cargo.toml docs .sqlx \
    ledger node pallets primitives metadata res runtime util tests relay .

    ARG NATIVEARCH

    RUN cargo build -p midnight-node --locked --release

    RUN mkdir -p /artifacts-$NATIVEARCH \
        && mv /target/release/midnight-node /artifacts-$NATIVEARCH

    SAVE ARTIFACT /artifacts-$NATIVEARCH

# node-image-minimal creates a minimal node image for metadata extraction
node-image-minimal:
    ARG NATIVEARCH
    FROM DOCKERFILE -f ./images/node/Dockerfile .
    USER root

    RUN mkdir -p /node
    COPY +build-node-only/artifacts-$NATIVEARCH/midnight-node /

    RUN chown -R appuser:appuser /midnight-node /node ./bin ./res
    SAVE IMAGE localhost/node-minimal:latest

# Grabs metadata.scale file from the latest node
get-metadata:
    FROM +subxt
    DO github.com/EarthBuild/lib+INSTALL_DIND
    COPY local-environment/check-health.sh /usr/local/bin/check-health.sh
    WITH DOCKER --load localhost/node-minimal:latest=+node-image-minimal
      RUN docker run --env CFG_PRESET=dev -p 9944:9944 localhost/node-minimal:latest & \
          check-health.sh -t 30 -u http://localhost:9944 && \
          subxt metadata -f bytes > /metadata.scale && \
          docker kill $(docker ps -q --filter ancestor=localhost/node-minimal:latest)
    END
    SAVE ARTIFACT /metadata.scale

# rebuild-metadata gets the metadata file and adds it to the metadata crate
rebuild-metadata:
    FROM +subxt
    DO github.com/EarthBuild/lib+INSTALL_DIND
    COPY node/Cargo.toml /node/
    RUN cat /node/Cargo.toml | grep -m 1 version | sed 's/version *= *"\([^\"]*\)".*/\1/' > node_version
    LET NODE_VERSION = "$(cat node_version)"
    COPY +get-metadata/metadata.scale /metadata.scale
    SAVE ARTIFACT /metadata.scale AS LOCAL metadata/static/midnight_metadata.scale
    SAVE ARTIFACT /metadata.scale AS LOCAL metadata/static/midnight_metadata_${NODE_VERSION}.scale

# rebuild-sqlx rebuilds the subxt offline data for compile-time query checking
rebuild-sqlx:
    ARG USEROS
    FROM +prep
    CACHE --sharing shared --id cargo-git /usr/local/cargo/git
    CACHE --sharing shared --id cargo-reg /usr/local/cargo/registry
    CACHE /target
    COPY local-environment/localenv_postgres.password .
    RUN \
        DATABASE_URL=postgres://postgres:$(cat localenv_postgres.password)@$([ "$USEROS" = "linux" ] && echo "172.17.0.1" || echo "host.docker.internal"):5432/cexplorer \
        cargo sqlx prepare --workspace
    SAVE ARTIFACT .sqlx AS LOCAL .sqlx

# rebuild-redemption-skeleton rebuilds the redemption skeleton contract using aiken
rebuild-redemption-skeleton:
    # aiken doesn't support arm yet.
    FROM --platform=linux/amd64 public.ecr.aws/amazonlinux/amazonlinux:2023-minimal@sha256:13bffb7de7ef4836742a6be2b09642e819aaec50ceed1d7961424e19a95da0de

    # Install dependencies for Node.js (curl-minimal already in base image)
    RUN microdnf -y install tar gzip xz && \
        microdnf clean all && rm -rf /var/cache/dnf /var/cache/yum

    # Install Node.js 22 from official binaries (AL2023's nodejs is v18)
    ARG NODE_VERSION=22.13.1
    RUN curl -fsSL https://nodejs.org/dist/v${NODE_VERSION}/node-v${NODE_VERSION}-linux-x64.tar.xz -o node.tar.xz && \
        tar -xJf node.tar.xz -C /usr/local --strip-components=1 && \
        rm node.tar.xz && \
        node --version && npm --version

    # renovate: datasource=npm packageName=aiken-lang/aiken
    ENV aiken_version=1.1.19
    RUN npm install -g @aiken-lang/aiken@${aiken_version}
    COPY tests/redemption-skeleton .
    RUN aiken build --trace-level verbose
    SAVE ARTIFACT plutus.json AS LOCAL tests/src/plutus.json

rebuild-genesis-state:
    ARG NETWORK
    ARG GENERATE_TEST_TXS=true
    ARG RNG_SEED=0000000000000000000000000000000000000000000000000000000000000037
    ARG TOOLKIT_IMAGE=+toolkit-image
    FROM ${TOOLKIT_IMAGE}
    USER root
    ENV RUST_BACKTRACE=1
    # Skips genesis generation if you do not have the secrets for the environment you're building for (expected)
    COPY --if-exists secrets/${NETWORK}-genesis-seeds.json /secrets/genesis-seeds.json

    # Copy ledger parameters config (undeployed uses res/dev/)
    RUN mkdir -p /genesis-config
    IF [ "${NETWORK}" = "undeployed" ]
        COPY res/dev/ledger-parameters-config.json /genesis-config/ledger-parameters-config.json
    ELSE
        COPY res/${NETWORK}/ledger-parameters-config.json /genesis-config/ledger-parameters-config.json
    END

    # wallet-seed-3 is the wallet Lace uses for testing.
    # It is derived from the 24 word mnemonic: abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon diesel
    RUN if [ "${NETWORK}" = "undeployed" ]; then \
            mkdir -p /secrets/; \
            echo '{ \
                "wallet-seed-0": "0000000000000000000000000000000000000000000000000000000000000001", \
                "wallet-seed-1": "0000000000000000000000000000000000000000000000000000000000000002", \
                "wallet-seed-2": "0000000000000000000000000000000000000000000000000000000000000003", \
                "wallet-seed-3": "a51c86de32d0791f7cffc3bdff1abd9bb54987f0ed5effc30c936dddbb9afd9d530c8db445e4f2d3ea42a321b260e022aadf05987c9a67ec7b6b6ca1d0593ec9" \
            }' > /secrets/genesis-seeds.json; \
        fi

    RUN mkdir -p /res/genesis
    IF [ -f /secrets/genesis-seeds.json ]
        RUN /midnight-node-toolkit generate-genesis \
            --network ${NETWORK} \
            --seeds-file /secrets/genesis-seeds.json \
            --ledger-parameters-config /genesis-config/ledger-parameters-config.json
        RUN cp out/genesis_*.mn /res/genesis/
    ELSE
        RUN echo "No genesis seeds file found for ${NETWORK}, using existing genesis state"
        COPY res/genesis/genesis_state_${NETWORK}.mn res/genesis/genesis_block_${NETWORK}.mn /res/genesis
    END

    RUN mkdir -p /res/test-contract
    RUN mkdir -p out /res/test-contract \
        && if [ "$GENERATE_TEST_TXS" = "true" ]; then \
            /midnight-node-toolkit generate-txs \
                --src-file out/genesis_block_${NETWORK}.mn \
                --dust-warp \
                --dest-file out/contract_tx_1_deploy_${NETWORK}.mn \
                --to-bytes \
                contract-simple deploy \
                --rng-seed "$RNG_SEED" \
            && /midnight-node-toolkit contract-address \
                --src-file out/contract_tx_1_deploy_${NETWORK}.mn \
                | tr -d '\n' > out/contract_address_${NETWORK}.mn \
            && /midnight-node-toolkit generate-txs \
                --src-file out/genesis_block_${NETWORK}.mn \
                --src-file out/contract_tx_1_deploy_${NETWORK}.mn \
                --dust-warp \
                --dest-file out/contract_tx_2_store_${NETWORK}.mn \
                --to-bytes \
                contract-simple call \
                --call-key store \
                --rng-seed "$RNG_SEED" \
                --contract-address $(cat out/contract_address_${NETWORK}.mn) \
            && /midnight-node-toolkit generate-txs \
                --src-file out/genesis_block_${NETWORK}.mn \
                --src-file out/contract_tx_1_deploy_${NETWORK}.mn \
                --src-file out/contract_tx_2_store_${NETWORK}.mn \
                --dust-warp \
                --dest-file out/contract_tx_3_check_${NETWORK}.mn \
                --to-bytes \
                contract-simple call \
                --call-key check \
                --rng-seed "$RNG_SEED" \
                --contract-address $(cat out/contract_address_${NETWORK}.mn) \
            && /midnight-node-toolkit generate-txs \
                --src-file out/genesis_block_${NETWORK}.mn \
                --src-file out/contract_tx_1_deploy_${NETWORK}.mn \
                --src-file out/contract_tx_2_store_${NETWORK}.mn \
                --src-file out/contract_tx_3_check_${NETWORK}.mn \
                --dust-warp \
                --dest-file out/contract_tx_4_change_authority_${NETWORK}.mn \
                --to-bytes \
                contract-simple maintenance \
                --rng-seed "$RNG_SEED" \
                --contract-address $(cat out/contract_address_${NETWORK}.mn) \
                --new-authority-seed 1000000000000000000000000000000000000000000000000000000000000001 \
            && cp out/contract*.mn /res/test-contract \
        ; fi

    # Disabling zswap test data regeneration for now.
    # We need smart contracts to produce the test tokens it needs.
    RUN mkdir -p /res/test-zswap
    RUN mkdir -p out /res/test-zswap \
        && if [ "$GENERATE_TEST_TXS" = "true" ]; then \
            /midnight-node-toolkit generate-txs \
                --src-file out/genesis_block_${NETWORK}.mn \
                --dust-warp \
                --dest-file out/zswap_undeployed.mn \
                --to-bytes batches \
                -n 1 \
                -b 1 \
                --rng-seed "$RNG_SEED" \
            && cp out/zswap_*.mn /res/test-zswap \
        ; fi

    RUN mkdir -p /res/test-tx-deserialize
    RUN mkdir -p out /res/test-tx-deserialize \
        && if [ "$GENERATE_TEST_TXS" = "true" ]; then \
            /midnight-node-toolkit show-address \
                --network $NETWORK \
                --seed "0000000000000000000000000000000000000000000000000000000000000002" \
                --unshielded \
                > out/dest_addr.mn \
            && /midnight-node-toolkit generate-txs \
                --src-file out/genesis_block_${NETWORK}.mn \
                --dust-warp \
                --dest-file out/serialized_tx_with_context.mn \
                --to-bytes \
                single-tx \
                --unshielded-amount 500 \
                --rng-seed "$RNG_SEED" \
                --source-seed "0000000000000000000000000000000000000000000000000000000000000001" \
                --destination-address $(cat out/dest_addr.mn) \
            && /midnight-node-toolkit get-tx-from-context \
                --network $NETWORK \
                --src-file out/serialized_tx_with_context.mn \
                --dest-file out/serialized_tx_no_context.mn \
                --from-bytes \
            && cp out/serialized_* /res/test-tx-deserialize \
        ; fi

    RUN mkdir -p /res/test-data/contract/counter \
        && if [ "$GENERATE_TEST_TXS" = "true" ]; then \
            /midnight-node-toolkit generate-intent deploy \
                --coin-public $( \
                    /midnight-node-toolkit \
                    show-address \
                    --network $NETWORK \
                    --seed 0000000000000000000000000000000000000000000000000000000000000001 \
                    --coin-public \
                ) \
                -c /toolkit-js/test/contract/contract.config.ts \
                --output-intent /res/test-data/contract/counter/deploy.bin \
                --output-private-state /res/test-data/contract/counter/initial_state.json \
                --output-zswap-state /res/test-data/contract/counter/initial_zswap_state.json \
                0 \
            && /midnight-node-toolkit send-intent \
                --src-file /res/genesis/genesis_block_${NETWORK}.mn \
                --dust-warp \
                --intent-file /res/test-data/contract/counter/deploy.bin \
                --compiled-contract-dir /toolkit-js/test/contract/managed/counter \
                --rng-seed "$RNG_SEED" \
                --to-bytes \
                --dest-file /res/test-data/contract/counter/deploy_tx.mn \
            && /midnight-node-toolkit contract-address \
                --src-file /res/test-data/contract/counter/deploy_tx.mn \
                | tr -d '\n' > /res/test-data/contract/counter/contract_address.mn \
            && /midnight-node-toolkit contract-state \
                --src-file /res/genesis/genesis_block_${NETWORK}.mn \
                --src-file /res/test-data/contract/counter/deploy_tx.mn \
                --contract-address $(cat /res/test-data/contract/counter/contract_address.mn) \
                --dest-file /res/test-data/contract/counter/contract_state.mn \
        ; fi
    RUN mkdir -p /res/test-data/contract/mint \
        && if [ "$GENERATE_TEST_TXS" = "true" ]; then \
            /midnight-node-toolkit generate-intent deploy \
                --coin-public $( \
                    /midnight-node-toolkit \
                    show-address \
                    --network $NETWORK \
                    --seed 0000000000000000000000000000000000000000000000000000000000000001 \
                    --coin-public \
                ) \
                -c /toolkit-js/mint/mint.config.ts \
                --output-intent /res/test-data/contract/mint/deploy.bin \
                --output-private-state /res/test-data/contract/mint/initial_state.json \
                --output-zswap-state /res/test-data/contract/mint/initial_zswap_state.json \
            && /midnight-node-toolkit send-intent \
                --src-file /res/genesis/genesis_block_${NETWORK}.mn \
                --dust-warp \
                --intent-file /res/test-data/contract/mint/deploy.bin \
                --compiled-contract-dir /toolkit-js/mint/out \
                --rng-seed "$RNG_SEED" \
                --to-bytes \
                --dest-file /res/test-data/contract/mint/deploy_tx.mn \
            && /midnight-node-toolkit contract-address \
                --src-file /res/test-data/contract/mint/deploy_tx.mn \
                | tr -d '\n' > /res/test-data/contract/mint/contract_address.mn \
            && /midnight-node-toolkit contract-state \
                --src-file /res/genesis/genesis_block_${NETWORK}.mn \
                --src-file /res/test-data/contract/mint/deploy_tx.mn \
                --contract-address $(cat /res/test-data/contract/mint/contract_address.mn) \
                --dest-file /res/test-data/contract/mint/contract_state.mn \
        ; fi
    IF [ "$GENERATE_TEST_TXS" = "true" ]
        COPY +toolkit-js-prep/toolkit-js/test/contract/managed/counter/keys /res/test-data/contract/counter/keys
    END

    SAVE ARTIFACT /res/genesis/* AS LOCAL res/genesis/
    SAVE ARTIFACT --if-exists /res/test-contract/* AS LOCAL res/test-contract/
    SAVE ARTIFACT --if-exists /res/test-zswap/* AS LOCAL res/test-zswap/
    SAVE ARTIFACT --if-exists /res/test-tx-deserialize/* AS LOCAL res/test-tx-deserialize/
    SAVE ARTIFACT --if-exists /res/genesis/genesis_block_undeployed.mn AS LOCAL util/toolkit/test-data/genesis/
    SAVE ARTIFACT --if-exists /res/genesis/genesis_state_undeployed.mn AS LOCAL util/toolkit/test-data/genesis/
    SAVE ARTIFACT --if-exists /res/test-data/contract/counter/* AS LOCAL util/toolkit/test-data/contract/counter/
    SAVE ARTIFACT --if-exists /res/test-data/contract/mint/* AS LOCAL util/toolkit/test-data/contract/mint/

# rebuild-genesis-state-undeployed rebuilds the genesis ledger state for undeployed network - this MUST be followed by updating the chainspecs for CI to pass!
rebuild-genesis-state-undeployed:
    BUILD +rebuild-genesis-state \
        --NETWORK=undeployed

# rebuild-genesis-state-devnet rebuilds the genesis ledger state for devnet network - this MUST be followed by updating the chainspecs for CI to pass!
rebuild-genesis-state-devnet:
    BUILD +rebuild-genesis-state \
        --NETWORK=devnet \
        --GENERATE_TEST_TXS=false

# rebuild-genesis-state-devnet rebuilds the genesis ledger state for devnet network - this MUST be followed by updating the chainspecs for CI to pass!
rebuild-genesis-state-node-dev-01:
    BUILD +rebuild-genesis-state \
        --NETWORK=node-dev-01 \
        --GENERATE_TEST_TXS=false

# rebuild-genesis-state-qanet rebuilds the genesis ledger state for devnet network - this MUST be followed by updating the chainspecs for CI to pass!
rebuild-genesis-state-qanet:
    BUILD +rebuild-genesis-state \
        --NETWORK=qanet \
        --GENERATE_TEST_TXS=false

# rebuild-genesis-state-preview rebuilds the genesis ledger state for preview network - this MUST be followed by updating the chainspecs for CI to pass!
rebuild-genesis-state-preview:
    BUILD +rebuild-genesis-state \
        --NETWORK=preview \
        --GENERATE_TEST_TXS=false

# rebuild-genesis-state-preprod rebuilds the genesis ledger state for preprod network - this MUST be followed by updating the chainspecs for CI to pass!
rebuild-genesis-state-preprod:
    BUILD +rebuild-genesis-state \
        --NETWORK=preprod \
        --GENERATE_TEST_TXS=false

# rebuild-all-genesis-states rebuilds the genesis ledger state for all networks - this MUST be followed by updating the chainspecs for CI to pass!
rebuild-all-genesis-states:
    BUILD +rebuild-genesis-state-undeployed
    BUILD +rebuild-genesis-state-devnet
    BUILD +rebuild-genesis-state-node-dev-01
    BUILD +rebuild-genesis-state-qanet
    BUILD +rebuild-genesis-state-preview
    BUILD +rebuild-genesis-state-preprod

# rebuild-chainspec for a given NETWORK
rebuild-chainspec:
    ARG NETWORK
    ARG NODE_IMAGE=+node-image
    FROM ${NODE_IMAGE}
    USER root

    RUN CFG_PRESET=$NETWORK /midnight-node build-spec --disable-default-bootnode > res/$NETWORK/chain-spec.json

    # create abridge chain-spec that is diff tools and github friendly:
    RUN cat res/$NETWORK/chain-spec.json | \
      jq '.genesis.runtimeGenesis.code = "<snipped>" | .properties.genesis_extrinsics = "<snipped>" | .properties.genesis_state = "<snipped>"' > res/$NETWORK/chain-spec-abridged.json

    RUN /midnight-node build-spec --chain=res/$NETWORK/chain-spec.json --raw --disable-default-bootnode > res/$NETWORK/chain-spec-raw.json

    SAVE ARTIFACT /res/$NETWORK/*.json AS LOCAL res/$NETWORK/

# rebuild-all-chainspecs Rebuild all chainspecs. No secrets required.
rebuild-all-chainspecs:
    BUILD +rebuild-chainspec --NETWORK=node-dev-01
    BUILD +rebuild-chainspec --NETWORK=qanet
    BUILD +rebuild-chainspec --NETWORK=preview
    BUILD +rebuild-chainspec --NETWORK=preprod

# rebuild-genesis Rebuild the initial ledger state genesis and chainspecs. Secrets required to rebuild prod/preprod geneses.
rebuild-genesis:
    LOCALLY
    WAIT
        BUILD +rebuild-all-genesis-states
    END
    BUILD +rebuild-all-chainspecs
    RUN echo "Rebuilt genesis and chainspecs"

# ci runs a quick approximation of the ci targets
ci:
    BUILD +scan
    BUILD +audit
    BUILD +test

# Precompiled midnight contracts for use in testing and for the toolkit.
contract-precompile-image:
    # The results of this image is platform independent so we don't need to build for all platforms.
    BUILD +contract-precompile-image-single-platform

contract-precompile-image-single-platform:
    FROM public.ecr.aws/amazonlinux/amazonlinux:2023-minimal@sha256:13bffb7de7ef4836742a6be2b09642e819aaec50ceed1d7961424e19a95da0de
    # Install unzip and wget
    RUN microdnf -y install unzip wget tar gzip && \
        microdnf clean all && rm -rf /var/cache/dnf /var/cache/yum
    # Install gh CLI
    RUN wget -q https://github.com/cli/cli/releases/download/v2.62.0/gh_2.62.0_linux_amd64.tar.gz && \
        tar -xzf gh_2.62.0_linux_amd64.tar.gz && \
        mv gh_2.62.0_linux_amd64/bin/gh /usr/local/bin/ && \
        rm -rf gh_2.62.0_linux_amd64*

    # Fetch CompactC x86_64
    COPY COMPACTC_VERSION .
    RUN --secret GH_TOKEN set -e && \
        VERSION=$(cat COMPACTC_VERSION) && \
        RELEASE_TAG="compactc-v${VERSION}" && \
        echo "Attempting to download compactc from release: ${RELEASE_TAG}" && \
        if gh release download --repo midnight-ntwrk/artifacts "${RELEASE_TAG}" --pattern "*x86_64-unknown-linux-musl.zip" 2>/dev/null; then \
            echo "Successfully downloaded from release"; \
        elif gh api repos/midnight-ntwrk/artifacts/git/refs/tags/${RELEASE_TAG} >/dev/null 2>&1; then \
            echo "ERROR: Tag '${RELEASE_TAG}' exists but has no release with binary assets." && \
            echo "Available releases with binaries:" && \
            gh release list --repo midnight-ntwrk/artifacts --limit 5 && \
            exit 1; \
        else \
            echo "ERROR: No release or tag found for '${RELEASE_TAG}'" && \
            echo "Available releases:" && \
            gh release list --repo midnight-ntwrk/artifacts --limit 5 && \
            exit 1; \
        fi
    RUN unzip compactc*.zip

    COPY ledger/test-data/simple-merkle-tree.compact simple-merkle-tree.compact
    RUN ./compactc simple-merkle-tree.compact simple-merkle-tree
    # Keys should not have 0 size (but will have if we ran out of memory):
    RUN [ -s /simple-merkle-tree/keys/check.prover ]
    RUN [ -s /simple-merkle-tree/keys/check.verifier ]
    RUN [ -s /simple-merkle-tree/keys/store.prover ]
    RUN [ -s /simple-merkle-tree/keys/store.verifier ]

    ENV PATH=$PATH:/bin
    ENTRYPOINT [ "/bin/sh" ]

    ENV GHCR_REGISTRY=ghcr.io/midnight-ntwrk
    ARG IMAGE_TAG=$(cat COMPACTC_VERSION)
    ENV IMAGE_TAG=$IMAGE_TAG
    LABEL org.opencontainers.image.source=https://github.com/midnight-ntwrk/artifacts
    LABEL org.opencontainers.image.title=node-test-contract-precompiles
    LABEL org.opencontainers.image.description="Midnight Test Contract Precompiles"
    SAVE IMAGE --push $GHCR_REGISTRY/midnight-test-contract-precompiles:$IMAGE_TAG

use-contract-precompile-image:
    FROM public.ecr.aws/amazonlinux/amazonlinux:2023-minimal@sha256:13bffb7de7ef4836742a6be2b09642e819aaec50ceed1d7961424e19a95da0de
#    FROM +contract-precompile-image
    COPY COMPACTC_VERSION .
    ARG IMAGE_TAG=$(cat COMPACTC_VERSION)
    FROM ghcr.io/midnight-ntwrk/midnight-test-contract-precompiles:$IMAGE_TAG
    SAVE ARTIFACT /simple-merkle-tree AS LOCAL target/contracts/simple-merkle-tree

# a common setup of the build environment (not designed to be called directly)
node-ci-image:
    BUILD --platform=linux/arm64 +node-ci-image-single-platform
    BUILD --platform=linux/amd64 +node-ci-image-single-platform

node-ci-image-single-platform:
    ARG NATIVEARCH
    FROM public.ecr.aws/amazonlinux/amazonlinux:2023-minimal@sha256:13bffb7de7ef4836742a6be2b09642e819aaec50ceed1d7961424e19a95da0de

    # Install curl for rust installation
    RUN microdnf -y install curl-minimal ca-certificates && \
        microdnf clean all && rm -rf /var/cache/dnf /var/cache/yum

    # Install rust with complete profile for profiler runtime support (needed for cargo llvm-cov)
    RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain 1.93 --profile complete
    ENV PATH="/root/.cargo/bin:${PATH}"

    # Install build dependencies
    RUN microdnf -y update && \
        microdnf -y install \
        gcc \
        gcc-c++ \
        make \
        clang \
        openssl-devel \
        libpq-devel \
        sqlite-devel \
        openssl \
        protobuf-compiler \
        pkgconfig \
        openssh-clients \
        git \
        tar \
        gzip \
        jq && \
        microdnf clean all && rm -rf /var/cache/dnf /var/cache/yum
        # gcc-aarch64-linux-gnu \
        # libc6-dev-arm64-cross \
        # gcc-x86-64-linux-gnu \
        # crossbuild-essential-amd64 \
        # libc6-amd64-cross

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
    RUN cargo binstall --no-confirm cargo-nextest cargo-llvm-cov cargo-audit cargo-deny cargo-chef

    # subwasm can be used to diff between runtimes
    # renovate: datasource=github-releases packageName=chevdor/subwasm
    ARG SUBWASM_VERSION=0.21.3
    RUN cargo install --locked --git https://github.com/chevdor/subwasm --tag v$SUBWASM_VERSION
    RUN cargo install --locked cargo-shear --version 1.9.1
    RUN cargo install sqlx-cli --no-default-features --features rustls,postgres

    ENV CARGO_PROFILE_RELEASE_BUILD_OVERRIDE_DEBUG=true
    ENV CARGO_TERM_COLOR=always

    # SAVE IMAGE under the rust version used.
    # We rebuild the image weekly to apply security patches.
    ENV IMAGE_TAG="1.93"
    LABEL org.opencontainers.image.source=https://github.com/midnight-ntwrk/artifacts
    LABEL org.opencontainers.image.title=node-ci
    LABEL org.opencontainers.image.description="Midnight Node CI Image"
    SAVE IMAGE --push \
        ghcr.io/midnight-ntwrk/midnight-node-ci:$IMAGE_TAG-$NATIVEARCH

# a common setup of the build environment (not designed to be called directly)
prep-no-copy:
    ARG NATIVEARCH
    FROM --platform=$NATIVEPLATFORM +node-ci-image-single-platform
    # FROM midnightntwrk/midnight-node-ci:$RUST_VERSION-$NATIVEARCH

    # Used to add repository for nodejs
    RUN microdnf -y update && \
        microdnf -y install ca-certificates && \
        microdnf clean all && rm -rf /var/cache/dnf /var/cache/yum

    RUN cargo --version

prep:
    FROM +prep-no-copy
    COPY --keep-ts --dir \
        Cargo.lock Cargo.toml .cargo .config .sqlx deny.toml docs \
        ledger LICENSE node pallets primitives README.md res runtime \
        metadata rustfmt.toml util tests relay COMPACTC_VERSION .

    RUN rustup show
    # This doesn't seem to prevent the downloading at a later point, but
    # for now this is ok as there's only one compile task dependent on this.
    # RUN cargo fetch --locked \
    #   --target aarch64-unknown-linux-gnu \
    #   --target x86_64-unknown-linux-gnu \
    #   --target wasm32v1-none
    SAVE IMAGE --cache-hint

# Prepares Node Toolkit (JS) in time for testing
# Always uses linux/amd64 platform because compactc doesn't release for arm64
toolkit-js-prep:
    FROM --platform=linux/amd64 public.ecr.aws/amazonlinux/amazonlinux:2023-minimal@sha256:13bffb7de7ef4836742a6be2b09642e819aaec50ceed1d7961424e19a95da0de

    # Install dependencies for Node.js and toolkit-js (curl-minimal already in base image)
    RUN microdnf -y install tar gzip xz unzip && \
        microdnf clean all && rm -rf /var/cache/dnf /var/cache/yum

    # Install Node.js 22 x64 from official binaries (AL2023's nodejs is v18, which lacks File API needed by undici)
    # Always use x64 since this target is always built for linux/amd64 platform
    ARG NODE_VERSION=22.13.1
    RUN curl -fsSL https://nodejs.org/dist/v${NODE_VERSION}/node-v${NODE_VERSION}-linux-x64.tar.xz -o node.tar.xz && \
        tar -xJf node.tar.xz -C /usr/local --strip-components=1 && \
        rm node.tar.xz && \
        node --version && npm --version

    COPY COMPACTC_VERSION .
    COPY util/toolkit-js toolkit-js
    ARG COMPACTC_VERSION=$(cat COMPACTC_VERSION)
    ENV COMPACTC_VERSION=$COMPACTC_VERSION

    WORKDIR /toolkit-js
    RUN --secret GITHUB_TOKEN export GITHUB_TOKEN=$(cat /run/secrets/GITHUB_TOKEN) && npm ci
    RUN npm run build
    # Run npm compact script (includes fetch-compactc + compile steps)
    # fetch-compactc uses GITHUB_TOKEN to download compactc from artifacts
    RUN --secret GITHUB_TOKEN export GITHUB_TOKEN=$(cat /run/secrets/GITHUB_TOKEN) && npm run compact
    # Verify keys were generated
    RUN ls -la ./test/contract/managed/counter/keys/ && [ -s ./test/contract/managed/counter/keys/increment.verifier ]

    SAVE ARTIFACT /toolkit-js

# toolkit-js-prep-local saves Node Toolkit (JS) build artifacts
toolkit-js-prep-local:
    # We use `--platform=linux/amd64` here because compactc doesn't release for linux/arm64
    FROM --platform=linux/amd64 +toolkit-js-prep
    SAVE ARTIFACT /toolkit-js/node_modules AS LOCAL ./util/toolkit-js/node_modules
    SAVE ARTIFACT /toolkit-js/dist AS LOCAL ./util/toolkit-js/dist
    SAVE ARTIFACT /toolkit-js/test/contract/managed/counter AS LOCAL ./util/toolkit-js/test/contract/managed/counter
    SAVE ARTIFACT /toolkit-js/mint/out AS LOCAL ./util/toolkit-js/mint/out

# check-deps checks for unused dependencies
check-deps:
    FROM +prep
    RUN cargo install cargo-shear --version 1.6.6 --locked

    # shear
    RUN cargo shear

# check-rust runs cargo fmt and clippy.
planner:
    FROM +prep
    CACHE --sharing shared --id cargo-git /usr/local/cargo/git
    CACHE --sharing shared --id cargo-reg /usr/local/cargo/registry
    CACHE /target
    RUN cargo chef prepare --recipe-path recipe.json
    SAVE ARTIFACT recipe.json /recipe.json

check-rust-prepare:
    # NOTE: This just uses recipe.json - no src files!
    FROM +prep-no-copy
    # COPY +planner/recipe.json /recipe.json
    CACHE --sharing shared --id cargo-git /usr/local/cargo/git
    CACHE --sharing shared --id cargo-reg /usr/local/cargo/registry

    # Build dependencies - this is the caching Docker layer!
    # RUN SKIP_WASM_BUILD=1 cargo chef cook --clippy --workspace --all-targets  --features runtime-benchmarks --recipe-path /recipe.json

check-rust:
    FROM +check-rust-prepare
    CACHE --sharing shared --id cargo-git /usr/local/cargo/git
    CACHE --sharing shared --id cargo-reg /usr/local/cargo/registry
    COPY --keep-ts --dir \
        Cargo.lock Cargo.toml .config .sqlx deny.toml docs \
        ledger LICENSE node pallets primitives README.md res runtime \
    	metadata rustfmt.toml util tests relay COMPACTC_VERSION .

    RUN cargo fmt --all -- --check

    ENV SKIP_WASM_BUILD=1

    # ensure runtime benchmark feature enable to check they compile.
    RUN cargo clippy --workspace --all-targets --features runtime-benchmarks -- -D warnings

    # unfortunately we need to do cargo clean here to run within disk space limits
    RUN status=0; \
        for pkg in $(cargo metadata --no-deps --format-version 1 \
            | jq -r '.packages[].name'); do \
            echo "===> Checking $pkg"; \
            if ! cargo check -p "$pkg"; then \
            echo "Failed: $pkg"; \
            status=1; \
            fi; \
            cargo clean; \
        done; \
        exit $status

# check-metadata confirms that metadata in the repo matches a given node image
check-metadata:
    ARG NODE_IMAGE
    #=ghcr.io/midnight-ntwrk/midnight-node:latest
    FROM +subxt
    COPY local-environment/check-health.sh /usr/local/bin/check-health.sh

    WITH DOCKER --pull ${NODE_IMAGE}
      RUN docker run --env CFG_PRESET=dev -p 9944:9944 ${NODE_IMAGE} & \
          check-health.sh -t 30 -u http://localhost:9944 && \
          subxt metadata -f bytes > /image_metadata.scale && \
          docker kill $(docker ps -q --filter ancestor=${NODE_IMAGE})
    END
    COPY metadata/static/midnight_metadata.scale repo_metadata.scale
    RUN diff image_metadata.scale repo_metadata.scale

# check lints/format checks for entire repo
check:
    BUILD +check-rust

# test runs the tests in parallel with code coverage.
# Core tests - excludes Midnight Node Toolkit (requires Node Toolkit (JS) npm packages from midnight-js)
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

    # Run all tests EXCEPT:
    # - Midnight Node Toolkit (depends on Node Toolkit (JS) npm packages from midnight-js)
    # - pallet-midnight fixture tests (depend on .mn files that need regenerating with Midnight Node Toolkit)
    RUN MIDNIGHT_LEDGER_EXPERIMENTAL=1 cargo nextest r --profile ci --release --workspace --locked \
        --exclude midnight-node-toolkit \
        -E 'not (test(/^tests::test_get_contract_state$/) | test(/^tests::test_send_mn_transaction$/) | test(/^tests::test_validation_works$/))'

    # RUN MIDNIGHT_LEDGER_EXPERIMENTAL=1 cargo llvm-cov nextest --profile ci --release --workspace --locked \
    #     --exclude midnight-node-toolkit \
    #     -E 'not (test(/^tests::test_get_contract_state$/) | test(/^tests::test_send_mn_transaction$/) | test(/^tests::test_validation_works$/))'
    # RUN cargo llvm-cov report --html --release --output-dir /test-artifacts-$NATIVEARCH/html
    # RUN cargo llvm-cov report --lcov --release --fail-under-regions 14 --ignore-filename-regex res/src/subxt_metadata.rs --output-path /test-artifacts-$NATIVEARCH/tests.lcov

    # AS /target is a temp cache, copy the results to /test-artifacts, otherwise earthly won't find them later
    # SAVE ARTIFACT --if-exists ./test-artifacts-$NATIVEARCH AS LOCAL ./test-artifacts

# Pallet fixture tests - runs pallet-midnight tests that depend on regenerated .mn fixtures
# These tests do NOT require toolkit-js
test-pallet-fixtures:
    ARG NATIVEARCH
    FROM +prep
    CACHE --sharing shared --id cargo-git /usr/local/cargo/git
    CACHE --sharing shared --id cargo-reg /usr/local/cargo/registry
    CACHE /target

    # Compile the tests to go as fast as possible on this machine:
    ENV RUSTFLAGS="-C target-cpu=native"
    COPY .envrc ./bin/.envrc
    COPY static/contracts/simple-merkle-tree /test-static/simple-merkle-tree
    ENV MIDNIGHT_LEDGER_TEST_STATIC_DIR=/test-static

    # Run pallet-midnight fixture tests only llvm-cov
    RUN MIDNIGHT_LEDGER_EXPERIMENTAL=1 cargo nextest r --profile ci --release --locked \
        -E 'test(/^tests::test_get_contract_state$/) | test(/^tests::test_send_mn_transaction$/) | test(/^tests::test_validation_works$/)'
    # RUN cargo llvm-cov report --html --release --output-dir /test-artifacts-pallet-fixtures-$NATIVEARCH/html
    # RUN cargo llvm-cov report --lcov --release --output-path /test-artifacts-pallet-fixtures-$NATIVEARCH/tests.lcov

    # SAVE ARTIFACT ./test-artifacts-pallet-fixtures-$NATIVEARCH AS LOCAL ./test-artifacts-pallet-fixtures

# Midnight Node Toolkit tests - requires Node Toolkit (JS) which depends on midnight-js npm packages
# NOTE: This target builds for native platform, but copies toolkit-js from amd64 build (compactc is amd64-only)
build-test-toolkit:
    ARG NATIVEARCH
    ARG GITHUB_TOKEN
    FROM +prep
    CACHE --sharing shared --id cargo-git /usr/local/cargo/git
    CACHE --sharing shared --id cargo-reg /usr/local/cargo/registry
    CACHE /target

    # Install dependencies for Node.js (curl-minimal already in base image)
    RUN microdnf -y install tar gzip xz && \
        microdnf clean all && rm -rf /var/cache/dnf /var/cache/yum

    # Install Node.js 22 for native platform (AL2023's nodejs is v18, which lacks File API needed by undici)
    # Use native architecture since tests run on native platform, even though toolkit-js is from amd64
    ARG NODE_VERSION=22.13.1
    ARG TARGETARCH
    RUN if [ "$TARGETARCH" = "arm64" ]; then \
            NODE_ARCH="arm64"; \
        else \
            NODE_ARCH="x64"; \
        fi && \
        curl -fsSL https://nodejs.org/dist/v${NODE_VERSION}/node-v${NODE_VERSION}-linux-${NODE_ARCH}.tar.xz -o node.tar.xz && \
        tar -xJf node.tar.xz -C /usr/local --strip-components=1 && \
        rm node.tar.xz && \
        node --version && npm --version

    # Test
    RUN mkdir /test-artifacts-toolkit
    # Compile the tests to go as fast as possible on this machine:
    ENV RUSTFLAGS="-C target-cpu=native"
    COPY .envrc ./bin/.envrc
    COPY static/contracts/simple-merkle-tree /test-static/simple-merkle-tree
    ENV MIDNIGHT_LEDGER_TEST_STATIC_DIR=/test-static

    # Extract Node Toolkit (JS)
    # We use `--platform=linux/amd64` here because compactc doesn't release for linux/arm64
    COPY --platform=linux/amd64 +toolkit-js-prep/toolkit-js util/toolkit-js

    # Run Midnight Node Toolkit package tests only (requires toolkit-js)
    COPY scripts/test-toolkit.sh /test-toolkit.sh
    ENTRYPOINT ["/test-toolkit.sh"]
    SAVE IMAGE

test-toolkit:
    ARG NATIVEARCH
    FROM earthly/dind:alpine
    RUN mkdir -p /artifacts
    WITH DOCKER --load test-toolkit:latest=+build-test-toolkit
        # Use --network=host so testcontainers postgres is accessible via localhost
        RUN docker run \
            --network=host \
            -v /var/run/docker.sock:/var/run/docker.sock \
            -v /artifacts:/test-artifacts-toolkit-$NATIVEARCH \
            -e TESTCONTAINERS_HOST_OVERRIDE=localhost \
            test-toolkit:latest
    END
    SAVE ARTIFACT /artifacts AS LOCAL ./test-artifacts-toolkit

build-prepare:
    # NOTE: This just uses recipe.json - no src files!
    FROM +prep-no-copy
    # TODO: re-enable when chef is improved.
    # COPY +planner/recipe.json /recipe.json
    # CACHE --sharing shared --id cargo-git /usr/local/cargo/git
    # CACHE --sharing shared --id cargo-reg /usr/local/cargo/registry

    ARG EARTHLY_GIT_SHORT_HASH
    ENV SUBSTRATE_CLI_GIT_COMMIT_HASH=$EARTHLY_GIT_SHORT_HASH
    ENV CARGO_PROFILE_RELEASE_BUILD_OVERRIDE_DEBUG=true
    ENV CC=clang
    ENV CXX=clang++

    # Build dependencies - this is the caching Docker layer!
    # TODO: re-enable when chef is improved.
    # RUN SKIP_WASM_BUILD=1 cargo chef cook --release --workspace --all-targets --recipe-path /recipe.json

build-upgrader:
    FROM +prep
    ARG NATIVEARCH

    RUN mkdir -p /artifacts-$NATIVEARCH
    RUN SKIP_WASM_BUILD=1 cargo build -p upgrader --locked --release \
        && mv /target/release/upgrader /artifacts-$NATIVEARCH
    SAVE ARTIFACT /artifacts-$NATIVEARCH AS LOCAL artifacts

# build creates production ready binaries
build:
    ARG NATIVEARCH

    FROM +prep

    ARG EARTHLY_GIT_SHORT_HASH
    ENV SUBSTRATE_CLI_GIT_COMMIT_HASH=$EARTHLY_GIT_SHORT_HASH
    ENV CARGO_PROFILE_RELEASE_BUILD_OVERRIDE_DEBUG=true
    ENV CC=clang
    ENV CXX=clang++

    WAIT
        BUILD +build-upgrader
        BUILD +build-fork
        BUILD +build-undo
    END

    RUN mkdir -p /artifacts-$NATIVEARCH
    COPY +build-upgrader/artifacts-$NATIVEARCH /artifacts-$NATIVEARCH
    COPY +build-fork/artifacts-$NATIVEARCH /artifacts-$NATIVEARCH
    COPY +build-undo/artifacts-$NATIVEARCH /artifacts-$NATIVEARCH

    # Already saved as local
    SAVE ARTIFACT /artifacts-$NATIVEARCH

build-normal:
    FROM +build-prepare
    # CACHE --sharing shared --id cargo-git /usr/local/cargo/git
    # CACHE --sharing shared --id cargo-reg /usr/local/cargo/registry
    # CACHE /target
    COPY --keep-ts --dir Cargo.lock Cargo.toml docs .sqlx \
    ledger node pallets primitives metadata res runtime util tests relay COMPACTC_VERSION .

    ARG NATIVEARCH

    # Should we need to cross compile again, these need to be set:
    # ENV CC_aarch64_unknown_linux_gnu=aarch64-linux-gnu-gcc
    # ENV CXX_aarch64_unknown_linux_gnu=aarch64-linux-gnu-g++
    # ENV CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc
    # ENV CC_x86_64_unknown_linux_gnu=x86_64-linux-gnu-gcc
    # ENV CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=x86_64-linux-gnu-gcc
    # ENV AR_X86_64_UNKNOWN_LINUX_GNU=ar
    # ENV CXX_X86_64_UNKNOWN_LINUX_GNU=x86_64-unknown-linux-gnu-g++=g++

    # Default build (no hardfork)
    RUN \
        cargo build --workspace --locked --release

    RUN mkdir -p /artifacts-$NATIVEARCH/midnight-node-runtime/ \
        && mv /target/release/midnight-node /artifacts-$NATIVEARCH \
        && mv /target/release/midnight-node-toolkit /artifacts-$NATIVEARCH \
        && mv /target/release/upgrader /artifacts-$NATIVEARCH \
        && mv /target/release/aiken-deployer /artifacts-$NATIVEARCH \
        && cp /target/release/wbuild/midnight-node-runtime/*.wasm /artifacts-$NATIVEARCH/midnight-node-runtime/

    SAVE ARTIFACT /artifacts-$NATIVEARCH AS LOCAL artifacts

build-fork:
    FROM +prep
    # CACHE --sharing shared --id cargo-git /usr/local/cargo/git
    # CACHE --sharing shared --id cargo-reg /usr/local/cargo/registry
    # CACHE /target
    COPY --keep-ts --dir Cargo.lock Cargo.toml docs .sqlx \
    ledger node pallets primitives res metadata runtime util tests relay .

    ARG NATIVEARCH

    RUN mkdir -p /artifacts-$NATIVEARCH/test && mkdir -p /artifacts-$NATIVEARCH/rollback

    # Hardfork build
    RUN HARDFORK_TEST=1 cargo build -p midnight-node-runtime  --locked --release
    RUN mv /target/release/wbuild/midnight-node-runtime/*.wasm \
        /artifacts-$NATIVEARCH/test

    SAVE ARTIFACT /artifacts-$NATIVEARCH AS LOCAL artifacts

build-undo:
    FROM +prep
    ARG NATIVEARCH

    RUN mkdir -p /artifacts-$NATIVEARCH/test && mkdir -p /artifacts-$NATIVEARCH/rollback
    RUN rm -Rf /target/release/build/midnight-node-runtime-*
    # Rollback build
    RUN HARDFORK_TEST_ROLLBACK=1 cargo build --workspace --locked --release
    RUN mv /target/release/wbuild/midnight-node-runtime/midnight_node_runtime.compact.compressed.wasm \
        /artifacts-$NATIVEARCH/rollback/midnight_node_runtime_rollback.compact.compressed.wasm

    SAVE ARTIFACT /artifacts-$NATIVEARCH AS LOCAL artifacts

build-benchmarks:
    FROM +build-prepare
    COPY --keep-ts --dir Cargo.lock Cargo.toml docs .sqlx \
    ledger node pallets primitives metadata res runtime util tests .

    ARG NATIVEARCH

    # Build with runtime-benchmarks feature
    RUN \
        cargo build --workspace --locked --release --features runtime-benchmarks

    RUN mkdir -p /artifacts-$NATIVEARCH \
        && mv /target/release/midnight-node /artifacts-$NATIVEARCH/midnight-node-benchmarks

    SAVE ARTIFACT /artifacts-$NATIVEARCH AS LOCAL artifacts-benchmarks

subwasm:
    ARG NATIVEARCH
    FROM +build-normal
    # Saves testnet runtime as runtime_000.wasm
    RUN subwasm get wss://rpc.testnet.midnight.network/ \
        && subwasm diff ./runtime_000.wasm /artifacts-$NATIVEARCH/rollback/midnight_node_runtime_rollback.compact.compressed.wasm

# node-image creates the Midnight Substrate Node's image
node-image:
    ARG NATIVEARCH
    ARG EARTHLY_GIT_SHORT_HASH
    FROM DOCKERFILE -f ./images/node/Dockerfile .
    USER root

    RUN mkdir -p /artifacts-$NATIVEARCH
    RUN mkdir -p node

    COPY +build-normal/artifacts-$NATIVEARCH/midnight-node /
    COPY +build-normal/artifacts-$NATIVEARCH/aiken-deployer /
    COPY +build-normal/artifacts-$NATIVEARCH/midnight-node-runtime/*.wasm /artifacts-$NATIVEARCH/

    # Extract version from Cargo.toml to preserve semver pre-release suffix (e.g., 0.19.0-rc.1)
    COPY node/Cargo.toml /node/
    RUN cat /node/Cargo.toml | grep -m 1 version | sed 's/version *= *"\([^\"]*\)".*/\1/' > /version

    ENV GHCR_REGISTRY=ghcr.io/midnight-ntwrk
    ENV IMAGE_TAG="$(cat /version)-$EARTHLY_GIT_SHORT_HASH-$NATIVEARCH"
    ENV IMAGE_TAG_DEV="$(cat /version)-dev-$EARTHLY_GIT_SHORT_HASH-$NATIVEARCH"
    ENV NODE_DEV_01_TAG="$(cat /version)-$EARTHLY_GIT_SHORT_HASH-node-dev-01"

    RUN echo image tag=midnight-node:$IMAGE_TAG | tee /artifacts-$NATIVEARCH/node_image_tag
    RUN chown -R appuser:appuser /midnight-node /aiken-deployer /node ./bin ./res
    SAVE IMAGE --push \
        $GHCR_REGISTRY/midnight-node:latest-$NATIVEARCH \
        $GHCR_REGISTRY/midnight-node:$IMAGE_TAG \
        $GHCR_REGISTRY/midnight-node:$IMAGE_TAG_DEV \
        $GHCR_REGISTRY/midnight-node:$NODE_DEV_01_TAG

    # Re-export build artifacts which contain wasm
    COPY .envrc /artifacts-$NATIVEARCH/.envrc
    COPY res/ /artifacts-$NATIVEARCH/res/
    COPY +build-normal/artifacts-$NATIVEARCH /artifacts-$NATIVEARCH
    SAVE ARTIFACT /artifacts-$NATIVEARCH/* AS LOCAL artifacts-$NATIVEARCH/

# node-benchmarks-image creates the Midnight Substrate Node's image with runtime-benchmarks feature
node-benchmarks-image:
    ARG NATIVEARCH
    ARG EARTHLY_GIT_SHORT_HASH
    FROM DOCKERFILE -f ./images/node/Dockerfile .
    USER root

    RUN mkdir -p /artifacts-$NATIVEARCH

    COPY +build-benchmarks/artifacts-$NATIVEARCH/midnight-node-benchmarks /midnight-node

    # Extract version from Cargo.toml to preserve semver pre-release suffix (e.g., 0.19.0-rc.1)
    COPY node/Cargo.toml /node/
    RUN cat /node/Cargo.toml | grep -m 1 version | sed 's/version *= *"\([^\"]*\)".*/\1/' > /version

    ENV GHCR_REGISTRY=ghcr.io/midnight-ntwrk
    ENV IMAGE_TAG="$(cat /version)-$EARTHLY_GIT_SHORT_HASH-$NATIVEARCH"
    ENV NODE_DEV_01_TAG="$(cat /version)-$EARTHLY_GIT_SHORT_HASH-node-dev-01"

    RUN echo image tag=midnight-node-benchmarks:$IMAGE_TAG | tee /artifacts-$NATIVEARCH/node_benchmarks_image_tag
    LABEL org.opencontainers.image.source=https://github.com/midnight-ntwrk/artifacts
    LABEL org.opencontainers.image.title=midnight-node-benchmarks
    LABEL org.opencontainers.image.description="Midnight Node with Runtime Benchmarks"
    SAVE IMAGE --push \
        $GHCR_REGISTRY/midnight-node-benchmarks:latest-$NATIVEARCH \
        $GHCR_REGISTRY/midnight-node-benchmarks:$IMAGE_TAG \
        $GHCR_REGISTRY/midnight-node-benchmarks:$NODE_DEV_01_TAG

    SAVE ARTIFACT /artifacts-$NATIVEARCH/* AS LOCAL artifacts-benchmarks-$NATIVEARCH/

# toolkit-image creates an image to run the midnight toolkit
toolkit-image:
    ARG NATIVEARCH
    ARG EARTHLY_GIT_SHORT_HASH
    # Warning, seeing the same bug as recorded here: https://github.com/earthly/earthly/issues/932
    FROM DOCKERFILE --build-arg ARCH="$NATIVEARCH" -f ./images/toolkit/Dockerfile .
    USER root

    # Install dependencies for Node.js (curl-minimal already in base image)
    RUN microdnf -y install tar gzip xz && \
        microdnf clean all && rm -rf /var/cache/dnf /var/cache/yum

    # Install Node.js 22 from official binaries (AL2023's nodejs is v18, which lacks File API needed by undici)
    ARG NODE_VERSION=22.13.1
    RUN if [ "$NATIVEARCH" = "arm64" ]; then \
            NODE_ARCH="arm64"; \
        else \
            NODE_ARCH="x64"; \
        fi && \
        curl -fsSL https://nodejs.org/dist/v${NODE_VERSION}/node-v${NODE_VERSION}-linux-${NODE_ARCH}.tar.xz -o node.tar.xz && \
        tar -xJf node.tar.xz -C /usr/local --strip-components=1 && \
        rm node.tar.xz && \
        node --version && npm --version

    # Add toolkit-js
    # We use `--platform=linux/amd64` here because compactc doesn't release for linux/arm64
    COPY --platform=linux/amd64 +toolkit-js-prep/toolkit-js /toolkit-js

    COPY +build-normal/artifacts-$NATIVEARCH/midnight-node-toolkit /
    RUN mkdir -p /.cache/midnight/zk-params /.cache/sync

    LET NODE_VERSION="$(cat node_version)"
    ENV GHCR_REGISTRY=ghcr.io/midnight-ntwrk
    ENV IMAGE_TAG="${NODE_VERSION}-${EARTHLY_GIT_SHORT_HASH}-${NATIVEARCH}"
    ENV NODE_DEV_01_TAG="${NODE_VERSION}-${EARTHLY_GIT_SHORT_HASH}-node-dev-01"
    LABEL org.opencontainers.image.source=https://github.com/midnight-ntwrk/artifacts
    RUN chown -R appuser:appuser /midnight-node-toolkit /toolkit-js ./bin /.cache /test-static
    SAVE IMAGE --push \
        $GHCR_REGISTRY/midnight-node-toolkit:latest-$NATIVEARCH \
        $GHCR_REGISTRY/midnight-node-toolkit:$IMAGE_TAG \
        $GHCR_REGISTRY/midnight-node-toolkit:$NODE_DEV_01_TAG

# hardfork-test-upgrader-image creates the hardfork test upgrader tool image
hardfork-test-upgrader-image:
    ARG NATIVEARCH
    ARG EARTHLY_GIT_SHORT_HASH
    FROM DOCKERFILE -f ./images/hardfork-test-upgrader/Dockerfile .
    USER root

    COPY +build/artifacts-$NATIVEARCH/upgrader /
    COPY +build/artifacts-$NATIVEARCH/test/* /
    COPY +build/artifacts-$NATIVEARCH/rollback/* /

    COPY node/Cargo.toml /node/
    LET NODE_VERSION = "$(awk -F'\042' '/^version/ {print $2}' node/Cargo.toml)"

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

# audit-rust checks for rust security vulnerabilities
audit-rust:
    FROM +prep
    RUN mkdir -p /scan_reports
    # See deny.toml for which advisories are getting ignored
    RUN --no-cache cargo deny -f sarif check > /scan_reports/cargo-deny.sarif || true
    SAVE ARTIFACT scan_reports/cargo-deny.sarif AS LOCAL scan_reports/cargo-deny.sarif

audit-npm:
    ARG DIRECTORY
    ARG REPORT_NAME
    FROM public.ecr.aws/amazonlinux/amazonlinux:2023-minimal@sha256:13bffb7de7ef4836742a6be2b09642e819aaec50ceed1d7961424e19a95da0de

    # Install dependencies for Node.js (curl-minimal already in base image)
    RUN microdnf -y install tar gzip xz && \
        microdnf clean all && rm -rf /var/cache/dnf /var/cache/yum

    # Install Node.js 22 from official binaries (AL2023's nodejs is v18)
    ARG NODE_VERSION=22.13.1
    ARG TARGETARCH
    RUN if [ "$TARGETARCH" = "arm64" ]; then \
            NODE_ARCH="arm64"; \
        else \
            NODE_ARCH="x64"; \
        fi && \
        curl -fsSL https://nodejs.org/dist/v${NODE_VERSION}/node-v${NODE_VERSION}-linux-${NODE_ARCH}.tar.xz -o node.tar.xz && \
        tar -xJf node.tar.xz -C /usr/local --strip-components=1 && \
        rm node.tar.xz && \
        node --version && npm --version

    COPY ${DIRECTORY} ${DIRECTORY}
    WORKDIR ${DIRECTORY}
    RUN mkdir -p /scan_reports
    RUN --no-cache npm audit --audit-level high --json > npm-audit-${REPORT_NAME}.json \
      && npx npm-audit-sarif -o /scan_reports/npm-audit-${REPORT_NAME}.sarif npm-audit-${REPORT_NAME}.json
    SAVE ARTIFACT /scan_reports/npm-audit-${REPORT_NAME}.sarif AS LOCAL scan_reports/npm-audit-${REPORT_NAME}.sarif

audit-yarn:
    ARG DIRECTORY
    ARG REPORT_NAME
    FROM public.ecr.aws/amazonlinux/amazonlinux:2023-minimal@sha256:13bffb7de7ef4836742a6be2b09642e819aaec50ceed1d7961424e19a95da0de

    # Install dependencies for Node.js (curl-minimal already in base image)
    RUN microdnf -y install tar gzip xz && \
        microdnf clean all && rm -rf /var/cache/dnf /var/cache/yum

    # Install Node.js 22 from official binaries (AL2023's nodejs is v18)
    ARG NODE_VERSION=22.13.1
    ARG TARGETARCH
    RUN if [ "$TARGETARCH" = "arm64" ]; then \
            NODE_ARCH="arm64"; \
        else \
            NODE_ARCH="x64"; \
        fi && \
        curl -fsSL https://nodejs.org/dist/v${NODE_VERSION}/node-v${NODE_VERSION}-linux-${NODE_ARCH}.tar.xz -o node.tar.xz && \
        tar -xJf node.tar.xz -C /usr/local --strip-components=1 && \
        rm node.tar.xz && \
        node --version && npm --version

    # Install and enable corepack for yarn support
    RUN npm install -g corepack && corepack enable

    COPY metadata/static metadata/static
    COPY ${DIRECTORY} ${DIRECTORY}
    WORKDIR ${DIRECTORY}
    RUN yarn install --immutable
    RUN mkdir -p /scan_reports
    RUN --no-cache OUTPUT="$(yarn npm audit --severity high --json)" && echo "${OUTPUT:-{}}" > npm-audit-${REPORT_NAME}.json \
      && if [ -s "npm-audit-${REPORT_NAME}.json" ]; then npx npm-audit-sarif -o /scan_reports/npm-audit-${REPORT_NAME}.sarif npm-audit-${REPORT_NAME}.json; fi
    SAVE ARTIFACT /scan_reports/npm-audit-${REPORT_NAME}.sarif AS LOCAL scan_reports/npm-audit-${REPORT_NAME}.sarif

audit-local-environment:
    BUILD +audit-npm --DIRECTORY=local-environment/ --REPORT_NAME=local-environment

audit-toolkit-js:
    BUILD +audit-npm --DIRECTORY=util/toolkit-js/ --REPORT_NAME=toolkit-js

audit-ui:
    BUILD +audit-yarn --DIRECTORY=ui/ --REPORT_NAME=ui

audit-ui-tests:
    BUILD +audit-yarn --DIRECTORY=ui/tests/ --REPORT_NAME=ui-tests

# audit-nodejs checks for javascript security vulerabilities
audit-nodejs:
    BUILD +audit-local-environment
    BUILD +audit-toolkit-js
    BUILD +audit-ui
    BUILD +audit-ui-tests

# audit checks for security vulnerabilities
audit:
    BUILD +audit-rust
    BUILD +audit-nodejs

# partnerchains-dev contains tools for working with partner chains contracts on Cardano
partnerchains-dev:
    LET PARTNER_CHAINS_VERSION=1.5.0
    LET CARDANO_VERSION=10.1.4

    ARG EARTHLY_GIT_SHORT_HASH

    FROM public.ecr.aws/amazonlinux/amazonlinux:2023-minimal@sha256:13bffb7de7ef4836742a6be2b09642e819aaec50ceed1d7961424e19a95da0de
    # Get node version for the image tag
    COPY node/Cargo.toml /node/
    RUN cat /node/Cargo.toml | grep -m 1 version | sed 's/version *= *"\([^\"]*\)".*/\1/' > node_version
    RUN rm -rf /node
    LET NODE_VERSION = "$(cat node_version)"
    LET IMAGE_TAG_SEMVER=$NODE_VERSION-$EARTHLY_GIT_SHORT_HASH

    # Install Node.js repository
    RUN printf "%s\n" \
        "[nodesource]" \
        "name=Node.js Packages for Linux RPM based distros - \$basearch" \
        "baseurl=https://rpm.nodesource.com/pub_22.x/el/9/\$basearch" \
        "enabled=1" \
        "gpgcheck=1" \
        "gpgkey=https://rpm.nodesource.com/pub/el/NODESOURCE-GPG-SIGNING-KEY-EL" \
        > /etc/yum.repos.d/nodesource.repo

    # Install necessary packages
    RUN microdnf -y install \
        curl \
        unzip \
        nodejs \
        bash \
        jq \
        socat \
        && microdnf clean all && rm -rf /var/cache/dnf /var/cache/yum

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
    RUN CFG_PRESET=dev /entrypoint.sh

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
    FROM +prep
    COPY --keep-ts --dir Cargo.lock Cargo.toml docs .sqlx \
    ledger node pallets primitives metadata res runtime util tests local-environment scripts .
    WORKDIR tests/e2e
    RUN cargo test --test e2e_tests -- --test-threads=4 --nocapture

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
    ARG TARGETPLATFORM
    ARG USERARCH
    WORKDIR local-environment
    RUN npm ci
    RUN ARCHITECTURE=$USERARCH MIDNIGHT_NODE_IMAGE=$NODE_IMAGE npm run stop:local-env
    RUN ARCHITECTURE=$USERARCH MIDNIGHT_NODE_IMAGE=$NODE_IMAGE npm run run:local-env

start-local-env-with-indexer:
    LOCALLY
    ARG NODE_IMAGE
    ARG TARGETPLATFORM
    ARG USERARCH
    ARG INDEXER_API_IMAGE
    ARG CHAIN_INDEXER_IMAGE
    ARG WALLET_INDEXER_IMAGE
    WORKDIR local-environment
    RUN npm ci
    RUN ARCHITECTURE=$USERARCH MIDNIGHT_NODE_IMAGE=$NODE_IMAGE INDEXER_CHAIN_IMAGE=$CHAIN_INDEXER_IMAGE INDEXER_WALLET_IMAGE=$WALLET_INDEXER_IMAGE INDEXER_API_IMAGE=$INDEXER_API_IMAGE npm run stop:local-env -- -p withindexer
    RUN ARCHITECTURE=$USERARCH MIDNIGHT_NODE_IMAGE=$NODE_IMAGE INDEXER_CHAIN_IMAGE=$CHAIN_INDEXER_IMAGE INDEXER_WALLET_IMAGE=$WALLET_INDEXER_IMAGE INDEXER_API_IMAGE=$INDEXER_API_IMAGE npm run run:local-env-with-indexer -- -p withindexer

start-local-env-with-indexer-ci:
    LOCALLY
    ARG NODE_IMAGE
    ARG TARGETPLATFORM
    ARG USERARCH
    ARG INDEXER_API_IMAGE
    ARG CHAIN_INDEXER_IMAGE
    ARG WALLET_INDEXER_IMAGE
    WORKDIR local-environment
    RUN npm ci
    RUN ARCHITECTURE=$USERARCH MIDNIGHT_NODE_IMAGE=$NODE_IMAGE INDEXER_CHAIN_IMAGE=$CHAIN_INDEXER_IMAGE INDEXER_WALLET_IMAGE=$WALLET_INDEXER_IMAGE INDEXER_API_IMAGE=$INDEXER_API_IMAGE npm run run:local-env-with-indexer -- -p withindexer


stop-local-env:
    LOCALLY
    ARG USERARCH
    WORKDIR local-environment
    RUN npm ci
    RUN ARCHITECTURE=$USERARCH MIDNIGHT_NODE_IMAGE=any/any npm run stop:local-env

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

#images Build all the images
images:
    FROM scratch
    BUILD +node-image
    BUILD +toolkit-image
