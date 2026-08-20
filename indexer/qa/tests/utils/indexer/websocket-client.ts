// This file is part of midnightntwrk/midnight-indexer
// Copyright (C) Midnight Foundation
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
import { retry } from '@utils/retry-helper';
import type {
  Block,
  BlockOffset,
  DustLedgerEvent,
  DustGenerationsEvent,
  DustNullifierTransaction,
  ShieldedNullifierTransaction,
  ZswapLedgerEvent,
  ShieldedTransactionsEvent,
  UnshieldedTransactionEvent,
  ContractAction,
  ContractEvent,
  ContractEventFilter,
  GraphQLResponse,
  BridgeEvent,
  BridgeBalance,
  BridgePoolSummary,
} from './indexer-types';
import { CONTRACT_EVENTS_SUBSCRIPTION } from './graphql/contract-event-queries';
import {
  BLOCKS_SUBSCRIPTION_FROM_BLOCK_BY_OFFSET,
  BLOCKS_SUBSCRIPTION_FROM_LATEST_BLOCK,
  SHIELDED_TRANSACTION_SUBSCRIPTION_BY_SESSION_ID,
  UNSHIELDED_TX_SUBSCRIPTION_BY_ADDRESS,
  UNSHIELDED_TX_SUBSCRIPTION_BY_ADDRESS_AND_TRANSACTION_ID,
  CONTRACT_ACTIONS_SUBSCRIPTION_FROM_LATEST_BLOCK,
  CONTRACT_ACTIONS_SUBSCRIPTION_FROM_BLOCK_BY_OFFSET,
  DUST_LEDGER_EVENTS_SUBSCRIPTION_DEFAULT,
  DUST_LEDGER_EVENTS_SUBSCRIPTION_FROM_ID,
  DUST_GENERATIONS_SUBSCRIPTION,
  DUST_NULLIFIER_TRANSACTIONS_SUBSCRIPTION,
  SHIELDED_NULLIFIER_TRANSACTIONS_SUBSCRIPTION,
  ZSWAP_LEDGER_EVENTS_SUBSCRIPTION_DEFAULT,
  ZSWAP_LEDGER_EVENTS_SUBSCRIPTION_FROM_ID,
  BRIDGE_EVENTS_SUBSCRIPTION_DEFAULT,
  BRIDGE_EVENTS_SUBSCRIPTION_FROM,
  BRIDGE_BALANCE_SUBSCRIPTION,
  BRIDGE_POOL_UPDATES_SUBSCRIPTION,
} from './graphql/subscriptions';

export type BlockSubscriptionResponse = GraphQLResponse<{ blocks: Block }>;

export interface BridgePoolUpdate {
  newEvent: BridgeEvent | null;
  pool: BridgePoolSummary;
}

export type BridgePoolUpdateSubscriptionResponse = GraphQLResponse<{
  bridgePoolUpdates: BridgePoolUpdate;
}>;

export type UnshieldedTxSubscriptionResponse = GraphQLResponse<{
  unshieldedTransactions: UnshieldedTransactionEvent;
}>;

export type ShieldedTxSubscriptionResponse = GraphQLResponse<{
  shieldedTransactions: ShieldedTransactionsEvent;
}>;

export type ContractActionSubscriptionResponse = GraphQLResponse<{
  contractActions: ContractAction;
}>;

export type DustLedgerEventSubscriptionResponse = GraphQLResponse<{
  dustLedgerEvents: DustLedgerEvent;
}>;

export type DustGenerationsSubscriptionResponse = GraphQLResponse<{
  dustGenerations: DustGenerationsEvent;
}>;

export type BridgeEventSubscriptionResponse = GraphQLResponse<{ bridgeEvents: BridgeEvent }>;

export type BridgeBalanceSubscriptionResponse = GraphQLResponse<{ bridgeBalance: BridgeBalance }>;

export type DustNullifierTransactionSubscriptionResponse = GraphQLResponse<{
  dustNullifierTransactions: DustNullifierTransaction;
}>;

export type ShieldedNullifierTransactionSubscriptionResponse = GraphQLResponse<{
  shieldedNullifierTransactions: ShieldedNullifierTransaction;
}>;

export type ZswapLedgerEventSubscriptionResponse = GraphQLResponse<{
  zswapLedgerEvents: ZswapLedgerEvent;
}>;

export type ContractEventSubscriptionResponse = GraphQLResponse<{
  contractEvents: ContractEvent;
}>;

/**
 * Handlers used to respond to incoming GraphQL subscription messages.
 */
