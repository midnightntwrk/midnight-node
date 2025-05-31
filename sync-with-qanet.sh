#!/bin/bash

# Login to the correct cluster to access db-sync
tsh kube login k0-eks-platform-dev-eu-01

export CFG_PRESET=qanet
export BOOTNODES="/dns/boot-node-01.qanet.dev.midnight.network/tcp/30333/ws/p2p/12D3KooWEyoppNCUx8Yx66oV9fJnriXwCcXwDDUA2kj6vnc6iDEp /dns/boot-node-02.qanet.dev.midnight.network/tcp/30333/ws/p2p/12D3KooWHdiAxVd8uMQR1hGWXccidmfCwLqcMpGwR6QcTP6QRMuD /dns/boot-node-03.qanet.dev.midnight.network/tcp/30333/ws/p2p/12D3KooWSCufgHzV4fCwRijfH2k3abrpAJxTKxEvN1FDuRXA2U9x"
./scripts/sync.sh
