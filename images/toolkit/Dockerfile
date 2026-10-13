FROM public.ecr.aws/amazonlinux/amazonlinux:2023-minimal@sha256:68b1e82cd69ade271c092bc1959fa6fc820945d0c8dbb7a88cf6c1824a184688
# ntp to keep correct time
# curl to enable compose healthchecks
# Package versions are tied to the base image — update when bumping the base digest
RUN microdnf -y install \
  ca-certificates-2025.2.76 \
  curl-minimal-8.21.0 \
  procps-ng-3.3.17 \
  strace-6.12 \
  vim-enhanced-9.2.920 \
  jq-1.8.1 \
  tree-1.8.0 && \
  microdnf clean all && rm -rf /var/cache/dnf /var/cache/yum

# Create appuser before the COPYs so files land already owned by it: a later
# `chown -R` rewrites every copied file into a duplicate image layer.
RUN useradd -u 10001 -d /nonexistent -s /usr/sbin/nologin -M -U -c "" appuser

COPY --chown=appuser:appuser .envrc ./bin/.envrc
COPY --chown=appuser:appuser static/contracts/simple-merkle-tree /test-static/simple-merkle-tree

ENV TOOLKIT_JS_PATH="/toolkit-js"
ENV MIDNIGHT_LEDGER_TEST_STATIC_DIR=/test-static
ENV MIDNIGHT_PP=/.cache/midnight/zk-params
ENV MN_FETCH_CACHE=redb:/.cache/toolkit_fetch_cache.db
ENV MN_LEDGER_CACHE_DB=/.cache/toolkit_ledger_cache_db

# Get node version for the image tag
COPY node/Cargo.toml /node/
RUN cat /node/Cargo.toml | grep -m 1 version | sed 's/version *= *"\([^\"]*\)".*/\1/' > node_version && \
  rm -rf /node
COPY util/toolkit/bin/entrypoint.sh /entrypoint.sh

# One-shot CLI, not a service: nothing long-lived to probe. NONE states that
# explicitly (and satisfies HEALTHCHECK-presence lints such as checkov CKV_DOCKER_2).
HEALTHCHECK NONE

USER appuser
ENTRYPOINT ["/entrypoint.sh"]
