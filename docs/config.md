# Configuring the Midnight Node

To run the node in local dev mode, use the following command:

```shell
$ docker run --env CFG_PRESET=dev ghcr.io/midnight-ntwrk/midnight-node:latest
```

You should see the node start, with blocks beginning to author.

If you'd like to see the current configuration of the node, set the `SHOW_CONFIG` environment variable to true:

```shell
$ docker run --env CFG_PRESET=dev --env SHOW_CONFIG=1 ghcr.io/midnight-ntwrk/midnight-node:latest
```

Or run using `--help`

```shell
$ docker run ghcr.io/midnight-ntwrk/midnight-node:latest --help
```

This will show you the most up-to-date list of all configuration values available in the node.

## Equivalence with Substrate CLI

The node application has been designed to maintain equivalence with the default
Substrate CLI as far as possible. This means online support articles that solve
your problem that reference the Substrate CLI will work for the Midnight node.

You can either set the CLI by passing it as arguments to the docker image, or by setting the `ARGS` environment variable.

Example of equivalent commands:

```shell
$ docker run ghcr.io/midnight-ntwrk/midnight-node:latest --dev
```

```shell
$ docker run --env ARGS="--dev" ghcr.io/midnight-ntwrk/midnight-node:latest
```
