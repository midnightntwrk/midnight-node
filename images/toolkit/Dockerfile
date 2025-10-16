FROM debian:bookworm-slim
# ntp to keep correct time
# curl to enable compose healthchecks
RUN apt-get update -qq \
    && apt-get upgrade -y -qq \
    && apt-get install -y -qq ca-certificates curl gnupg procps strace gdb vim jq tree \
    && rm -rf /var/lib/apt/lists/*

# Add NodeSource repository with GPG verification
RUN mkdir -p /usr/share/keyrings && \
    curl -fsSL https://deb.nodesource.com/gpgkey/nodesource-repo.gpg.key | gpg --dearmor --yes -o /usr/share/keyrings/nodesource.gpg

COPY .envrc ./bin/.envrc
COPY static/contracts/simple-merkle-tree /test-static/simple-merkle-tree

ENV TOOLKIT_JS_PATH="/toolkit-js"
ENV MIDNIGHT_LEDGER_TEST_STATIC_DIR=/test-static
ENV MIDNIGHT_PP=/.cache/midnight/zk-params
ENV MN_SYNC_CACHE=/.cache/sync

# Get node version for the image tag
COPY node/Cargo.toml /node/
RUN cat /node/Cargo.toml | grep -m 1 version | sed 's/version *= *"\([^\"]*\)".*/\1/' > node_version
RUN rm -rf /node
COPY util/toolkit/bin/entrypoint.sh /entrypoint.sh

RUN adduser --disabled-password --gecos "" --home "/nonexistent" --shell "/sbin/nologin" --no-create-home --uid "10001" appuser
USER appuser
ENTRYPOINT ["/entrypoint.sh"]
