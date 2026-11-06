# Context: a +node-rootfs* Earthly artifact. A Dockerfile, not Earthly commands,
# because Earthly writes HEALTHCHECK in shell form, which scratch cannot run.
FROM scratch

COPY rootfs/ /
COPY --chown=10001:10001 rootfs/node /node
# A fresh named volume takes its ownership from the image, so 10001 can write it.
# Copying the empty chain dir is what creates /data: --chown skips existing dirs.
COPY --chown=10001:10001 rootfs/node/chain /data

ENV BASE_PATH=/node/chain
ENV RPC_PORT=9944
ENV RUST_BACKTRACE=1
ENV SSL_CERT_FILE=/etc/pki/tls/certs/ca-bundle.crt

USER 10001:10001

EXPOSE 30333 9933 9944 9615

# `/health` is liveness: 200 whenever RPC answers, even mid-sync or with 0 peers.
# Not `/health/readiness` - that 500s while syncing, so a fresh node would sit
# unhealthy for hours. The probe reads `RPC_PORT`; set it to match a custom
# `--rpc-port`.
HEALTHCHECK --interval=30s --timeout=5s --start-period=2m --retries=3 \
    CMD ["/midnight-node", "health"]

ENTRYPOINT ["/midnight-node"]
