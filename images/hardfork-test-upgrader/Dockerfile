FROM debian:bookworm-slim
# ntp to keep correct time
# curl to enable compose healthchecks
RUN apt-get update -qq \
    && apt-get upgrade -y -qq \
    && apt-get install -y -qq ca-certificates curl procps strace gdb vim jq tree \
    && rm -rf /var/lib/apt/lists/*

ENV RUNTIME_PATH=/midnight_node_runtime.compact.compressed.wasm

RUN mkdir -p /app

# Get node version for the image tag
COPY node/Cargo.toml /node/
RUN cat /node/Cargo.toml | grep -m 1 version | sed 's/version *= *"\([^\"]*\)".*/\1/' > /app/node_version
RUN rm -rf /node

ENTRYPOINT ["/app/upgrader"]
