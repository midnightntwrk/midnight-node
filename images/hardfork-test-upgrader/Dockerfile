FROM public.ecr.aws/amazonlinux/amazonlinux:2023-minimal@sha256:13bffb7de7ef4836742a6be2b09642e819aaec50ceed1d7961424e19a95da0de
# ntp to keep correct time
# curl to enable compose healthchecks
# Package versions are tied to the base image — update when bumping the base digest
RUN microdnf -y install \
  ca-certificates-2025.2.76 \
  curl-minimal-8.15.0 \
  procps-ng-3.3.17 \
  strace-6.12 \
  gdb-16.3 \
  vim-enhanced-9.1.1591 \
  jq-1.7.1 \
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