export interface SubscriptionHandlers<T> {
  /** Called when a new payload is received (optional: the client invokes it as `next?.()`) */
  next?: (value: T) => void;

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
  /** The active WebSocket connection; null until connectionInit() succeeds */
  private ws: WebSocket | null = null;

  /** The endpoint where to send graphql subscriptions */
  private readonly graphqlAPIEndpoint: string;

  /** WebSocket URL; set in constructor */
  private readonly targetUrl: string;

  /** Counter to generate unique operation IDs */
  private nextId = 0;

  /** Maps operation IDs to their registered event handlers */
  private handlersMap = new Map<string, SubscriptionHandlers<unknown>>();

  /**
   * Lightweight constructor: only stores the target URL. The actual connection
   * is created in connectionInit().
   */
  constructor() {
    const apiVersion = process.env.INDEXER_API_VERSION?.trim() || 'v4';
    this.graphqlAPIEndpoint = `/api/${apiVersion}/graphql/ws`;
    this.targetUrl = env.getIndexerWebsocketBaseURL() + this.graphqlAPIEndpoint;
  }

  /** Attaches common handlers to the current WebSocket and returns it */
  private attachWsHandlers(ws: WebSocket): WebSocket {
    ws.onmessage = this.handleMessage.bind(this);
    ws.onerror = (event: { type: string; target: unknown; message?: string }) => {
      const err = typeof event.message === 'string' ? event.message : event.type;
      log.warn(
        `WebSocket onerror: type=${event.type}, error=${err}, target=${String(event.target)}`,
      );
    };
    ws.onclose = (event: { code: number; reason: string; wasClean: boolean }) => {
      log.warn(
        `WebSocket onclose: code=${event.code}, reason=${event.reason || '(no reason)'}, wasClean=${event.wasClean}`,
      );
    };
    return ws;
  }

  /** Creates a new WebSocket, attaches handlers, and assigns to this.ws */
  private createWebSocket(): void {
    // A previous failed connection attempt may have left a socket in CONNECTING
    // or OPEN. Detach handlers and close it so a retry does not leak orphan
    // sockets that keep handshaking against the gateway and add to load.
    if (this.ws !== null) {
      const stale = this.ws;
      stale.onmessage = null;
      stale.onerror = null;
      stale.onclose = null;
      try {
        stale.close();
      } catch (error) {
        log.debug(`Ignoring close() error during stale-socket cleanup: ${String(error)}`);
      }
      this.ws = null;
    }
    this.ws = this.attachWsHandlers(new WebSocket(this.targetUrl, 'graphql-transport-ws'));
  }

  /** Returns the active WebSocket; throws if connectionInit() has not succeeded. */
  private getWs(): WebSocket {
    if (this.ws == null) {
      throw new Error(
        'WebSocket not connected. Call connectionInit() and await it before using the client.',
      );
    }
    return this.ws;
  }

  /**
   * Creates the WebSocket, waits until it is OPEN (or aborts on CLOSED/timeout),
   * then sends the GraphQL connection_init and waits for connection_ack.
   * On connection failure (CLOSED or timeout), retries by creating a new WebSocket.
   */
  async connectionInit(payload?: Record<string, unknown>): Promise<void> {
    const waitMaxAttempts = 10; // 10 × 500ms = 5s max wait per connection attempt
    const waitDelayMs = 500;

    await retry(
      async () => {
        this.createWebSocket();
        const ws = this.ws!;

        for (let attempt = 1; attempt <= waitMaxAttempts; attempt++) {
          const state = ws.readyState;
          const stateName = IndexerWsClient.getStateName(state);
          log.debug(`WebSocket state (attempt ${attempt}/${waitMaxAttempts}): ${stateName}`);

          if (state === WebSocket.OPEN) break;
          if (state === WebSocket.CLOSED) {
            throw new Error(
              `WebSocket connection failed (state: ${stateName}). Will retry with a new socket.`,
            );
          }
          if (attempt === waitMaxAttempts) {
            throw new Error(
              `Failed after ${waitMaxAttempts} attempts for websocket connection ready check. Last state: ${stateName}`,
            );
          }
          await new Promise((resolve) => setTimeout(resolve, waitDelayMs));
        }

        const init: GraphQLConnectionInitMessage = {
          type: 'connection_init',
          payload, // Payload is optional and can be used for negotiation
        };

        const response: Promise<{ type: string }> = new Promise((resolve, reject) => {
          const timeout = setTimeout(() => {
            ws.removeEventListener('message', onMessage);
            reject(new Error('Timed out waiting for connection_ack'));
          }, 2000);

          const onMessage = (event: MessageEvent) => {
            const message = JSON.parse(event.data);
            if (message.type === 'connection_ack') {
              clearTimeout(timeout);
              ws.removeEventListener('message', onMessage);
              resolve(message);
            }
          };

          ws.addEventListener('message', onMessage);
          ws.send(JSON.stringify(init));
        });

        assert((await response).type == 'connection_ack');
      },
      {
        maxRetries: 2,
        delayMs: 1000,
        retryLabel: 'websocket connection',
      },
    );
  }

