FROM debian:bookworm-slim@sha256:78d2f66e0fec9e5a39fb2c72ea5e052b548df75602b5215ed01a17171529f706
# ntp to keep correct time
# curl to enable compose healthchecks
RUN apt-get update -qq \
    && apt-get upgrade -y -qq \
    && apt-get install -y -qq ca-certificates curl procps strace gdb vim jq tree \
    && rm -rf /var/lib/apt/lists/*

ENV RUNTIME_PATH=/midnight_node_runtime.compact.compressed.wasm

# Get node version for the image tag
COPY node/Cargo.toml /node/
RUN cat /node/Cargo.toml | grep -m 1 version | sed 's/version *= *"\([^\"]*\)".*/\1/' > node_version
RUN rm -rf /node
COPY util/upgrader/bin/entrypoint.sh /entrypoint.sh

RUN adduser --disabled-password --gecos "" --home "/nonexistent" --shell "/sbin/nologin" --no-create-home --uid "10001" appuser
USER appuser
ENTRYPOINT ["/entrypoint.sh"]
