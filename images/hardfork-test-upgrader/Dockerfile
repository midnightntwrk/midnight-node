FROM public.ecr.aws/amazonlinux/amazonlinux:2023-minimal
# ntp to keep correct time
# curl to enable compose healthchecks
RUN microdnf -y update && \
  microdnf -y install ca-certificates curl-minimal procps-ng strace gdb vim jq tree && \
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
