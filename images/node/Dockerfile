# Stage 1: Build libfaketime (keeps gcc out of the runtime image)
FROM public.ecr.aws/amazonlinux/amazonlinux:2023-minimal@sha256:13bffb7de7ef4836742a6be2b09642e819aaec50ceed1d7961424e19a95da0de AS libfaketime-builder
# renovate: datasource=github-releases depName=wolfcw/libfaketime
ARG LIBFAKETIME_VERSION=v0.9.10
RUN microdnf -y install gcc-11.5.0 make-4.3 git-2.50.1 && \
    git clone --depth 1 --branch ${LIBFAKETIME_VERSION} https://github.com/wolfcw/libfaketime.git && \
    CFLAGS="-DFORCE_MONOTONIC_FIX -DFORCE_PTHREAD_NONVER" make -C /libfaketime/src install

# Stage 2: Runtime image
FROM public.ecr.aws/amazonlinux/amazonlinux:2023-minimal@sha256:13bffb7de7ef4836742a6be2b09642e819aaec50ceed1d7961424e19a95da0de

ENV BASE_PATH=/node/chain
ENV RUST_BACKTRACE=1

# Runtime dependencies only — no build toolchain
# curl to enable compose healthchecks
# Package versions are tied to the base image — update when bumping the base digest
RUN microdnf -y install \
    procps-ng-3.3.17 \
    strace-6.12 \
    gdb-16.3 \
    vim-enhanced-9.1.1591 \
    jq-1.7.1 \
    tree-1.8.0 && \
    # Download bytehound profiler (x86_64 only, harmless no-op on arm64)
    curl -fsSL -o /tmp/bytehound.tgz https://github.com/koute/bytehound/releases/download/0.11.0/bytehound-x86_64-unknown-linux-gnu.tgz && \
    tar -xzf /tmp/bytehound.tgz -C /tmp && \
    mv /tmp/libbytehound.so /usr/lib/libbytehound.so && \
    rm -f /tmp/bytehound.tgz && \
    microdnf clean all && rm -rf /var/cache/dnf /var/cache/yum

# Copy pre-built libfaketime from builder stage
COPY --from=libfaketime-builder /usr/local/lib/faketime/ /usr/local/lib/faketime/

COPY .envrc ./bin/.envrc
COPY res/ ./res/
COPY node/bin/entrypoint.sh /entrypoint.sh

RUN useradd -u 10001 -d /nonexistent -s /usr/sbin/nologin -M -U -c "" appuser
USER appuser

EXPOSE 30333 9933 9944 9615
ENTRYPOINT ["/entrypoint.sh"]
