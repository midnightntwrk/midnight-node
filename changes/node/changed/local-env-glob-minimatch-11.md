#local-env
# local-environment: commands can find network files again

`run`, `stop`, `image-upgrade` and `verify-finality` failed with `No .network.yaml file found for namespace '<net>'` because the `minimatch` 11 security override broke `glob` 11, which then matched nothing. `glob` is now 14, which is built for `minimatch` 11, and the override is `^11.0.5`.
