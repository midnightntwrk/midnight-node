ARG RUST_VERSION
FROM lukemathwalker/cargo-chef:0.1.77-rust-$RUST_VERSION-trixie AS chef
WORKDIR /build

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
SHELL ["/bin/bash", "-c"]
ARG PROFILE=release
COPY --from=planner /build/recipe.json recipe.json
RUN cargo chef cook --profile $PROFILE --recipe-path recipe.json
COPY . .
RUN cargo build -p indexer-standalone --locked --features standalone --profile $PROFILE && \
    mkdir -p /runtime/usr/local/bin && \
    mv "./target/${PROFILE/dev/debug}/indexer-standalone" /runtime/usr/local/bin && \
    mv /build/indexer-standalone/bin/entrypoint.sh /runtime/usr/local/bin && \
    mkdir -p /runtime/opt/indexer-standalone && \
    mv /build/indexer-standalone/config.yaml /runtime/opt/indexer-standalone

FROM debian:trixie-slim@sha256:f6e2cfac5cf956ea044b4bd75e6397b4372ad88fe00908045e9a0d21712ae3ba AS runtime
RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates=20250419 && \
    rm -rf /var/lib/apt/lists/*
RUN useradd -u 10001 -d /nonexistent -s /usr/sbin/nologin -M -c "" appuser && \
    passwd -l appuser && \
    mkdir /var/run/indexer-standalone && \
    chown appuser:appuser /var/run/indexer-standalone && \
    mkdir /data && \
    chown appuser:appuser /data
COPY --from=builder --chown=appuser:appuser /runtime /
USER appuser
WORKDIR /opt/indexer-standalone
ENTRYPOINT ["entrypoint.sh"]
EXPOSE 8088