  /**
   * Terminates the underlying WebSocket connection to the indexer.
   */
  async connectionClose(): Promise<void> {
    if (this.ws === null) return;

    // Capture the socket reference so a late onClose firing after
    // `this.ws = null` (set below or on a parallel teardown path) does
    // not dereference null — the original source of the
    // "Cannot read properties of null (reading 'removeEventListener')"
    // uncaught exception under parallel test execution.
    const ws = this.ws;

    const closePromise = new Promise<void>((resolve) => {
      const onClose = () => {
        ws.removeEventListener('close', onClose);
        resolve();
      };

      ws.addEventListener('close', onClose);
      ws.close(); // initiate close
    });

    const timeoutPromise = new Promise<void>((resolve) =>
      setTimeout(() => {
        log.warn('WebSocket did not close within timeout; continuing');
        resolve();
      }, 2000),
    );

    // Either the connection gets closed or we timeout
    await Promise.race([closePromise, timeoutPromise]);
    this.ws = null;
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
        // async-graphql 7.2 emits subscription errors (e.g. quota rejections)
        // as `{type: 'next', payload: {data: null, errors: [...]}}` rather than
        // the legacy `{type: 'error', payload: [...]}`. Both shapes are valid
        // in graphql-transport-ws; route errors-inside-next to the error
        // handler so tests don't silently treat rejections as successes.
        //
        // Consequence: `handlers.error` receives an `Error` from this branch
        // and the raw payload (string or `Array<GraphQLError>`) from the
        // legacy `case 'error'` branch below. Helpers that need the bare
        // server message must coerce both — use
        // `extractSubscriptionErrorMessage()` from `./subscription-error`.
        const errors = (payload as { errors?: Array<{ message?: string }> } | null)?.errors;
        if (Array.isArray(errors) && errors.length > 0) {
          const message = errors[0]?.message ?? 'GraphQL subscription error';
          handlers.error?.(new Error(message));
        } else {
          handlers.next?.(payload);
        }
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
    this.getWs().send(JSON.stringify(payload));
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
    this.getWs().send(JSON.stringify(payload));

    return () => {
      const stopMessage: GraphQLStopMessage = { id, type: 'stop' };
      this.getWs().send(JSON.stringify(stopMessage));
      this.handlersMap.delete(id);
    };
  }

  /**
   * Subscribes to public contract events matching a filter.
   *
   * The stream starts from the later of `id` (event-id resumption cursor) and
   * `filter.fromBlock`; if `filter.toBlock` is set the stream terminates once the
   * chain reaches that block (the server sends a `complete` message).
   *
   * @param handlers - Object containing callback functions for subscription events
   * @param filter - The contract event filter (contractAddress is required)
   * @param id - Optional event-id resumption cursor
   * @param queryOverride - Optional custom GraphQL subscription. If provided, the
   *                        caller is responsible for matching the variables
   *
   * @returns An object with subscription ID and unsubscribe function
   */
  subscribeToContractEvents(
    handlers: SubscriptionHandlers<ContractEventSubscriptionResponse>,
    filter: ContractEventFilter,
    id?: number,
    queryOverride?: string,
  ): { unsubscribe: () => void; id: string } {
    const subscriptionId = this.getNextId();

    const query = queryOverride ?? CONTRACT_EVENTS_SUBSCRIPTION;
    const variables: Record<string, unknown> = {
      FILTER: filter,
      ...(id !== undefined && { ID: id }),
    };

    log.debug(`Contract events subscription query:\n${query}`);
    log.debug(`Contract events subscription variables:\n${JSON.stringify(variables, null, 2)}`);

    const payload: GraphQLStartMessage = {
      id: subscriptionId,
      type: 'start',
      payload: {
        query,
        variables,
      },
    };

    this.handlersMap.set(subscriptionId, handlers as SubscriptionHandlers<unknown>);
    this.getWs().send(JSON.stringify(payload));

    return {
      id: subscriptionId,
      unsubscribe: () => {
        const stopMessage: GraphQLStopMessage = { id: subscriptionId, type: 'stop' };
        this.getWs().send(JSON.stringify(stopMessage));
        this.handlersMap.delete(subscriptionId);
      },
    };
  }

