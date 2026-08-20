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

//! Static version reporting for binaries.
//!
//! Every binary entry point should invoke [`crate::handle_version_flag!`]
//! before any configuration loading so that `--version` / `-v` works without a
//! `config.yaml` on disk. The version is baked in at compile time via
//! `CARGO_PKG_VERSION` (inherited from the workspace `version`); the optional
//! git SHA and build date come from `indexer-common/build.rs` and are simply
//! omitted on systems where neither `git` nor `date` is available.

const GIT_SHA: Option<&str> = option_env!("MIDNIGHT_INDEXER_GIT_SHA");
const BUILD_DATE: Option<&str> = option_env!("MIDNIGHT_INDEXER_BUILD_DATE");

#[derive(Debug, PartialEq, Eq)]
enum Action {
    Continue,
    PrintVersionAndExit,
}

/// Print the version line to stdout and exit 0 if argv contains a version
/// flag; otherwise return so the caller can continue normal startup.
///
/// Prefer the [`crate::handle_version_flag!`] macro at the call site, which
/// captures the *caller's* `CARGO_BIN_NAME` and `CARGO_PKG_VERSION`
/// automatically so each binary's `main` is a single line.
pub fn handle_version_flag(bin_name: &str, version: &str) {
    if action_for(std::env::args().skip(1)) == Action::PrintVersionAndExit {
        println!("{}", format_version_line(bin_name, version));
        std::process::exit(0);
    }
}

/// One-liner wrapper around [`handle_version_flag`] that fills in the
/// caller's `CARGO_BIN_NAME` and `CARGO_PKG_VERSION` at compile time.
#[macro_export]
macro_rules! handle_version_flag {
    () => {
        $crate::version::handle_version_flag(env!("CARGO_BIN_NAME"), env!("CARGO_PKG_VERSION"))
    };
}

/// Cargo-style version line, optionally appending git SHA and build date in
/// parentheses when the build provided them.
pub fn format_version_line(bin_name: &str, version: &str) -> String {
    format_version_line_with(bin_name, version, GIT_SHA, BUILD_DATE)
}

fn format_version_line_with(
    bin_name: &str,
    version: &str,
    git_sha: Option<&str>,
    build_date: Option<&str>,
) -> String {
    match (git_sha, build_date) {
        (Some(sha), Some(date)) => format!("{bin_name} {version} ({sha} {date})"),
        (Some(sha), None) => format!("{bin_name} {version} ({sha})"),
        (None, Some(date)) => format!("{bin_name} {version} ({date})"),
        (None, None) => format!("{bin_name} {version}"),
    }
}

fn action_for<I, S>(args: I) -> Action
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let found = args
        .into_iter()
        .any(|a| matches!(a.as_ref(), "-v" | "--version"));
    if found {
        Action::PrintVersionAndExit
    } else {
        Action::Continue
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_no_metadata() {
        assert_eq!(
            format_version_line_with("standalone", "4.2.1", None, None),
            "standalone 4.2.1"
        );
    }

    #[test]
    fn format_sha_only() {
        assert_eq!(
            format_version_line_with("standalone", "4.2.1", Some("abc12345"), None),
            "standalone 4.2.1 (abc12345)"
        );
    }

    #[test]
    fn format_date_only() {
        assert_eq!(
            format_version_line_with("standalone", "4.2.1", None, Some("2026-04-28")),
            "standalone 4.2.1 (2026-04-28)"
        );
    }

    #[test]
    fn format_sha_and_date() {
        assert_eq!(
            format_version_line_with("standalone", "4.2.1", Some("abc12345"), Some("2026-04-28")),
            "standalone 4.2.1 (abc12345 2026-04-28)"
        );
    }

    #[test]
    fn detects_long_flag() {
        assert_eq!(action_for(["--version"]), Action::PrintVersionAndExit);
    }

    #[test]
    fn detects_short_flag() {
        assert_eq!(action_for(["-v"]), Action::PrintVersionAndExit);
    }

    #[test]
    fn upper_v_is_not_a_version_flag() {
        assert_eq!(action_for(["-V"]), Action::Continue);
    }

    #[test]
    fn detects_flag_among_other_args() {
        assert_eq!(
            action_for(["--config", "x.yaml", "--version"]),
            Action::PrintVersionAndExit
        );
    }

    #[test]
    fn no_flag_means_continue() {
        assert_eq!(action_for(["--config", "x.yaml"]), Action::Continue);
    }

    #[test]
    fn empty_args_means_continue() {
        assert_eq!(action_for(Vec::<&str>::new()), Action::Continue);
    }
}
