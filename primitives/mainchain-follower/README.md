# Compile-time checked database queries

The database queries in this repo are checked at compile-time. When changing a query, the metadata for that query must be re-generated. This can be done via earthly:

```bash
$ earthly +rebuild-sqlx
```

**NOTE:** `local-env` must be running for this to work! `earthly +start-local-env-latest`
