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

use futures::{Stream, StreamExt, stream};

/// Flattens a stream of results of chunks of items into a stream of results of items.
pub fn flatten_chunks<T, E>(
    chunks: impl Stream<Item = Result<Vec<T>, E>>,
) -> impl Stream<Item = Result<T, E>> {
    chunks.flat_map(|chunk: Result<Vec<_>, E>| match chunk {
        Ok(chunk) => stream::iter(chunk.into_iter().map(Ok)).left_stream(),
        Err(error) => stream::iter([Err(error)]).right_stream(),
    })
}

#[cfg(test)]
mod tests {
    use crate::stream::flatten_chunks;
    use assert_matches::assert_matches;
    use futures::{TryStreamExt, stream};
    use std::convert::Infallible;

    #[tokio::test]
    async fn test_flatten_chunks() {
        let chunks = stream::iter(vec![Ok::<_, Infallible>(vec![1, 2, 3]), Ok(vec![4, 5, 6])]);
        let flattened = flatten_chunks(chunks).try_collect::<Vec<_>>().await;
        assert_matches!(flattened, Ok(x) if x == vec![1, 2, 3, 4, 5, 6]);

        let chunks = stream::iter(vec![Ok::<_, &'static str>(vec![1, 2, 3]), Err("error")]);
        let mut flattened = flatten_chunks(chunks);
        assert_eq!(flattened.try_next().await, Ok(Some(1)));
        assert_eq!(flattened.try_next().await, Ok(Some(2)));
        assert_eq!(flattened.try_next().await, Ok(Some(3)));
        assert_eq!(flattened.try_next().await, Err("error"));
    }
}
