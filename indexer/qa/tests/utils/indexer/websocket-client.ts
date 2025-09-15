// This file is part of midnightntwrk/midnight-indexer
// Copyright (C) 2025 Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// You may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

import { env } from 'environment/model';
import { GraphQLError } from 'graphql';
import log from '@utils/logging/logger';
import type {
  Block,
  BlockOffset,
  ShieldedTransactionsEvent,
  UnshieldedTransactionEvent,
  GraphQLResponse,
} from './indexer-types';
import {
  BLOCKS_SUBSCRIPTION_FROM_BLOCK_BY_OFFSET,
  BLOCKS_SUBSCRIPTION_FROM_LATEST_BLOCK,
  SHIELDED_TRANSACTION_SUBSCRIPTION_BY_SESSION_ID,
  UNSHIELDED_TX_SUBSCRIPTION_BY_ADDRESS,
  UNSHIELDED_TX_SUBSCRIPTION_BY_ADDRESS_AND_TRANSACTION_ID,
} from './graphql/subscriptions';

export type BlockSubscriptionResponse = GraphQLResponse<{ blocks: Block }>;

export type UnshieldedTxSubscriptionResponse = GraphQLResponse<{
  unshieldedTransactions: UnshieldedTransactionEvent;
}>;

export type ShieldedTxSubscriptionResponse = GraphQLResponse<{
  shieldedTransactions: ShieldedTransactionsEvent;
}>;

/**
 * Handlers used to respond to incoming GraphQL subscription messages.
 */
export interface SubscriptionHandlers<T> {
  /** Called when a new payload is received */
  next: (value: T) => void;

  /** Called when an error is received */
  error?: (err: Error | GraphQLError) => void;

  /** Called when the subscription completes */
  complete?: (message: GraphQLCompleteMessage) => void;
}

export interface UnshieldedTransactionSubscriptionParams {
  /** The unshielded address to subscribe to transactions for */
  address: string;
  /** The transaction ID to start subscribing from (inclusive) */
  transactionId?: number;
}

/**
 * GraphQL protocol-compliant connection_init message structure.
 */
interface GraphQLConnectionInitMessage {
  type: 'connection_init';
  payload?: Record<string, unknown>;
}

/**
 * GraphQL protocol-compliant request message structure based on the fact
 * that indexer accepts both subscriptions and mutations
 */
export interface GraphQLStartMessage {
  id: string;
  type: 'start';
  payload: {
    query: string;
    variables?: Record<string, unknown>;
  };
}

/**
 * GraphQL protocol-compliant stop message structure.
 */
export interface GraphQLStopMessage {
  id: string;
  type: 'stop';
}

/**
 * GraphQL protocol-compliant open session message
 */
export interface GraphQLOpenSessionMessage {
  id: string;
  type: 'next';
  payload: {
    data: {
      connect: string;
    };
  };
}

/**
 * GraphQL protocol-compliant close session message
 */
export interface GraphQLCloseSessionMessage {
  id: string;
  type: 'stop';
  payload: {
    data: {
      disconnect: null;
    };
  };
}

/**
 * GraphQL protocol-compliant complete message
 */
export interface GraphQLCompleteMessage {
  id: string;
  type: 'complete';
}

/**
 * A low-level WebSocket client that directly implements the GraphQL over WebSocket protocol.
 * Supports mutations and streaming subscriptions to blocks, transactions, contracts and wallet
 * related events
 */
export class IndexerWsClient {
  /** The active WebSocket connection */
  private ws: WebSocket;

  /** The endpoint where to send graphql subscriptions */
  private readonly graphqlAPIEndpoint: string = '/api/v1/graphql/ws';

  /** Counter to generate unique operation IDs */
  private nextId = 0;

  /** Maps operation IDs to their registered event handlers */
  private handlersMap = new Map<string, SubscriptionHandlers<unknown>>();

