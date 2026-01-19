FROM public.ecr.aws/amazonlinux/amazonlinux:2023-minimal@sha256:13bffb7de7ef4836742a6be2b09642e819aaec50ceed1d7961424e19a95da0de

ENV BASE_PATH=/node/chain
ENV RUST_BACKTRACE=1

# ntp to keep correct time
# curl to enable compose healthchecks
RUN microdnf -y update && \
    microdnf -y install ca-certificates curl-minimal procps-ng strace gdb vim jq tree gcc gcc-c++ make git cmake wget libtool autoconf automake && \
    # Build libfaketime with FORCE_MONOTONIC_FIX and FORCE_PTHREAD_NONVER
    git clone --depth 1 https://github.com/wolfcw/libfaketime.git && \
    CFLAGS="-DFORCE_MONOTONIC_FIX -DFORCE_PTHREAD_NONVER" make -C /libfaketime/src install && \
    rm -rf /libfaketime && \
    # Download bytehound
    wget -q https://github.com/koute/bytehound/releases/download/0.11.0/bytehound-x86_64-unknown-linux-gnu.tgz && \
    [ -f bytehound-x86_64-unknown-linux-gnu.tgz ] && \
    tar -xzvf bytehound-x86_64-unknown-linux-gnu.tgz && \
    mv libbytehound.so /usr/lib/libbytehound.so && \
    rm -rf bytehound* && \
    # Cleanup
    microdnf clean all && rm -rf /var/cache/dnf /var/cache/yum

COPY .envrc ./bin/.envrc
COPY res/ ./res/
COPY node/bin/entrypoint.sh /entrypoint.sh

RUN useradd -u 10001 -d /nonexistent -s /usr/sbin/nologin -M -U -c "" appuser
USER appuser

EXPOSE 30333 9933 9944 9615
ENTRYPOINT ["/entrypoint.sh"]
