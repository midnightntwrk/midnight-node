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

//! Opt-in deflate compression for GraphQL subscription WebSockets.
//!
//! Wire format for the `graphql-transport-ws+deflate` subprotocol:
//!
//! - Server to client: graphql-transport-ws JSON messages of at least `MIN_COMPRESS_LEN` (256)
//!   bytes are sent as binary frames containing the zlib-compressed (RFC 1950) UTF-8 JSON, which
//!   browsers decompress with the built-in `DecompressionStream('deflate')`. Smaller messages
//!   remain plain text frames, so clients must handle both frame types.
//! - Client to server: text frames carry plain JSON as usual; binary frames are treated as
//!   zlib-compressed JSON and are inflated subject to `MAX_INFLATED_LEN` (1 MiB).
//! - Control frames (ping/pong/close) are never compressed.
//!
//! Clients must offer the standard `graphql-transport-ws` subprotocol alongside
//! `graphql-transport-ws+deflate`: the former satisfies the GraphQL protocol negotiation, the
//! latter enables compression. Clients that do not offer the deflate variant are byte-for-byte
//! unaffected.
//!
//! Compression uses a fresh context per message, so there is no per-connection compressor state
//! and no shared-window memory cost per connection.

use axum::extract::ws::Message;
use flate2::{Compression, read::ZlibDecoder, write::ZlibEncoder};
use futures::{Sink, Stream};
use std::{
    io::{Read, Write},
    pin::Pin,
    task::{Context, Poll},
};

/// The subprotocol identifier offered in addition to the standard `graphql-transport-ws`.
pub const GRAPHQL_TRANSPORT_WS_DEFLATE: &str = "graphql-transport-ws+deflate";

/// Outbound messages shorter than this are left uncompressed: at this size the deflate overhead
/// outweighs the gain and the wire format permits mixing text and binary frames.
const MIN_COMPRESS_LEN: usize = 256;

/// Upper bound for inflated client-to-server payloads (decompression bomb guard); aligned with
/// the default HTTP `request_body_limit`.
const MAX_INFLATED_LEN: usize = 1024 * 1024;

/// Wraps a WebSocket such that outbound text messages are zlib-compressed into binary frames and
/// inbound binary frames are inflated back into text messages.
pub struct DeflateWebSocket<S> {
    inner: S,
}

impl<S> DeflateWebSocket<S> {
    pub fn new(inner: S) -> Self {
        Self { inner }
    }
}

impl<S> Stream for DeflateWebSocket<S>
where
    S: Stream<Item = Result<Message, axum::Error>> + Unpin,
{
    type Item = Result<Message, axum::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match Pin::new(&mut self.inner).poll_next(cx) {
            Poll::Ready(Some(Ok(Message::Binary(data)))) => Poll::Ready(Some(
                inflate_capped(&data)
                    .map(|text| Message::Text(text.into()))
                    .map_err(axum::Error::new),
            )),

            other => other,
        }
    }
}

impl<S> Sink<Message> for DeflateWebSocket<S>
where
    S: Sink<Message, Error = axum::Error> + Unpin,
{
    type Error = axum::Error;

    fn poll_ready(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Pin::new(&mut self.inner).poll_ready(cx)
    }

    fn start_send(mut self: Pin<&mut Self>, message: Message) -> Result<(), Self::Error> {
        let message = match message {
            Message::Text(text) if text.len() >= MIN_COMPRESS_LEN => {
                let compressed = compress(text.as_bytes()).map_err(axum::Error::new)?;
                Message::Binary(compressed.into())
            }

            other => other,
        };

        Pin::new(&mut self.inner).start_send(message)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Pin::new(&mut self.inner).poll_close(cx)
    }
}