  /**
   * Initializes a new WebSocket connection using the GraphQL transport protocol.
   */
  constructor() {
    const targetUrl = env.getIndexerWebsocketBaseURL() + this.graphqlAPIEndpoint;
    this.ws = new WebSocket(targetUrl, 'graphql-transport-ws');
    this.ws.onmessage = this.handleMessage.bind(this);
  }

  /**
   * Initialises the websocket connection with a connection_init message
   */
  async connectionInit(payload?: Record<string, unknown>): Promise<void> {
    // Let's wait 2 secs for the connection to be established first, before sending
    // the connection init
    const timeoutMs = 2000;
    log.debug(`Ready state = ${IndexerWsClient.getStateName(this.ws.readyState)}`);
    if (this.ws.readyState !== WebSocket.OPEN) {
      const maxTime = Date.now() + timeoutMs;
      while (this.ws.readyState !== WebSocket.OPEN) {
        if (Date.now() > maxTime) {
          throw new Error('WebSocket connection timeout');
        }
        await new Promise((res) => setTimeout(res, 50));
        log.debug(`Ready state = ${IndexerWsClient.getStateName(this.ws.readyState)}`);
      }
    }

    const init: GraphQLConnectionInitMessage = {
      type: 'connection_init',
      payload, // Payload is optional and can be used for negotiation
    };

    const response: Promise<{ type: string }> = new Promise((resolve, reject) => {
      const timeout = setTimeout(() => {
        this.ws.removeEventListener('message', onMessage);
        reject(new Error('Timed out waiting for connection_ack'));
      }, timeoutMs);

      const onMessage = (event: MessageEvent) => {
        const message = JSON.parse(event.data);
        if (message.type === 'connection_ack') {
          clearTimeout(timeout);
          this.ws.removeEventListener('message', onMessage);
          resolve(message);
        }
      };

      this.ws.addEventListener('message', onMessage);
      this.ws.send(JSON.stringify(init));
    });

    assert((await response).type == 'connection_ack');
  }

  /**
   * Terminates the underlying WebSocket connection to the indexer.
   */
  async connectionClose(): Promise<void> {
    const closePromise = new Promise<void>((resolve, reject) => {
      const onClose = () => {
        this.ws.removeEventListener('close', onClose);
        resolve();
      };

      this.ws.addEventListener('close', onClose);
      this.ws.close(); // initiate close
    });

    const timeout = new Promise<void>((_, reject) =>
      setTimeout(() => reject(new Error('WebSocket did not close within 2 seconds.')), 2000),
    );

    await Promise.race([closePromise, timeout]);
  }

  /** Generates a new unique operation ID */
  private getNextId(): string {
    return String(this.nextId++);
  }

  /** Handles all incoming WebSocket messages from the server */
  private handleMessage(event: MessageEvent) {
    const message = JSON.parse(event.data);
    const { id, payload, type } = message;

    if (type === 'connection_ack') {
      return;
    }

    const handlers = this.handlersMap.get(id);
    if (!handlers) return;

    switch (type) {
      case 'next':
        handlers.next?.(payload);
        break;
      case 'error':
        handlers.error?.(payload);
        break;
      case 'complete':
        handlers.complete?.(message as GraphQLCompleteMessage);
        this.handlersMap.delete(id);
        break;
    }
  }

  /**
   * Sends a WebSocket message to the server as a JSON object
   *
   * @param payload The payload of the message
   */
  send<T = unknown>(payload: T): void {
    this.ws.send(JSON.stringify(payload));
  }

  /**
   * Starts a GraphQL subscription and routes incoming results to the provided handlers.
   * @param query The subscription query string
   * @param handlers The object containing callbacks for subscription events
   * @param variables Optional subscription variables
   * @returns A cleanup function to cancel the subscription
   */
  subscribe<T>(
    query: string,
    handlers: SubscriptionHandlers<T>,
    variables?: Record<string, unknown>,
  ): () => void {
    const id = this.getNextId();
    const payload: GraphQLStartMessage = {
      id,
      type: 'start',
      payload: {
        query,
        variables,
      },
    };

    this.handlersMap.set(id, handlers as SubscriptionHandlers<unknown>);
    this.ws.send(JSON.stringify(payload));

    return () => {
      const stopMessage: GraphQLStopMessage = { id, type: 'stop' };
      this.ws.send(JSON.stringify(stopMessage));
      this.handlersMap.delete(id);
    };
  }

