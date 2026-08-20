// This file is part of midnight-indexer.
// Copyright (C) Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// You may not use this file except in compliance with the License.
// You may obtain a copy of the License at
// http://www.apache.org/licenses/LICENSE-2.0
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use figment::{
    Error, Figment, Metadata, Profile, Provider,
    providers::{Env, Format, Yaml},
    value::{Dict, Map, Tag, Value},
};
use serde::Deserialize;
use std::{
    env, fs,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

const CONFIG_FILE: &str = "CONFIG_FILE";

/// Env var prefix for configuration overlays.
const ENV_PREFIX: &str = "APP__";

/// Nesting separator within env var names: `APP__A__B` sets `a.b`.
const ENV_SEPARATOR: &str = "__";

/// Env var suffix: `APP__X_FILE=/path` sources `APP__X`'s value from the file at `/path`.
const FILE_SUFFIX: &str = "_FILE";

/// Rejects oversized files (secrets are a few KB at most).
const MAX_SECRET_FILE_SIZE: u64 = 64 * 1024;

/// Extension methods for "configuration structs" which can be deserialized.
pub trait ConfigExt
where
    Self: for<'de> Deserialize<'de>,
{
    /// Load the configuration from the file at the value of the `CONFIG_FILE` environment variable
    /// or `config.yaml` by default, with an overlay provided by environment variables prefixed with
    /// `"APP__"` and split/nested via `"__"`.
    ///
    /// `APP__X_FILE=/path` sources `APP__X`'s value from `/path`, so secrets can be mounted as
    /// Kubernetes Secret files. Sources are merged in ascending precedence: config file, secret
    /// files, environment variables.
    fn load() -> Result<Self, Box<figment::Error>> {
        warn_about_env_secrets();

        let config_file = env::var(CONFIG_FILE)
            .map(Yaml::file_exact)
            .unwrap_or(Yaml::file_exact("config.yaml"));

        let config = Figment::new()
            .merge(config_file)
            .merge(SecretFiles::from_env())
            .merge(Env::prefixed(ENV_PREFIX).split(ENV_SEPARATOR))
            .extract()?;

        Ok(config)
    }
}

impl<T> ConfigExt for T where T: for<'de> Deserialize<'de> {}

/// Substrings marking an env var as carrying a secret. Approximate by necessity: which config
/// fields are `SecretString` is not knowable here, because serde never tells a provider what it is
/// deserializing into.
///
/// `ID` is deliberately absent - it would match `APP__APPLICATION__NETWORK_ID` - so
/// `BLOCKFROST_ID` is listed by name instead. Extend both this and its test when adding a secret.
const SECRET_MARKERS: &[&str] = &["SECRET", "PASSWORD", "TOKEN", "CREDENTIAL", "BLOCKFROST_ID"];

/// Whether `key` names a secret being passed insecurely. `has_value` excludes empty placeholders,
/// which is how Docker Compose materialises an unset optional var.
fn is_env_secret(key: &str, has_value: bool) -> bool {
    has_value
        && key.starts_with(ENV_PREFIX)
        && !key.ends_with(FILE_SUFFIX)
        && SECRET_MARKERS.iter().any(|marker| key.contains(marker))
}

/// Warn about secrets passed directly as env vars rather than via `APP__X_FILE`.
///
/// Advice for the *next* deployment, not this one: by the time this runs the value is already in
/// `/proc/<pid>/environ`, where it stays until the process exits regardless of what we do here.
fn warn_about_env_secrets() {
    let insecure = env::vars_os()
        .filter_map(|(key, value)| {
            let key = key.into_string().ok()?;
            is_env_secret(&key, !value.is_empty()).then_some(key)
        })
        .collect::<Vec<_>>();

    if !insecure.is_empty() {
        // stderr, not `log`, because this runs before logger initialisation.
        eprintln!(
            "warning: secrets passed via the environment: {}. Prefer mounting each as a file and \
             pointing `<VAR>{FILE_SUFFIX}` at it; an env var is readable for the lifetime of the \
             process via /proc/<pid>/environ and cannot be scrubbed once set.",
            insecure.join(", ")
        );
    }
}

/// Configuration source reading values from the files named by `APP__X_FILE` env vars.
///
/// The secret goes file -> [`Dict`] -> config struct, never through [`env::set_var`], which would
/// park it in a permanent, un-zeroable store that any code in the process can read back via
/// [`env::var`]. Config structs typically hold the value in a `SecretString`, which zeroizes on
/// drop, so this is the only route by which the secret never outlives its use.
///
/// It is also the only route that keeps the secret out of `/proc/<pid>/environ`, which exposes the
/// environment the process was *exec'd* with. That is a one-way door: `unsetenv` hides an inherited
/// var from [`env::var`] but leaves it in `/proc/<pid>/environ` (verified on glibc), so a directly
/// set `APP__X` cannot be scrubbed from inside the process at all. Values written by
/// [`env::set_var`] never appear there in the first place.
///
/// An unreadable or oversized file is fatal, unlike the directly-set env var it stands in for: a
/// silently-absent DB password fails later and far more confusingly.
#[derive(Debug)]
struct SecretFiles(Vec<(String, PathBuf)>);

impl SecretFiles {
    /// Collect the `APP__X_FILE` vars from the process environment.
    fn from_env() -> Self {
        // `env::vars` panics if *any* var in the process environment is not valid UTF-8, including
        // vars that are none of our business; `vars_os` lets us skip those. Paths need not be UTF-8
        // at all, hence `PathBuf`.
        let vars = env::vars_os()
            .filter_map(|(key, path)| Some((key.into_string().ok()?, PathBuf::from(path))))
            .filter(|(key, _)| {
                key.starts_with(ENV_PREFIX)
                    && key.ends_with(FILE_SUFFIX)
                    // A directly-set `APP__X` wins by merge order anyway; skipping the read avoids
                    // failing startup over a stale `APP__X_FILE` whose value would be discarded.
                    && env::var(&key[..key.len() - FILE_SUFFIX.len()]).is_err()
                    && key_path(key).all(|segment| !segment.is_empty())
            })
            .collect();

        Self(vars)
    }
}

impl Provider for SecretFiles {
    fn metadata(&self) -> Metadata {
        Metadata::named("`APP__*_FILE` secret files")
    }

    fn data(&self) -> Result<Map<Profile, Dict>, Error> {
        // A loop rather than `try_fold`, because a closure returning `figment::Error` trips
        // `clippy::result_large_err`; the trait pins the error type, so it cannot be boxed.
        let mut dict = Dict::new();
        for (key, path) in &self.0 {
            let secret = read_secret_file(path)
                .map_err(|error| Error::from(format!("{key}='{}': {error}", path.display())))?;

            // Always a `String`: unlike `Env`, we do not parse the value, since an all-digit
            // secret coerced to a number would then fail to deserialize into a string field.
            insert_nested(
                &mut dict,
                &key_path(key).collect::<Vec<_>>(),
                Value::String(Tag::Default, secret),
            );
        }

        Ok(Profile::Default.collect(dict))
    }
}

/// Split `APP__INFRA__STORAGE__PASSWORD_FILE` into `["infra", "storage", "password"]`, matching how
/// [`Env`] with `split("__")` lowercases and nests `APP__INFRA__STORAGE__PASSWORD`.
fn key_path(key: &str) -> impl Iterator<Item = String> {
    key[ENV_PREFIX.len()..key.len() - FILE_SUFFIX.len()]
        .split(ENV_SEPARATOR)
        .map(|segment| segment.to_ascii_lowercase())
}

/// Insert `value` at the nested `path`, creating intermediate dicts as needed.
fn insert_nested(dict: &mut Dict, path: &[String], value: Value) {
    let Some((key, rest)) = path.split_first() else {
        return;
    };

    if rest.is_empty() {
        dict.insert(key.to_owned(), value);
        return;
    }

    let entry = dict
        .entry(key.to_owned())
        .or_insert_with(|| Value::Dict(Tag::Default, Dict::new()));

    // A non-dict here means two `_FILE` vars disagree about the shape (`APP__A_FILE` alongside
    // `APP__A__B_FILE`); the deeper path wins, as it does for `Env`.
    let Value::Dict(_, nested) = entry else {
        *entry = Value::Dict(Tag::Default, Dict::new());
        return insert_nested(dict, path, value);
    };

    insert_nested(nested, rest, value);
}

/// Read a secret from `path`, trimming surrounding whitespace, which Kubernetes Secret files and
/// `echo`-created ones routinely carry.
fn read_secret_file(path: &Path) -> Result<String, String> {
    // Advisory pre-check: opening a FIFO blocks until a writer appears, so reject non-regular files
    // before opening one. The authoritative check is against the handle below.
    let metadata = fs::metadata(path).map_err(|error| format!("cannot stat: {error}"))?;
    if !metadata.is_file() {
        return Err("not a regular file".to_owned());
    }

    let file = File::open(path).map_err(|error| format!("cannot open: {error}"))?;

    // Every check from here on is against this handle, so a symlink swapped after the pre-check
    // cannot smuggle in a different or larger file (TOCTOU).
    let metadata = file
        .metadata()
        .map_err(|error| format!("cannot stat: {error}"))?;
    if !metadata.is_file() {
        return Err("not a regular file".to_owned());
    }

    // Bounded by the reader, not by the size reported before the read: the file could have grown.
    let mut secret = String::new();
    file.take(MAX_SECRET_FILE_SIZE + 1)
        .read_to_string(&mut secret)
        .map_err(|error| format!("cannot read: {error}"))?;

    if u64::try_from(secret.len()).unwrap_or(u64::MAX) > MAX_SECRET_FILE_SIZE {
        return Err(format!("exceeds the {MAX_SECRET_FILE_SIZE}-byte limit"));
    }

    Ok(secret.trim().to_owned())
}

#[cfg(test)]
mod tests {
    use crate::config::{
        CONFIG_FILE, ConfigExt, MAX_SECRET_FILE_SIZE, SecretFiles, is_env_secret, key_path,
        read_secret_file,
    };
    use assert_matches::assert_matches;
    use figment::{Figment, Provider};
    use serde::Deserialize;
    use std::{env, io::Write, path::PathBuf};
    use tempfile::NamedTempFile;

    #[test]
    fn test_load() {
        unsafe {
            env::set_var("APP__API__PORT", "4242");
        }

        let config = MainConfig::load();
        assert_matches!(
            config,
            Ok(MainConfig { config: Config { api: api::Config { port, .. } } }) if port == 4242
        );

        unsafe {
            env::set_var(CONFIG_FILE, "nonexistent.yaml");
        }
        let config = Config::load();
        assert!(config.is_err());
    }

    /// Every var the Indexer treats as a secret must be flagged when set directly, and nothing else
    /// may be. Extend alongside `SECRET_MARKERS` when a new `SecretString` config field appears.
    #[test]
    fn test_is_env_secret() {
        let secrets = [
            "APP__INFRA__SECRET",
            "APP__INFRA__STORAGE__PASSWORD",
            "APP__INFRA__PUB_SUB__PASSWORD",
            "APP__INFRA__NODE__BLOCKFROST_ID",
        ];
        for key in secrets {
            assert!(is_env_secret(key, true), "{key} should be flagged");
            assert!(
                !is_env_secret(key, false),
                "{key} is empty, so a placeholder"
            );
        }

        let benign = [
            // The reason `ID` is not itself a marker.
            "APP__APPLICATION__NETWORK_ID",
            "APP__INFRA__STORAGE__PORT",
            "APP__INFRA__NODE__URL",
            // Already doing the right thing.
            "APP__INFRA__SECRET_FILE",
            "APP__INFRA__STORAGE__PASSWORD_FILE",
            // Not ours.
            "POSTGRES_PASSWORD",
            "RUST_LOG",
        ];
        for key in benign {
            assert!(!is_env_secret(key, true), "{key} should not be flagged");
        }
    }

    #[test]
    fn test_key_path() {
        let segments = key_path("APP__INFRA__STORAGE__PASSWORD_FILE").collect::<Vec<_>>();
        assert_eq!(segments, ["infra", "storage", "password"]);
    }

    #[test]
    fn test_secret_files_nests_and_lowercases() {
        let secrets = SecretFiles(vec![
            (
                "APP__INFRA__STORAGE__PASSWORD_FILE".to_owned(),
                secret_file("storage-password"),
            ),
            (
                "APP__INFRA__PUB_SUB__PASSWORD_FILE".to_owned(),
                secret_file("pub-sub-password"),
            ),
        ]);

        let infra = extract::<Infra>(secrets);
        assert_eq!(infra.infra.storage.password, "storage-password");
        assert_eq!(infra.infra.pub_sub.password, "pub-sub-password");
    }

    #[test]
    fn test_secret_files_trims_whitespace() {
        let secrets = single("  \tpadded-secret\n  \n");
        assert_eq!(extract::<Secret>(secrets).secret, "padded-secret");
    }

    /// An all-digit secret must stay a string; `Env` would parse it into a number, which then fails
    /// to deserialize into a string field.
    #[test]
    fn test_secret_files_does_not_coerce_numeric_secret() {
        let secrets = single("12345678");
        assert_eq!(extract::<Secret>(secrets).secret, "12345678");
    }

    #[test]
    fn test_secret_files_missing_file_is_fatal() {
        let secrets = SecretFiles(vec![(
            "APP__SECRET_FILE".to_owned(),
            PathBuf::from("/this/file/does/not/exist"),
        )]);

        let error = secrets
            .data()
            .expect_err("missing file must be fatal")
            .to_string();
        assert!(
            error.contains("APP__SECRET_FILE"),
            "error should name the var: {error}"
        );
        assert!(
            error.contains("cannot stat"),
            "error should say what failed: {error}"
        );
    }

    #[test]
    fn test_secret_files_rejects_non_regular_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let error = read_secret_file(dir.path()).expect_err("a directory is not a secret");
        assert_eq!(error, "not a regular file");
    }

    #[test]
    fn test_secret_files_rejects_oversized_file() {
        let mut file = NamedTempFile::new().expect("tempfile");
        let oversized = "x".repeat(usize::try_from(MAX_SECRET_FILE_SIZE).expect("fits") + 1);
        file.write_all(oversized.as_bytes())
            .expect("write tempfile");

        let error = read_secret_file(file.path()).expect_err("oversized file must be rejected");
        assert!(error.contains("exceeds"), "error should say why: {error}");
    }

    /// The point of `*_FILE` is to keep the secret out of the process environment. Guards against a
    /// regression to materialising it via `env::set_var`.
    #[test]
    fn test_secret_file_never_enters_process_env() {
        let path = secret_file("secret-from-file");

        unsafe {
            env::remove_var("APP__NOT_IN_ENV");
            env::set_var("APP__NOT_IN_ENV_FILE", &path);
        }

        let secret = Figment::new()
            .merge(SecretFiles::from_env())
            .extract::<NotInEnv>()
            .expect("secret file should deserialize");

        assert_eq!(secret.not_in_env, "secret-from-file");
        assert!(
            env::var("APP__NOT_IN_ENV").is_err(),
            "the secret must never be written to the process environment",
        );

        unsafe {
            env::remove_var("APP__NOT_IN_ENV_FILE");
        }
    }

    #[test]
    fn test_env_var_beats_secret_file() {
        let path = secret_file("from-file");

        unsafe {
            env::set_var("APP__OVERRIDE_SECRET", "direct-value");
            env::set_var("APP__OVERRIDE_SECRET_FILE", &path);
        }

        let secret = Figment::new()
            .merge(SecretFiles::from_env())
            .merge(figment::providers::Env::prefixed("APP__").split("__"))
            .extract::<OverrideSecret>()
            .expect("override should deserialize");

        assert_eq!(
            secret.override_secret, "direct-value",
            "a directly-set env var must win over `*_FILE`"
        );

        unsafe {
            env::remove_var("APP__OVERRIDE_SECRET");
            env::remove_var("APP__OVERRIDE_SECRET_FILE");
        }
    }

    /// Write `content` to a temp file, leaking the handle so the path outlives this call.
    fn secret_file(content: &str) -> PathBuf {
        let mut file = NamedTempFile::new().expect("tempfile");
        write!(file, "{content}").expect("write tempfile");
        let (_, path) = file.keep().expect("keep tempfile");
        path
    }

    fn single(content: &str) -> SecretFiles {
        SecretFiles(vec![("APP__SECRET_FILE".to_owned(), secret_file(content))])
    }

    fn extract<T>(secrets: SecretFiles) -> T
    where
        T: for<'de> Deserialize<'de>,
    {
        Figment::new()
            .merge(secrets)
            .extract()
            .expect("secret files should deserialize")
    }

    #[derive(Debug, Deserialize)]
    struct Secret {
        secret: String,
    }

    #[derive(Debug, Deserialize)]
    struct NotInEnv {
        not_in_env: String,
    }

    #[derive(Debug, Deserialize)]
    struct OverrideSecret {
        override_secret: String,
    }

    #[derive(Debug, Deserialize)]
    struct Infra {
        infra: InfraInner,
    }

    #[derive(Debug, Deserialize)]
    struct InfraInner {
        storage: Password,
        pub_sub: Password,
    }

    #[derive(Debug, Deserialize)]
    struct Password {
        password: String,
    }

    #[derive(Debug, Clone, Deserialize)]
    pub struct MainConfig {
        /// Application specific configuration.
        #[serde(flatten)]
        pub config: Config,
    }

    /// Application specific configuration.
    #[derive(Debug, Clone, Deserialize)]
    pub struct Config {
        pub api: api::Config,
    }

    mod api {
        use serde::Deserialize;

        #[derive(Debug, Clone, Deserialize)]
        pub struct Config {
            pub port: u16,
        }
    }
}
