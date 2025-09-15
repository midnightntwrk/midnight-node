# e2e tests for local-env

To run them, ensure local-env is running first:

```
cd ../
earthly +start-local-env-latest
# or e.g. earthly +start-local-env --NODE_IMAGE=ghcr.io/midnight-ntwrk/midnight-node:0.13.0-95623962
```

Then run:

```
yarn run dev
# or
yarn run build && yarn run start
```