  /**
   * Returns the current socket ready state as a number.
   */
  getState() {
    return this.ws.readyState;
  }

  /**
   * Converts the ready state as a string.
   */
  static getStateName(state: number): string {
    return ['CONNECTING', 'OPEN', 'CLOSING', 'CLOSED'][state] ?? `UNKNOWN(${state})`;
  }

  /**
   * Subscribes to block events.
   *
   * This method subscribes to block events. This can be done providing an offset parameter
   * that contains a hash or a height. Alternatively, not providing any parameters assumes the
   * user is interested in the latest block.
   * Assuming the input paramters are valid and identify a block that exists, this will start a
   * streaming of blocks from that block.
   * - No blockOffset: start streaming from the latest block
   * - With hash: start streaming from the block with that hash
   * - With height: start streaming from the block with that height
   *
   * **Query Override Behavior:**
   * - If `queryOverride` is NOT provided: The function automatically selects the appropriate
   *   default query based on whether blockOffset is provided and handles all variable mapping
   * - If `queryOverride` IS provided: The function uses the provided query as-is, but still
   *   passes the blockOffset as variables (caller's responsibility to ensure the query matches
   *   the params provided)
   *
   * @param handlers - The handlers to receive the block events.
   * @param blockOffset - The block offset to subscribe to.
   * @param queryOverride - The query override to use.
   * @returns A function to unsubscribe from the block events.
   */
  subscribeToBlockEvents(
    handlers: SubscriptionHandlers<BlockSubscriptionResponse>,
    blockOffset?: BlockOffset,
    queryOverride?: string,
  ): () => void {
    const id = this.getNextId();

    const query =
      queryOverride ??
      (blockOffset
        ? BLOCKS_SUBSCRIPTION_FROM_BLOCK_BY_OFFSET
        : BLOCKS_SUBSCRIPTION_FROM_LATEST_BLOCK);
    const variables = blockOffset ? { OFFSET: blockOffset } : undefined;

    log.debug('Block subscription query:\n', query);
    log.debug('Block subscription variables:\n', variables);

    const payload: GraphQLStartMessage = {
      id,
      type: 'start',
      payload: {
        query,
        variables,
      },
    };

    log.debug('Block subscription full payload:\n', payload);

    // Fix type error by casting handlers to SubscriptionHandlers<unknown>
    this.handlersMap.set(id, handlers as SubscriptionHandlers<unknown>);
    this.ws.send(JSON.stringify(payload));

    return () => {
      const stopMessage: GraphQLStopMessage = { id, type: 'stop' };
      this.ws.send(JSON.stringify(stopMessage));
      this.handlersMap.delete(id);
    };
  }