  /**
   * Returns the current socket ready state as a number.
   */
  getState() {
    return this.getWs().readyState;
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

    log.debug(`Block subscription query:\n${query}`);
    log.debug(`Block subscription variables:\n${JSON.stringify(variables, null, 2)}`);

    const payload: GraphQLStartMessage = {
      id,
      type: 'start',
      payload: {
        query,
        variables,
      },
    };

    log.debug(`Block subscription full payload:\n${JSON.stringify(payload, null, 2)}`);

    // Fix type error by casting handlers to SubscriptionHandlers<unknown>
    this.handlersMap.set(id, handlers as SubscriptionHandlers<unknown>);
    this.getWs().send(JSON.stringify(payload));

    return () => {
      const stopMessage: GraphQLStopMessage = { id, type: 'stop' };
      this.getWs().send(JSON.stringify(stopMessage));
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

    log.debug(`Unshielded transaction subscription query:\n${query}`);
    log.debug(
      `Unshielded transaction subscription variables:\n${JSON.stringify(variables, null, 2)}`,
    );

    const payload: GraphQLStartMessage = {
      id,
      type: 'start',
      payload: {
        query,
        variables,
      },
    };

    log.debug(
      `Unshielded transaction subscription full payload:\n${JSON.stringify(payload, null, 2)}`,
    );

    // Type assertion to satisfy SubscriptionHandlers<unknown> requirement
    this.handlersMap.set(id, handlers as SubscriptionHandlers<unknown>);
    this.getWs().send(JSON.stringify(payload));

    return () => {
      const stopMessage: GraphQLStopMessage = { id, type: 'stop' };
      this.getWs().send(JSON.stringify(stopMessage));
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

    log.debug(`Shielded transaction subscription query:\n${query}`);
    log.debug(
      `Shielded transaction subscription variables:\n${JSON.stringify(variables, null, 2)}`,
    );

    const payload: GraphQLStartMessage = {
      id,
      type: 'start',
      payload: {
        query,
        variables,
      },
    };

    log.debug(
      `Shielded transaction subscription full payload:\n${JSON.stringify(payload, null, 2)}`,
    );

    // Type assertion to fix type error
    this.handlersMap.set(id, handlers as SubscriptionHandlers<unknown>);
    this.getWs().send(JSON.stringify(payload));

    return () => {
      const stopMessage: GraphQLStopMessage = { id, type: 'stop' };
      this.getWs().send(JSON.stringify(stopMessage));
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
   * @param options - Optional `ConnectOptions`. `startIndex` lets the wallet skip
   *   historical transaction scanning by telling the indexer to begin scanning from
   *   the given transaction index (see midnight-indexer#984 / PR #1039).
   *
   * @returns A session ID in case of success
   */
  async openWalletSession(viewingKey: string, options?: { startIndex?: number }): Promise<string> {
    const id = this.getNextId();

    const optionsClause =
      options?.startIndex !== undefined ? `, options: { startIndex: ${options.startIndex} }` : '';
    const connectMutation = `mutation OpenWalletSession {
      connect (viewingKey: "${viewingKey}"${optionsClause})
    }`;

    const payload: GraphQLStartMessage = {
      id,
      type: 'start',
      payload: {
        query: connectMutation,
      },
    };

    // Capture the socket once. If `this.ws` is reassigned during the
    // session lifetime (reconnect / teardown), the deferred cleanup still
    // targets the socket the listener was actually attached to.
    const ws = this.getWs();

    log.debug(connectMutation);
    log.debug(`${JSON.stringify(payload, null, 2)}`);
    ws.send(JSON.stringify(payload));

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
        ws.removeEventListener('message', handleMessage);
        try {
          ws.send(JSON.stringify({ id, type: 'stop' }));
        } catch (error) {
          log.debug(
            `Ignoring send() error during session cleanup (socket likely CLOSED): ${String(error)}`,
          );
        }
      };

      ws.addEventListener('message', handleMessage);
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

    // Capture the socket once — see openWalletSession for rationale.
    const ws = this.getWs();

    log.debug(disconnectMutation);
    log.debug(`${JSON.stringify(payload, null, 2)}`);
    ws.send(JSON.stringify(payload));

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
        ws.removeEventListener('message', handleMessage);
        try {
          ws.send(JSON.stringify({ id, type: 'stop' }));
        } catch (error) {
          log.debug(
            `Ignoring send() error during session cleanup (socket likely CLOSED): ${String(error)}`,
          );
        }
      };

      ws.addEventListener('message', handleMessage);
    });
  }

  /**
   * Subscribes to contract action events.
   *
   * This method subscribes to contract action events for a specific address.
   * This can be done providing a blockOffset parameter or without parameters
   * to start from the latest block.
   *
   * - No blockOffset: start streaming from the latest block
   * - With blockOffset: start streaming from the specified block
   *
   * **Query Override Behavior:**
   * - If `queryOverride` is NOT provided: The function automatically selects the appropriate
   *   default query based on whether blockOffset is provided and handles all variable mapping
   * - If `queryOverride` IS provided: The function uses the provided query as-is, but still
   *   passes the address and blockOffset as variables (caller's responsibility to ensure
   *   the query matches the params provided)
   *
   * @param handlers - The handlers to receive the contract action events.
   * @param address - The contract address to subscribe to.
   * @param blockOffset - The block offset to subscribe to.
   * @param queryOverride - The query override to use.
   * @returns A function to unsubscribe from the contract action events.
   */
  subscribeToContractActionEvents(
    handlers: SubscriptionHandlers<ContractActionSubscriptionResponse>,
    address: string,
    blockOffset?: BlockOffset,
    queryOverride?: string,
  ): () => void {
    const id = this.getNextId();

    const query =
      queryOverride ??
      (blockOffset
        ? CONTRACT_ACTIONS_SUBSCRIPTION_FROM_BLOCK_BY_OFFSET
        : CONTRACT_ACTIONS_SUBSCRIPTION_FROM_LATEST_BLOCK);
    const variables = {
      ADDRESS: address,
      ...(blockOffset && { OFFSET: blockOffset }),
    };

    log.debug(`Contract action subscription query:\n${query}`);
    log.debug(`Contract action subscription variables:\n${JSON.stringify(variables, null, 2)}`);

    const payload: GraphQLStartMessage = {
      id,
      type: 'start',
      payload: {
        query,
        variables,
      },
    };

    log.debug(`Contract action subscription full payload:\n${JSON.stringify(payload, null, 2)}`);

    this.handlersMap.set(id, handlers as SubscriptionHandlers<unknown>);
    this.getWs().send(JSON.stringify(payload));

    return () => {
      const stopMessage: GraphQLStopMessage = { id, type: 'stop' };
      this.getWs().send(JSON.stringify(stopMessage));
      this.handlersMap.delete(id);
    };
  }

  /**
   * Subscribes to dust ledger events.
   *
   * This method starts a GraphQL subscription that streams DustLedgerEvent updates from the indexer:
   *
   * - Without an offset: streams all new dust events from the latest position.
   * - With an offset (id): streams dust events starting from that event ID (e.g., previousMaxId + 1 to receive only new events).
   *
   * The correct GraphQL query is selected automatically unless a custom
   * queryOverride is provided. All incoming messages are routed to the given
   * handlers, and the returned function can be used to unsubscribe.
   *
   * @param handlers - Callback functions for handling incoming dust events
   * @param offset - Optional object containing an event ID to start from
   * @param queryOverride - Optional custom GraphQL subscription query
   *
   * @returns A function that unsubscribes from the dust event stream
   */
  subscribeToDustLedgerEvents(
    handlers: SubscriptionHandlers<DustLedgerEventSubscriptionResponse>,
    offset?: { id: number },
    queryOverride?: string,
  ): { unsubscribe: () => void; id: string } {
    const hasOffset = offset !== undefined && offset.id !== undefined;

    let query = queryOverride;
    if (!query) {
      query = hasOffset
        ? DUST_LEDGER_EVENTS_SUBSCRIPTION_FROM_ID
        : DUST_LEDGER_EVENTS_SUBSCRIPTION_DEFAULT;
    }
    const variables = hasOffset ? { id: offset.id } : undefined;

    const subscriptionId = this.getNextId();

    const payload: GraphQLStartMessage = {
      id: subscriptionId,
      type: 'start',
      payload: {
        query,
        variables,
      },
    };

    log.debug(`Dust Ledger Events payload:\n${JSON.stringify(payload, null, 2)}`);

    this.handlersMap.set(subscriptionId, handlers as SubscriptionHandlers<unknown>);
    this.getWs().send(JSON.stringify(payload));

    return {
      id: subscriptionId,
      unsubscribe: () => {
        const stopMessage: GraphQLStopMessage = {
          id: subscriptionId,
          type: 'stop',
        };
        this.getWs().send(JSON.stringify(stopMessage));
        this.handlersMap.delete(subscriptionId);
      },
    };
  }

  /**
   * Subscribes to zswap ledger events.
   *
   * This method starts a GraphQL subscription that streams ZswapLedgerEvent updates from the indexer:
   *
   * - Without an offset: streams historical zswap events from the beginning.
   * - With an offset (id): streams zswap events starting from that event ID.
   *
   * The correct GraphQL query is selected automatically unless a custom
   * queryOverride is provided. All incoming messages are routed to the given
   * handlers, and the returned function can be used to unsubscribe.
   *
   * Note: Unlike dust ledger events, zswap event IDs may not be strictly sequential
   * and events may be delivered in different orders depending on the offset.
   *
   * @param handlers - Callback functions for handling incoming zswap events
   * @param offset - Optional object containing an event ID to start from
   * @param queryOverride - Optional custom GraphQL subscription query
   *
   * @returns An object with subscription ID and unsubscribe function
   */
  subscribeToZswapLedgerEvents(
    handlers: SubscriptionHandlers<ZswapLedgerEventSubscriptionResponse>,
    offset?: { id: number },
    queryOverride?: string,
  ): { unsubscribe: () => void; id: string } {
    const hasOffset = offset !== undefined && offset.id !== undefined;

    let query = queryOverride;
    if (!query) {
      query = hasOffset
        ? ZSWAP_LEDGER_EVENTS_SUBSCRIPTION_FROM_ID
        : ZSWAP_LEDGER_EVENTS_SUBSCRIPTION_DEFAULT;
    }
    const variables = hasOffset ? { id: offset.id } : undefined;

    const subscriptionId = this.getNextId();

    const payload: GraphQLStartMessage = {
      id: subscriptionId,
      type: 'start',
      payload: {
        query,
        variables,
      },
    };

    log.debug(`Zswap Ledger Events payload:\n${JSON.stringify(payload, null, 2)}`);

    this.handlersMap.set(subscriptionId, handlers as SubscriptionHandlers<unknown>);
    this.getWs().send(JSON.stringify(payload));

    return {
      id: subscriptionId,
      unsubscribe: () => {
        const stopMessage: GraphQLStopMessage = {
          id: subscriptionId,
          type: 'stop',
        };
        this.getWs().send(JSON.stringify(stopMessage));
        this.handlersMap.delete(subscriptionId);
      },
    };
  }

  /**
   * Subscribes to a dust address's generations as a consistent snapshot at a block hash.
   *
   * Owned dtime updates after the cutoff height are issued first, then owned generation
   * entries interleaved with collapsed Merkle tree updates for the non-owned gaps, all
   * served at the given block's state. The subscription completes once emitted.
   *
   * @param handlers - Callback functions for handling incoming dust generation events
   * @param dustAddress - Bech32m-encoded dust address to subscribe for
   * @param blockHash - Hex-encoded block hash the generation snapshot is pinned to
   * @param dtimeCutoffHeight - Block height after which owned dtime updates are replayed
   *                            (pass 0 to replay all)
   * @param queryOverride - Optional custom GraphQL subscription query
   *
   * @returns An object with subscription ID and unsubscribe function
   */
  subscribeToDustGenerations(
    handlers: SubscriptionHandlers<DustGenerationsSubscriptionResponse>,
    dustAddress: string,
    blockHash: string,
    dtimeCutoffHeight: number,
    queryOverride?: string,
  ): { unsubscribe: () => void; id: string } {
    const query = queryOverride || DUST_GENERATIONS_SUBSCRIPTION;
    const variables = { dustAddress, blockHash, dtimeCutoffHeight };

    const subscriptionId = this.getNextId();

    const payload: GraphQLStartMessage = {
      id: subscriptionId,
      type: 'start',
      payload: {
        query,
        variables,
      },
    };

    log.debug(`Dust Generations payload:\n${JSON.stringify(payload, null, 2)}`);

    this.handlersMap.set(subscriptionId, handlers as SubscriptionHandlers<unknown>);
    this.getWs().send(JSON.stringify(payload));

    return {
      id: subscriptionId,
      unsubscribe: () => {
        const stopMessage: GraphQLStopMessage = {
          id: subscriptionId,
          type: 'stop',
        };
        this.getWs().send(JSON.stringify(stopMessage));
        this.handlersMap.delete(subscriptionId);
      },
    };
  }

  /**
   * Subscribes to transactions containing dust nullifiers matching the provided prefixes.
   *
   * Returns transaction and block references for wallet to fetch full data.
   * If `toBlock` is specified, the subscription finishes after reaching that block.
   *
   * @param handlers - Callback functions for handling incoming nullifier transaction events
   * @param nullifierLeBytesPrefixes - Array of hex-encoded 32-byte little-endian nullifier prefixes to match
   * @param fromBlock - Optional starting block height
   * @param toBlock - Optional ending block height (subscription finishes after this)
   * @param queryOverride - Optional custom GraphQL subscription query
   *
   * @returns An object with subscription ID and unsubscribe function
   */
  subscribeToDustNullifierTransactions(
    handlers: SubscriptionHandlers<DustNullifierTransactionSubscriptionResponse>,
    nullifierLeBytesPrefixes: string[],
    fromBlock?: number,
    toBlock?: number,
    queryOverride?: string,
  ): { unsubscribe: () => void; id: string } {
    const query = queryOverride || DUST_NULLIFIER_TRANSACTIONS_SUBSCRIPTION;
    const variables = { nullifierLeBytesPrefixes, fromBlock, toBlock };

    const subscriptionId = this.getNextId();

    const payload: GraphQLStartMessage = {
      id: subscriptionId,
      type: 'start',
      payload: {
        query,
        variables,
      },
    };

    log.debug(`Dust Nullifier Transactions payload:\n${JSON.stringify(payload, null, 2)}`);

    this.handlersMap.set(subscriptionId, handlers as SubscriptionHandlers<unknown>);
    this.getWs().send(JSON.stringify(payload));

    return {
      id: subscriptionId,
      unsubscribe: () => {
        const stopMessage: GraphQLStopMessage = {
          id: subscriptionId,
          type: 'stop',
        };
        this.getWs().send(JSON.stringify(stopMessage));
        this.handlersMap.delete(subscriptionId);
      },
    };
  }

  /**
   * Subscribes to transactions containing shielded (Zswap) nullifiers matching
   * the provided prefixes.
   *
   * Mirrors `subscribeToDustNullifierTransactions` but operates on the shielded
   * nullifier surface. Returns transaction and block references the wallet can
   * use to fetch the full transaction. If `toBlock` is supplied, the
   * subscription completes once that block is reached.
   *
   * @param handlers - Callback functions for handling incoming nullifier
   *   transaction events
   * @param nullifierPrefixes - Array of hex-encoded nullifier prefixes to match
   * @param fromBlock - Optional starting block height
   * @param toBlock - Optional ending block height (subscription finishes after
   *   this)
   * @param queryOverride - Optional custom GraphQL subscription query
   *
   * @returns An object with subscription ID and unsubscribe function
   */
  subscribeToShieldedNullifierTransactions(
    handlers: SubscriptionHandlers<ShieldedNullifierTransactionSubscriptionResponse>,
    nullifierPrefixes: string[],
    fromBlock?: number,
    toBlock?: number,
    queryOverride?: string,
  ): { unsubscribe: () => void; id: string } {
    const query = queryOverride || SHIELDED_NULLIFIER_TRANSACTIONS_SUBSCRIPTION;
    const variables = { nullifierPrefixes, fromBlock, toBlock };

    const subscriptionId = this.getNextId();

    const payload: GraphQLStartMessage = {
      id: subscriptionId,
      type: 'start',
      payload: {
        query,
        variables,
      },
    };

    log.debug(`Shielded Nullifier Transactions payload:\n${JSON.stringify(payload, null, 2)}`);

    this.handlersMap.set(subscriptionId, handlers as SubscriptionHandlers<unknown>);
    this.getWs().send(JSON.stringify(payload));

    return {
      id: subscriptionId,
      unsubscribe: () => {
        const stopMessage: GraphQLStopMessage = {
          id: subscriptionId,
          type: 'stop',
        };
        this.getWs().send(JSON.stringify(stopMessage));
        this.handlersMap.delete(subscriptionId);
      },
    };
  }

  /**
   * Subscribes to c2m-bridge events (#942).
   *
   * - Without `from`: streams historical events from the beginning, then live-tails.
   * - With `from`: replays events with id greater than the cursor, then live-tails.
   *
   * There is no completion sentinel; callers terminate on their own condition.
   *
   * @param handlers - Callbacks for incoming bridge event messages
   * @param opts - Optional `from` event-id cursor, `recipient` and `variant` filters
   * @param queryOverride - Optional custom GraphQL subscription query
   * @returns An object with subscription ID and unsubscribe function
   */
  subscribeToBridgeEvents(
    handlers: SubscriptionHandlers<BridgeEventSubscriptionResponse>,
    opts: { from?: number; recipient?: string; variant?: string } = {},
    queryOverride?: string,
  ): { unsubscribe: () => void; id: string } {
    const hasFrom = opts.from !== undefined;
    const query =
      queryOverride ||
      (hasFrom ? BRIDGE_EVENTS_SUBSCRIPTION_FROM : BRIDGE_EVENTS_SUBSCRIPTION_DEFAULT);
    const variables = {
      FROM: opts.from,
      RECIPIENT: opts.recipient,
      VARIANT: opts.variant,
    };

    const subscriptionId = this.getNextId();

    const payload: GraphQLStartMessage = {
      id: subscriptionId,
      type: 'start',
      payload: { query, variables },
    };

    log.debug(`Bridge Events payload:\n${JSON.stringify(payload, null, 2)}`);

    this.handlersMap.set(subscriptionId, handlers as SubscriptionHandlers<unknown>);
    this.getWs().send(JSON.stringify(payload));

    return {
      id: subscriptionId,
      unsubscribe: () => {
        const stopMessage: GraphQLStopMessage = { id: subscriptionId, type: 'stop' };
        this.getWs().send(JSON.stringify(stopMessage));
        this.handlersMap.delete(subscriptionId);
      },
    };
  }

  /**
   * Subscribes to a c2m-bridge address balance (#942). Emits the current balance
   * immediately on connect, then re-emits on every relevant event for the address.
   *
   * @param handlers - Callbacks for incoming bridge balance messages
   * @param address - The hex-encoded address to observe
   * @param queryOverride - Optional custom GraphQL subscription query
   * @returns An object with subscription ID and unsubscribe function
   */
  subscribeToBridgeBalance(
    handlers: SubscriptionHandlers<BridgeBalanceSubscriptionResponse>,
    address: string,
    queryOverride?: string,
  ): { unsubscribe: () => void; id: string } {
    const query = queryOverride || BRIDGE_BALANCE_SUBSCRIPTION;
    const variables = { ADDRESS: address };

    const subscriptionId = this.getNextId();

    const payload: GraphQLStartMessage = {
      id: subscriptionId,
      type: 'start',
      payload: { query, variables },
    };

    log.debug(`Bridge Balance payload:\n${JSON.stringify(payload, null, 2)}`);

    this.handlersMap.set(subscriptionId, handlers as SubscriptionHandlers<unknown>);
    this.getWs().send(JSON.stringify(payload));

    return {
      id: subscriptionId,
      unsubscribe: () => {
        const stopMessage: GraphQLStopMessage = { id: subscriptionId, type: 'stop' };
        this.getWs().send(JSON.stringify(stopMessage));
        this.handlersMap.delete(subscriptionId);
      },
    };
  }

  /**
   * Subscribes to c2m-bridge pool updates (#944). Emits an initial snapshot on
   * connect (newEvent = null, pool = current summary), then a refreshed summary
   * paired with each new pool-affecting event. There is no completion sentinel.
   *
   * @param handlers - Callbacks for incoming pool update messages
   * @param queryOverride - Optional custom GraphQL subscription query
   * @returns An object with subscription ID and unsubscribe function
   */
  subscribeToBridgePoolUpdates(
    handlers: SubscriptionHandlers<BridgePoolUpdateSubscriptionResponse>,
    queryOverride?: string,
  ): { unsubscribe: () => void; id: string } {
    const query = queryOverride || BRIDGE_POOL_UPDATES_SUBSCRIPTION;

    const subscriptionId = this.getNextId();

    const payload: GraphQLStartMessage = {
      id: subscriptionId,
      type: 'start',
      payload: { query },
    };

    log.debug(`Bridge Pool Updates payload:\n${JSON.stringify(payload, null, 2)}`);

    this.handlersMap.set(subscriptionId, handlers as SubscriptionHandlers<unknown>);
    this.getWs().send(JSON.stringify(payload));

    return {
      id: subscriptionId,
      unsubscribe: () => {
        const stopMessage: GraphQLStopMessage = { id: subscriptionId, type: 'stop' };
        this.getWs().send(JSON.stringify(stopMessage));
        this.handlersMap.delete(subscriptionId);
      },
    };
  }
}
