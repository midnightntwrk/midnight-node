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

use anyhow::{Context, anyhow, bail};
use derive_more::Display;
use futures::{
    SinkExt, Stream, StreamExt, TryStreamExt,
    future::{err, ok},
    stream::{SplitSink, SplitStream},
};
use graphql_client::{GraphQLQuery, QueryBody};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::LazyLock;
use tokio::net::TcpStream;
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async,
    tungstenite::{Message, client::IntoClientRequest},
};

type WsWrite = SplitSink<WsStream, Message>;
type WsRead = SplitStream<WsStream>;
type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

static CONNECTION_INIT: LazyLock<String> = LazyLock::new(|| {
    json!({
        "type": "connection_init",
    })
    .to_string()
});

/// Subscribe to the given GraphQL Websocket URL (typically ending with /graphql/ws) and
/// query variables.
pub async fn subscribe<T>(
    url: &str,
    variables: T::Variables,
) -> anyhow::Result<impl Stream<Item = anyhow::Result<T::ResponseData>>>
where
    T: GraphQLQuery,
{
    let ws_stream = connect_graphql_ws(url)
        .await
        .context("connect graphql websocket connection")?;

    let (mut write, mut read) = ws_stream.split();

    init_graphql_ws(&mut write, &mut read)
        .await
        .context("initialize graphql websocket connection")?;

    let QueryBody {
        variables,
        query,
        operation_name,
    } = T::build_query(variables);

    let subscribe_message = json!({
        "type": "subscribe",
        "id": "1",
        "payload": {
            "operationName": operation_name,
            "query": query,
            "variables": variables,
        }
    });

    write
        .send(Message::text(subscribe_message.to_string()))
        .await
        .context("send subscribe message")?;

    let messages = read
        .map(|result| {
            result
                .context("get next message")
                .and_then(|message| match message {
                    Message::Text(text) => serde_json::from_str::<ServerMessage>(&text)
                        .with_context(|| {
                            format!("deserialize text message to ServerMessage: {text}")
                        }),

                    _ => Err(anyhow!("unexpected non-text message")),
                })
        })
        .try_filter_map(|message| match message {
            ServerMessage::Next { payload } => match (payload.data, payload.errors) {
                (Some(data), None) => serde_json::from_value::<T::ResponseData>(data)
                    .map(|data| ok(Some(data)))
                    .unwrap_or_else(|error| err(anyhow!(error))),

                (None, Some(errors)) => err(anyhow!(
                    errors
                        .iter()
                        .map(|e| e.message.to_owned())
                        .collect::<Vec<_>>()
                        .join(", ")
                )),

                _ => err(anyhow!("unexpected GraphQL execution result")),
            },

            ServerMessage::Complete => ok(None),

            ServerMessage::Error { payload } => err(anyhow!(payload)),
        });

    Ok(messages)
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub enum ServerMessage {
    Next { payload: ExecutionResult },
    Complete,
    Error { payload: Value },
}

#[derive(Debug, Deserialize)]
pub struct ExecutionResult {
    pub data: Option<Value>,
    pub errors: Option<Vec<GraphQLError>>,
}

#[derive(Debug, Deserialize, Display)]
#[display("{message}")]
pub struct GraphQLError {
    pub message: String,
}

/// Connect to the given WebSocket URL and return the WebSocket stream.
async fn connect_graphql_ws(url: &str) -> anyhow::Result<WsStream> {
    let mut request = url
        .into_client_request()
        .context("convert url into client request")?;

    // Insert the GraphQL WebSocket subprotocol.
    let graphql_transport_ws = "graphql-transport-ws"
        .parse()
        .context("parse graphql-transport-ws as header value")?;
    request
        .headers_mut()
        .insert("Sec-WebSocket-Protocol", graphql_transport_ws);

    // Connect to the WebSocket server.
    let (ws_stream, _) = connect_async(request)
        .await
        .context("connect to WebSocket server")?;

    Ok(ws_stream)
}

/// Establish the GraphQL WebSocket connection by performing the handshake.
pub async fn init_graphql_ws(write: &mut WsWrite, read: &mut WsRead) -> anyhow::Result<()> {
    // Send the connection_init message.
    write
        .send(Message::text(&*CONNECTION_INIT))
        .await
        .context("send connection_init")?;

    // Await  the connection_ack message.
    let Some(message) = read.try_next().await.context("read WebSocket message")? else {
        bail!("WebSocket connection closed while awaiting connection_ack");
    };

    let Message::Text(message) = message else {
        bail!("received non-text message for connection_ack");
    };

    let message = serde_json::from_str::<Value>(&message).context("parse text message as JSON")?;

    let Value::String(tpe) = &message["type"] else {
        bail!("not received JSON object with string 'type' key");
    };

    if tpe != "connection_ack" {
        bail!("not received connection_ack");
    }

    Ok(())
}