  /**
   * Subscribes to unshielded transaction events for a specific address.
   *
   * This method subscribes to unshielded transaction events for the specified address:
   * - By default: receives all transactions involving the specified address
   * - With transactionId: receives transactions for the address starting from the specified ID
   *
   * **Query Override Behavior:**
   * - If `queryOverride` is NOT provided: The function automatically selects the appropriate
   *   default query based on whether transactionId is provided and handles all variable mapping
   * - If `queryOverride` IS provided: The function uses the provided query as-is, but still
   *   passes the address and transactionId as variables (caller's responsibility to ensure
   *   the query matches the params provided)
   *
   * @param handlers - Object containing callback functions for handling subscription events
   * @param params - Parameters specifying the address to subscribe to and optional transactionId
   * @param queryOverride - Optional custom GraphQL query. If provided, caller is responsible for it to match the params
   *
   * @returns A function that can be called to unsubscribe from the events
   */
  subscribeToUnshieldedTransactionEvents(
    handlers: SubscriptionHandlers<UnshieldedTxSubscriptionResponse>,
    params: UnshieldedTransactionSubscriptionParams,
    queryOverride?: string,
  ): () => void {
    const id = this.getNextId();

    // If queryOverride is provided, we use that, otherwise we use the default query
    // depending on the presence of the transactionId parameter
    const query =
      queryOverride ??
      (params.transactionId
        ? UNSHIELDED_TX_SUBSCRIPTION_BY_ADDRESS_AND_TRANSACTION_ID
        : UNSHIELDED_TX_SUBSCRIPTION_BY_ADDRESS);

    const variables: Record<string, unknown> = {
      ADDRESS: params.address,
      ...(params.transactionId !== undefined && { TRANSACTION_ID: params.transactionId }),
    };

    log.debug('Unshielded transaction subscription query:\n', query);
    log.debug('Unshielded transaction subscription variables:\n', variables);

    const payload: GraphQLStartMessage = {
      id,
      type: 'start',
      payload: {
        query,
        variables,
      },
    };

    log.debug('Unshielded transaction subscription full payload:\n', payload);

    // Type assertion to satisfy SubscriptionHandlers<unknown> requirement
    this.handlersMap.set(id, handlers as SubscriptionHandlers<unknown>);
    this.ws.send(JSON.stringify(payload));

    return () => {
      const stopMessage: GraphQLStopMessage = { id, type: 'stop' };
      this.ws.send(JSON.stringify(stopMessage));
      this.handlersMap.delete(id);
    };
  }

  /**
   * Subscribes to shielded transaction events for a specific wallet session.
   *
   * This method subscribes to shielded transaction events for the specified wallet session:
   * - Receives all shielded transactions and updates relevant to the wallet
   * - Includes both transaction updates and progress updates
   * - Requires an active wallet session (obtained via openWalletSession)
   *
   * **Query Override Behavior:**
   * - If `queryOverride` is NOT provided: The function uses the default shielded transaction
   *   subscription query and handles all variable mapping
   * - If `queryOverride` IS provided: The function uses the provided query as-is, but still
   *   passes the sessionId as a variable
   *
   * @param handlers - Object containing callback functions for handling subscription events
   * @param sessionId - The session ID obtained from openWalletSession
   * @param queryOverride - Optional custom GraphQL query. If provided, caller is responsible for it to match the sessionId
   *
   * @returns A function that can be called to unsubscribe from the events
   */
  subscribeToShieldedTransactionEvents(
    handlers: SubscriptionHandlers<ShieldedTxSubscriptionResponse>,
    sessionId: string,
    queryOverride?: string,
  ): () => void {
    const id = this.getNextId();

    // If queryOverride is provided, we use that, otherwise we use the default query
    const query = queryOverride ?? SHIELDED_TRANSACTION_SUBSCRIPTION_BY_SESSION_ID;
    const variables = {
      SESSION_ID: sessionId,
    };

    log.debug('Shielded transaction subscription query:\n', query);
    log.debug('Shielded transaction subscription variables:\n', variables);

    const payload: GraphQLStartMessage = {
      id,
      type: 'start',
      payload: {
        query,
        variables,
      },
    };

    log.debug('Shielded transaction subscription full payload:\n', payload);

    // Type assertion to fix type error
    this.handlersMap.set(id, handlers as SubscriptionHandlers<unknown>);
    this.ws.send(JSON.stringify(payload));

    return () => {
      const stopMessage: GraphQLStopMessage = { id, type: 'stop' };
      this.ws.send(JSON.stringify(stopMessage));
      this.handlersMap.delete(id);
    };
  }