#[derive(Debug, thiserror::Error)]
enum InflateError {
    #[error("cannot inflate WebSocket payload")]
    Inflate(#[from] std::io::Error),

    #[error("inflated WebSocket payload exceeds {MAX_INFLATED_LEN} bytes")]
    TooLarge,

    #[error("inflated WebSocket payload is not valid UTF-8")]
    Utf8(#[from] std::string::FromUtf8Error),
}

fn compress(payload: &[u8]) -> std::io::Result<Vec<u8>> {
    let mut encoder = ZlibEncoder::new(
        Vec::with_capacity(payload.len() / 4),
        Compression::default(),
    );
    encoder.write_all(payload)?;
    encoder.finish()
}

fn inflate_capped(payload: &[u8]) -> Result<String, InflateError> {
    let mut inflated = Vec::new();
    ZlibDecoder::new(payload)
        .take(MAX_INFLATED_LEN as u64 + 1)
        .read_to_end(&mut inflated)?;

    if inflated.len() > MAX_INFLATED_LEN {
        return Err(InflateError::TooLarge);
    }

    Ok(String::from_utf8(inflated)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::{SinkExt, StreamExt, stream};

    #[test]
    fn compress_inflate_roundtrip() {
        let payload =
            r#"{"type":"next","id":"1","payload":{"data":{"blocks":{"height":42}}}}"#.repeat(10);
        let compressed = compress(payload.as_bytes()).unwrap();
        assert!(compressed.len() < payload.len());
        assert_eq!(inflate_capped(&compressed).unwrap(), payload);
    }

    #[test]
    fn inflate_rejects_oversized_payload() {
        let bomb = compress(&vec![0u8; MAX_INFLATED_LEN + 1]).unwrap();
        assert!(matches!(inflate_capped(&bomb), Err(InflateError::TooLarge)));
    }

    #[test]
    fn inflate_rejects_garbage() {
        assert!(matches!(
            inflate_capped(b"not zlib data"),
            Err(InflateError::Inflate(_))
        ));
    }

    #[test]
    fn inflate_rejects_invalid_utf8() {
        let compressed = compress(&[0xff, 0xfe, 0xfd]).unwrap();
        assert!(matches!(
            inflate_capped(&compressed),
            Err(InflateError::Utf8(_))
        ));
    }

    #[tokio::test]
    async fn sink_compresses_large_text_only() {
        let mut messages = Vec::new();
        {
            let sink = futures::sink::unfold((), |(), message: Message| {
                messages.push(message);
                async move { Ok::<_, axum::Error>(()) }
            });
            futures::pin_mut!(sink);

            let large = "x".repeat(MIN_COMPRESS_LEN);
            let small = "y".repeat(MIN_COMPRESS_LEN - 1);
            // DeflateWebSocket requires Unpin, the pinned unfold sink is used via &mut.
            let mut socket = DeflateWebSocket::new(&mut sink);
            socket
                .send(Message::Text(large.clone().into()))
                .await
                .unwrap();
            socket
                .send(Message::Text(small.clone().into()))
                .await
                .unwrap();

            let [first, second] = &messages[..] else {
                panic!("expected two messages");
            };
            match first {
                Message::Binary(data) => assert_eq!(inflate_capped(data).unwrap(), large),
                other => panic!("expected binary frame, got {other:?}"),
            }
            assert!(matches!(second, Message::Text(text) if text.as_str() == small));
        }
    }

    #[tokio::test]
    async fn stream_inflates_binary_and_passes_text_through() {
        let compressed = compress(b"inflated payload").unwrap();
        let inner = stream::iter([
            Ok::<_, axum::Error>(Message::Binary(compressed.into())),
            Ok(Message::Text("plain".into())),
        ]);
        futures::pin_mut!(inner);

        let mut socket = DeflateWebSocket::new(&mut inner);

        let first = socket.next().await.unwrap().unwrap();
        assert!(matches!(first, Message::Text(text) if text.as_str() == "inflated payload"));

        let second = socket.next().await.unwrap().unwrap();
        assert!(matches!(second, Message::Text(text) if text.as_str() == "plain"));
    }
}
