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
RUN cargo build -p wallet-indexer --locked --features cloud --profile $PROFILE && \
    mkdir -p /runtime/usr/local/bin && \
    mv "./target/${PROFILE/dev/debug}/wallet-indexer" /runtime/usr/local/bin && \
    mv /build/wallet-indexer/bin/entrypoint.sh /runtime/usr/local/bin && \
    mkdir -p /runtime/opt/wallet-indexer && \
    mv /build/wallet-indexer/config.yaml /runtime/opt/wallet-indexer

FROM debian:trixie-slim@sha256:f6e2cfac5cf956ea044b4bd75e6397b4372ad88fe00908045e9a0d21712ae3ba AS runtime
RUN useradd -u 10001 -d /nonexistent -s /usr/sbin/nologin -M -c "" appuser && \
    passwd -l appuser && \
    mkdir /var/run/wallet-indexer && \
    chown appuser:appuser /var/run/wallet-indexer
COPY --from=builder --chown=appuser:appuser /runtime /
USER appuser
WORKDIR /opt/wallet-indexer
ENTRYPOINT ["entrypoint.sh"]