  /**
   * Opens a wallet session for the given viewingKey.
   *
   * NOTE: If the viewing key matches an existing wallet with relevant transactions, subscriptions
   * will stream wallet transaction data. If the viewing key doesn't match any wallet or
   * the wallet doesn't have transactions, the stream won't provide any transaction data
   * when starting a subscription
   *
   * @param viewingKey - The viewing key for the wallet
   *
   * @returns A session ID in case of success
   */
  async openWalletSession(viewingKey: string): Promise<string> {
    const id = this.getNextId();

    const connectMutation = `mutation OpenWalletSession {
      connect (viewingKey: "${viewingKey}")
    }`;

    const payload: GraphQLStartMessage = {
      id,
      type: 'start',
      payload: {
        query: connectMutation,
      },
    };

    log.debug(connectMutation);
    log.debug(`${JSON.stringify(payload, null, 2)}`);
    this.ws.send(JSON.stringify(payload));

    return new Promise<string>((resolve, reject) => {
      const timeout = setTimeout(() => {
        cleanup();
        reject(new Error('Timeout while waiting for session response'));
      }, 5000); // Optional: timeout after 5s

      const handleMessage = (event: MessageEvent<string>) => {
        try {
          const message = JSON.parse(event.data);

          if (message.id !== id) return;

          switch (message.type) {
            case 'next':
              // Expecting session ID in payload
              const sessionId = message.payload?.data?.connect;
              if (typeof sessionId === 'string') {
                resolve(sessionId);
              } else {
                const errorMsg = `Session ID not found in response: ${JSON.stringify(message, null, 2)}`;
                log.error(errorMsg);
                reject(new Error(errorMsg));
              }
              break;

            case 'complete':
              // Server signals end of messages — just cleanup
              cleanup();
              break;

            case 'error':
              reject(new Error(`GraphQL error: ${JSON.stringify(message.payload)}`));
              break;
          }
        } catch (err) {
          reject(err);
        }
      };

      const cleanup = () => {
        clearTimeout(timeout);
        this.ws.removeEventListener('message', handleMessage);
        this.ws.send(JSON.stringify({ id, type: 'stop' }));
      };

      this.ws.addEventListener('message', handleMessage);
    });
  }

  /**
   * Closes a wallet session for the given session ID.
   *
   * @param sessionId - The session ID for an previously opened session
   *
   * @returns A Websocket close message
   */
  async closeWalletSession(sessionId: string): Promise<GraphQLCloseSessionMessage> {
    const id = this.getNextId();

    const disconnectMutation = `mutation CloseWalletSession {
      disconnect(sessionId: "${sessionId}")
    }`;

    const payload: GraphQLStartMessage = {
      id,
      type: 'start',
      payload: {
        query: disconnectMutation,
      },
    };

    log.debug(disconnectMutation);
    log.debug(`${JSON.stringify(payload, null, 2)}`);
    this.ws.send(JSON.stringify(payload));

    return new Promise<GraphQLCloseSessionMessage>((resolve, reject) => {
      const timeout = setTimeout(() => {
        cleanup();
        reject(new Error('Timeout while waiting for disconnection response'));
      }, 5000); // Optional timeout

      let closeAckMessage: GraphQLCloseSessionMessage;

      const handleMessage = (event: MessageEvent<GraphQLCloseSessionMessage>) => {
        try {
          const message = JSON.parse(event.data.toString());

          if (message.id !== id) return; // not my message!

          switch (message.type) {
            case 'next':
              // Confirm expected structure
              if (message.payload?.data?.disconnect !== null) {
                reject(new Error('Unexpected payload in disconnect response'));
              }
              closeAckMessage = message;
              break;

            case 'complete':
              // Server confirms it's done — success
              cleanup();
              resolve(closeAckMessage);
              break;

            case 'error':
              cleanup();
              reject(new Error(`GraphQL error: ${JSON.stringify(message.payload)}`));
              break;
          }
        } catch (err) {
          cleanup();
          reject(err);
        }
      };

      const cleanup = () => {
        clearTimeout(timeout);
        this.ws.removeEventListener('message', handleMessage);
        this.ws.send(JSON.stringify({ id, type: 'stop' }));
      };

      this.ws.addEventListener('message', handleMessage);
    });
  }
}
