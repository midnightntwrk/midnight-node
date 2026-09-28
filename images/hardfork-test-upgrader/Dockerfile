FROM public.ecr.aws/amazonlinux/amazonlinux:2023-minimal@sha256:68b1e82cd69ade271c092bc1959fa6fc820945d0c8dbb7a88cf6c1824a184688
# ntp to keep correct time
# curl to enable compose healthchecks
# Package versions are tied to the base image — update when bumping the base digest
RUN microdnf -y install \
  ca-certificates-2025.2.76 \
  curl-minimal-8.21.0 \
  procps-ng-3.3.17 \
  strace-6.12 \
  gdb-16.3 \
  vim-enhanced-9.2.920 \
  jq-1.8.1 \
  tree-1.8.0 && \
  microdnf clean all && rm -rf /var/cache/dnf /var/cache/yum

ENV RUNTIME_PATH=/midnight_node_runtime.compact.compressed.wasm

# Get node version for the image tag
COPY node/Cargo.toml /node/
RUN cat /node/Cargo.toml | grep -m 1 version | sed 's/version *= *"\([^\"]*\)".*/\1/' > node_version && \
  rm -rf /node
COPY util/upgrader/bin/entrypoint.sh /entrypoint.sh

RUN useradd -u 10001 -d /nonexistent -s /usr/sbin/nologin -M -U -c "" appuser
USER appuser
ENTRYPOINT ["/entrypoint.sh"]
