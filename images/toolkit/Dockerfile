FROM public.ecr.aws/amazonlinux/amazonlinux:2023-minimal@sha256:13bffb7de7ef4836742a6be2b09642e819aaec50ceed1d7961424e19a95da0de
# ntp to keep correct time
# curl to enable compose healthchecks
RUN microdnf -y update && \
  microdnf -y install ca-certificates curl-minimal procps-ng strace gdb vim jq tree && \
  microdnf clean all && rm -rf /var/cache/dnf /var/cache/yum

COPY .envrc ./bin/.envrc
COPY static/contracts/simple-merkle-tree /test-static/simple-merkle-tree

ENV TOOLKIT_JS_PATH="/toolkit-js"
ENV MIDNIGHT_LEDGER_TEST_STATIC_DIR=/test-static
ENV MIDNIGHT_PP=/.cache/midnight/zk-params
ENV MN_FETCH_CACHE=redb:/.cache/fetch/toolkit.db

# Get node version for the image tag
COPY node/Cargo.toml /node/
RUN cat /node/Cargo.toml | grep -m 1 version | sed 's/version *= *"\([^\"]*\)".*/\1/' > node_version && \
  rm -rf /node
COPY util/toolkit/bin/entrypoint.sh /entrypoint.sh

RUN useradd -u 10001 -d /nonexistent -s /usr/sbin/nologin -M -U -c "" appuser
USER appuser
ENTRYPOINT ["/entrypoint.sh"]
