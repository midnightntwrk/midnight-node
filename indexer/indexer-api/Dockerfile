ARG RUST_VERSION
FROM lukemathwalker/cargo-chef:0.1.72-rust-$RUST_VERSION-bookworm AS chef
WORKDIR /build

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
SHELL ["/bin/bash", "-c"]
ARG PROFILE=release
RUN git config --global url."https://@github.com".insteadOf "ssh://git@github.com"
COPY --from=planner /build/recipe.json recipe.json
RUN cargo chef cook --profile $PROFILE --recipe-path recipe.json
COPY . .
RUN cargo build -p indexer-api --locked --features cloud --profile $PROFILE && \
    mkdir -p /runtime/usr/local/bin && \
    mv "./target/${PROFILE/dev/debug}/indexer-api" /runtime/usr/local/bin && \
    mv /build/indexer-api/bin/entrypoint.sh /runtime/usr/local/bin && \
    mkdir -p /runtime/opt/indexer-api && \
    mv /build/indexer-api/config.yaml /runtime/opt/indexer-api

FROM debian:bookworm-slim@sha256:b1a741487078b369e78119849663d7f1a5341ef2768798f7b7406c4240f86aef AS runtime
RUN adduser --disabled-password --gecos "" --home "/nonexistent" --shell "/sbin/nologin" --no-create-home --uid "10001" appuser && \
    mkdir /var/run/indexer-api && \
    chown appuser:appuser /var/run/indexer-api
COPY --from=builder --chown=appuser:appuser /runtime /
USER appuser
WORKDIR /opt/indexer-api
ENTRYPOINT ["entrypoint.sh"]
EXPOSE 8088
