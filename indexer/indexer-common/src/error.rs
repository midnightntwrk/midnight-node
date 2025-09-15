// This file is part of midnight-indexer.
// Copyright (C) 2025 Midnight Foundation
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

use std::error::Error as StdError;

/// Alias for `async` and `anyhow` friendly dynamic error
/// `Box<dyn std::error::Error + Send + Sync + 'static>`.
pub type BoxError = Box<dyn StdError + Send + Sync + 'static>;

/// Extension methods for types implementing `std::error::Error`.
pub trait StdErrorExt
where
    Self: StdError,
{
    /// Format this error as a chain of colon separated strings built from this error and all
    /// recursive sources.
    ///
    /// Can be used to log errors like this:
    ///
    /// `error!(error = error.as_chain(), "cannot do this or that");`
    fn as_chain(&self) -> String {
        let mut sources = vec![];
        sources.push(self.to_string());

        let mut source = self.source();
        while let Some(s) = source {
            sources.push(s.to_string());
            source = s.source();
        }

        sources.join(": ")
    }
}

impl<T> StdErrorExt for T where T: StdError {}

#[cfg(test)]
mod tests {
    use crate::error::StdErrorExt;
    use std::num::ParseIntError;
    use thiserror::Error;

    #[test]
    fn test_as_chain() {
        let number = "-1".parse::<u32>().map_err(Error);
        assert_eq!(
            number.unwrap_err().as_chain(),
            "error: invalid digit found in string"
        );
    }

    #[derive(Debug, Error)]
    #[error("error")]
    struct Error(#[source] ParseIntError);
}
