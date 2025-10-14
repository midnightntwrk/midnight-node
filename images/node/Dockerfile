FROM debian:bookworm-slim

ENV BASE_PATH=/node/chain
ENV RUST_BACKTRACE=1

# ntp to keep correct time
# curl to enable compose healthchecks
RUN apt-get update -qq && \
    apt-get upgrade -y -qq && \
    apt-get install -y -qq --no-install-recommends ca-certificates curl procps strace \
    gdb vim jq tree build-essential git cmake wget libtool autoconf automake && \
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

COPY .envrc ./bin/.envrc
COPY res/ ./res/
COPY node/bin/entrypoint.sh /entrypoint.sh

RUN adduser --disabled-password --gecos "" --home "/nonexistent" --shell "/sbin/nologin" --no-create-home --uid "10001" appuser
USER appuser

EXPOSE 30333 9933 9944 9615
ENTRYPOINT ["/entrypoint.sh"]
