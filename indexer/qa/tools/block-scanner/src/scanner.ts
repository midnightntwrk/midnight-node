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

import fs from "fs";
import path from "path";
import { TARGET_ENV, INDEXER_WS_URL, INDEXER_HTTP_URL } from "./env.js";
import { Block, RegularTransaction } from "./indexer-types.js";
import { updateTestDataFiles } from "./test-data-handler.js";

// Configuration constants
const CONFIG = {
  TMP_DIR: "tmp_scan",
  STATS_DIR: "stats",
  TIMEOUT_MS: 600_000,
  QUERY_TIMEOUT_MS: 10_000,
  CONNECTION_TIMEOUT_MS: 2_000,
  CONNECTION_MAX_RETRIES: 4,
  CONNECTION_RETRY_BASE_DELAY_MS: 2_000,
  WS_PROTOCOL: "graphql-transport-ws",
  SPINNER_UPDATE_INTERVAL_MS: 100,
  PROGRESS_UPDATE_INTERVAL: 100,
} as const;

type Config = typeof CONFIG;

// Parse command line arguments
const args = Bun.argv.slice(2);
const testDataFolder = args[0]; // First argument is the test data folder path

const handlersMap: Map<string, SubscriptionHandlers<any>> = new Map<
  string,
  SubscriptionHandlers<any>
>();

/**
 * Maps the Graphql response payload
 */
export interface SubscriptionPayload<T> {
  data?: T;
  errors?: any[];
}

interface BlockOffset {
  hash?: string;
  height?: number;
}

/**
 * Persisted scan statistics per environment (stored in stats/${TARGET_ENV}_stats.json).
 * Used for resume and cumulative totals across runs.
 */
interface ScanStats {
  lastScannedBlockHeight: number;
  totalBlocksScanned: number;
  blocksWithTransactions: number;
  totalTransactionsFound: number;
  contractActionsFound: number;
  totalScanDurationSeconds: number;
  lastUpdated: string;
}

function getStatsPath(): string {
  return path.join(CONFIG.STATS_DIR, `${TARGET_ENV}_stats.json`);
}

function ensureStatsDir(): void {
  if (!fs.existsSync(CONFIG.STATS_DIR)) fs.mkdirSync(CONFIG.STATS_DIR);
}

function loadStats(): ScanStats | null {
  const statsPath = getStatsPath();
  if (!fs.existsSync(statsPath)) return null;
  try {
    const raw = fs.readFileSync(statsPath, "utf-8");
    const data = JSON.parse(raw) as unknown;
    if (
      data &&
      typeof data === "object" &&
      "lastScannedBlockHeight" in data &&
      typeof (data as ScanStats).lastScannedBlockHeight === "number"
    ) {
      const s = data as ScanStats;
      return {
        lastScannedBlockHeight: s.lastScannedBlockHeight,
        totalBlocksScanned: Number(s.totalBlocksScanned) || 0,
        blocksWithTransactions: Number(s.blocksWithTransactions) || 0,
        totalTransactionsFound: Number(s.totalTransactionsFound) || 0,
        contractActionsFound: Number(s.contractActionsFound) || 0,
        totalScanDurationSeconds: Number(s.totalScanDurationSeconds) || 0,
        lastUpdated: typeof s.lastUpdated === "string" ? s.lastUpdated : "",
      };
    }
  } catch {
    // Invalid or missing file: treat as no previous stats
  }
  return null;
}

function writeStats(stats: ScanStats): void {
  ensureStatsDir();
  fs.writeFileSync(getStatsPath(), JSON.stringify(stats, null, 2), "utf-8");
}

/**
 * Handlers used to respond to incoming GraphQL subscription messages.
 */
export interface SubscriptionHandlers<T> {
  /** Called when a new payload is received */
  next: (value: T) => void;

  /** Called when an error is received (note this is actually obsolete) */
  error?: (err: any) => void;

  /** Called when the subscription completes */
  complete?: () => void;
}

if (!fs.existsSync(CONFIG.TMP_DIR)) fs.mkdirSync(CONFIG.TMP_DIR);

/**
 * Cleanup function for WebSocket and handlers
 */
async function cleanupResources(
  ws: WebSocket,
  handlersMap: Map<string, any>,
): Promise<void> {
  handlersMap.clear();
  ws.close();
  await new Promise<void>((resolve) => {
    ws.addEventListener("close", () => resolve());
  });
}

/**
 * Subscription manager for handling block scanning with progress tracking
 */
class BlockScanManager {
  private reachedTarget = false;
  private subscriptionError: Error | null = null;
  private targetHeight: number;
  private reachedResolve?: () => void;
  private errorResolve?: () => void;

  constructor(
    private handlersMap: Map<string, SubscriptionHandlers<any>>,
    private originalHandler: SubscriptionHandlers<
      SubscriptionPayload<{ blocks: Block }>
    >,
    private spinner: ReturnType<typeof createSpinner>,
  ) {
    this.targetHeight = 0;
  }

  setTargetHeight(height: number) {
    this.targetHeight = height;
  }

  createReachedPromise(): Promise<void> {
    return new Promise<void>((resolve) => {
      this.reachedResolve = resolve;
    });
  }

  createErrorPromise(): Promise<void> {
    return new Promise<void>((resolve) => {
      this.errorResolve = resolve;
    });
  }

  getSubscriptionError(): Error | null {
    return this.subscriptionError;
  }

  getWrappedHandler(): SubscriptionHandlers<
    SubscriptionPayload<{ blocks: Block }>
  > {
    return {
      next: (payload: SubscriptionPayload<{ blocks: Block }>) => {
        // First handle errors
        if (payload.errors) {
          console.error("[ERROR] - Indexer subscription failed!");
          console.error(
            "Indexer error payload:",
            JSON.stringify(payload.errors, null, 2),
          );
          console.info("[INFO ] - Closing connection and exiting...");

          this.subscriptionError = new Error("Indexer subscription failed");
          this.errorResolve?.();
          return;
        }

        // Call the original handler for normal processing
        this.originalHandler.next(payload);

        // Check if we've reached the target height
        const currentHeightVal = Number(payload?.data?.blocks?.height ?? NaN);
        if (
          !this.reachedTarget &&
          Number.isFinite(currentHeightVal) &&
          currentHeightVal >= this.targetHeight
        ) {
          this.reachedTarget = true;
          this.reachedResolve?.();
        }
      },
      complete: this.originalHandler.complete,
    };
  }
}

/**
 * Creates a spinner utility for CLI progress indication
 */
function createSpinner() {
  const isCI = process.env.CI === "true";

  // In CI environments, disable spinner completely
  if (isCI) {
    return {
      update(blockCount: number) {
        // No-op in CI
      },
      clear() {
        // No-op in CI
      },
    };
  }

  // Local terminal spinner with animation
  const spinnerChars = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
  let spinnerIndex = 0;
  let lastUpdate = Date.now();

  return {
    update(blockCount: number) {
      const now = Date.now();
      // Always update spinner and message, but only if enough time has passed
      if (now - lastUpdate > CONFIG.SPINNER_UPDATE_INTERVAL_MS) {
        spinnerIndex = (spinnerIndex + 1) % spinnerChars.length;
        lastUpdate = now;
      }
      process.stdout.write(
        `\r${spinnerChars[spinnerIndex]} Scanning blocks... ${blockCount} blocks received`,
      );
    },
    clear() {
      process.stdout.write("\r" + " ".repeat(80) + "\r");
    },
  };
}

function decodeReadyState(readyState: number): string {
  switch (readyState) {
    case WebSocket.OPEN:
      return "CONNECTED";
    case WebSocket.CONNECTING:
      return "CONNECTING";
    case WebSocket.CLOSING:
      return "CLOSING";
    case WebSocket.CLOSED:
      return "CLOSED";
    default:
      return "UNKNOWN";
  }
}

async function connectionInit(ws: WebSocket) {
  const timeoutMs = CONFIG.CONNECTION_TIMEOUT_MS;
  console.debug(
    `[DEBUG] - Websocket connection status: ${decodeReadyState(ws.readyState)}`,
  );

  const maxTime = Date.now() + timeoutMs;
  while (ws.readyState !== WebSocket.OPEN) {
    if (Date.now() > maxTime) {
      throw new Error("WebSocket connection timeout");
    }
    await new Promise((res) => setTimeout(res, 200));
    console.debug(
      `[DEBUG] - Web socket connection status: ${decodeReadyState(ws.readyState)}`,
    );

    if (ws.readyState === WebSocket.CLOSED) {
      throw new Error("Indexer websocket connection closed unexpectedly");
    }
  }

  const response: Promise<{ type: string }> = new Promise((resolve, reject) => {
    const timeout = setTimeout(() => {
      ws.removeEventListener("message", onMessage);
      reject(new Error("Timed out waiting for connection_ack"));
    }, timeoutMs);

    const onMessage = (event: MessageEvent) => {
      const message = JSON.parse(event.data);
      if (message.type === "connection_ack") {
        clearTimeout(timeout);
        ws.removeEventListener("message", onMessage);
        resolve(message);
      }
    };

    ws.addEventListener("message", onMessage);
    ws.send(
      JSON.stringify({
        type: "connection_init", // Payload is optional and can be used for negotiation
      }),
    );
  });

  if ((await response).type !== "connection_ack") {
    throw new Error("connection_ack message wasn't received");
  }
}

async function connectionInitWithRetry(): Promise<WebSocket> {
  let lastError: Error | undefined;
  const maxRetries: number = CONFIG.CONNECTION_MAX_RETRIES;
  const baseDelayMs: number = CONFIG.CONNECTION_RETRY_BASE_DELAY_MS;

  let ws: WebSocket;
  for (let attempt = 1; attempt <= maxRetries; attempt++) {
    try {
      console.info(`[INFO ] - Connection attempt ${attempt}/${maxRetries}`);
      ws = new WebSocket(INDEXER_WS_URL, CONFIG.WS_PROTOCOL);
      await connectionInit(ws);
      console.info(`[INFO ] - Successfully connected on attempt ${attempt}`);
      return ws; // Success!
    } catch (error) {
      lastError = error as Error;
      console.warn(
        `[WARN ] - Connection attempt ${attempt}/${maxRetries} failed: ${lastError.message}`,
      );

      if (attempt < maxRetries) {
        const delayMs = baseDelayMs * Math.pow(2, attempt - 1); // Exponential backoff
        console.info(`[INFO ] - Retrying in ${delayMs}ms...`);
        await new Promise((resolve) => setTimeout(resolve, delayMs));
      }
    }
  }

  throw new Error(
    `Failed to initialize connection after ${maxRetries} attempts. Last error: ${lastError?.message}`,
  );
}

function generateCustomId(): string {
  const chars =
    "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
  const segment = () =>
    Array.from({ length: 7 }, () =>
      chars.charAt(Math.floor(Math.random() * chars.length)),
    ).join("");
  return `${segment()}-${segment()}-${segment()}`;
}

function handleMessage(event: MessageEvent) {
  const message = JSON.parse(event.data);
  const { id, payload, type } = message;

  if (type === "connection_ack") {
    return;
  }

  const handlers = handlersMap.get(id);
  if (!handlers) return;

  switch (type) {
    case "next":
      handlers.next?.(payload);
      break;
    case "error":
      handlers.error?.(payload);
      break;
    case "complete":
      handlers.complete?.();
      handlersMap.delete(id);
      break;
  }
}

/**
 * Subscribes to block events from the websocket connection to the
 * indexer through the GraphQL API
 *
 * @param ws - The WebSocket connection to the indexer
 * @param blockOffset - The block offset to start from
 * @param handlers - The handlers to handle the block events
 * @param queryOverride - The query override to use
 * @param variablesOverride - The variables override to use
 * @returns Unsubscribe function to stop the subscription
 */
function subscribeToBlockEvents(
  ws: WebSocket,
  blockOffset: BlockOffset | undefined,
  handlers: SubscriptionHandlers<SubscriptionPayload<{ blocks: Block }>>,
  queryOverride?: string,
  variablesOverride?: Record<string, unknown>,
): () => void {
  const id = "1";

  // Read the GraphQL subscription query from file
  const BLOCK_SUBSCRIPTION_QUERY_WITH_OFFSET = fs
    .readFileSync(
      path.join(
        path.dirname(new URL(import.meta.url).pathname),
        "block-subscription.graphql",
      ),
      "utf-8",
    )
    .trim();

  let query = BLOCK_SUBSCRIPTION_QUERY_WITH_OFFSET;

  let variables: Record<string, unknown> | undefined = blockOffset
    ? {
        OFFSET: blockOffset,
      }
    : undefined;

  if (queryOverride) query = queryOverride;

  const payload = {
    id,
    type: "start",
    payload: {
      query,
      variables,
    },
  };

  handlersMap.set(id, handlers);
  ws.send(JSON.stringify(payload));

  return () => {
    const stopMessage = { id, type: "stop" };
    ws.send(JSON.stringify(stopMessage));
    handlersMap.delete(id);
  };
}

function parseStartBlockHeight(): number | undefined {
  const raw = process.env.START_BLOCK_HEIGHT ?? Bun.env.START_BLOCK_HEIGHT;
  if (raw === undefined || raw === "") return undefined;
  const n = parseInt(raw, 10);
  return Number.isFinite(n) && n >= 0 ? n : undefined;
}

async function main(): Promise<boolean> {
  // Record start time for duration calculation
  const startTime = Date.now();

  // Checking indexer is up and running on http ready endpoint
  console.info(
    `[INFO ] - Checking indexer is up and running on ${TARGET_ENV} (${INDEXER_HTTP_URL}/ready)`,
  );
  try {
    const httpReadyResponse = await fetch(`${INDEXER_HTTP_URL}/ready`, {
      signal: AbortSignal.timeout(10_000),
    });
    if (!httpReadyResponse.ok) {
      console.error(
        `[ERROR] - Indexer is not ready on ${TARGET_ENV} (${INDEXER_HTTP_URL}/ready)`,
      );
      console.error(
        `[ERROR] - Replied with status ${httpReadyResponse.status}: ${httpReadyResponse.statusText}`,
      );
      return false;
    }
  } catch (error) {
    console.error("[ERROR] - Failed to connect to indexer:", error);
    return false;
  }
  console.info(`[INFO ] - Indexer is ready!`);

  console.info(
    `[INFO ] - Connecting to indexer on ${TARGET_ENV} through websocket channel ${INDEXER_WS_URL}`,
  );

  // Initialize the websocket connection with retry
  const indexerWs = await connectionInitWithRetry();
  indexerWs.onmessage = handleMessage.bind(indexerWs);

  // One-shot query to get the latest block height at start time
  async function getLatestBlockHeight(
    ws: WebSocket,
    timeoutMs = CONFIG.QUERY_TIMEOUT_MS,
  ): Promise<number> {
    const id = generateCustomId();
    const query = `query GetLatestBlock { block { height } }`;

    return await new Promise<number>((resolve, reject) => {
      let resolved = false;

      const timeout = setTimeout(() => {
        handlersMap.delete(id);
        if (!resolved)
          reject(new Error("Timed out fetching latest block height"));
      }, timeoutMs);

      handlersMap.set(id, {
        next: (payload: { data?: { block?: { height?: number } } }) => {
          try {
            const height = payload?.data?.block?.height ?? 0;
            const stopMessage = { id, type: "stop" };
            ws.send(JSON.stringify(stopMessage));
            clearTimeout(timeout);
            handlersMap.delete(id);
            if (!resolved) {
              resolved = true;
              resolve(Number(height) || 0);
            }
          } catch (e) {
            clearTimeout(timeout);
            handlersMap.delete(id);
            if (!resolved) reject(e);
          }
        },
        complete: () => {
          // no-op; we resolve on next
        },
        error: (err) => {
          clearTimeout(timeout);
          handlersMap.delete(id);
          if (!resolved) reject(err);
        },
      });

      const payload = {
        id,
        type: "start",
        payload: { query },
      };
      ws.send(JSON.stringify(payload));
    });
  }

  const receivedBlocks: Block[] = [];
  let blocksWithTransactions = 0;
  let transactionsFound = 0;
  let contractActionsFound = 0;

  // Spinner for progress indication
  const spinner = createSpinner();
  const blockSubscriptionHandler: SubscriptionHandlers<
    SubscriptionPayload<{ blocks: Block }>
  > = {
    next: async (payload) => {
      if (payload.data !== undefined) {
        receivedBlocks.push(payload.data.blocks);
        if (payload.data?.blocks.transactions.length > 0) {
          transactionsFound += payload.data?.blocks.transactions.length;
          blocksWithTransactions++;

          // Write the block to file as a json line
          blocksFile.write(JSON.stringify(payload.data.blocks) + "\n");
        }
        if (
          payload.data?.blocks.transactions.some(
            (transaction) =>
              transaction.__typename === "RegularTransaction" &&
              (transaction as RegularTransaction).contractActions!.length > 0,
          )
        ) {
          contractActionsFound++;
        }
      }

      // Update spinner with current progress
      spinner.update(receivedBlocks.length);
    },
    error: (err) => {
      console.error(
        `[ERROR] - Subscription handler received error payload:\n${JSON.stringify(err, null, 2)}`,
      );
    },
    complete: () => {
      console.debug("Completed sent from Indexer");
    },
  };

  // Determine target height, then compute start height (START_BLOCK_HEIGHT > stats file > 0)
  const targetHeight = await getLatestBlockHeight(indexerWs).catch(() => 0);
  console.debug(
    `[DEBUG] - The selected environment has ${targetHeight} blocks`,
  );
  const TIMEOUT_MS = CONFIG.TIMEOUT_MS;

  const startBlockHeightEnv = parseStartBlockHeight();
  const previousStats = loadStats();
  const startHeight =
    startBlockHeightEnv !== undefined
      ? startBlockHeightEnv
      : previousStats !== null
        ? previousStats.lastScannedBlockHeight + 1
        : 0;

  if (startHeight > targetHeight) {
    console.info(
      "[INFO ] - Already caught up: start height exceeds latest block height; nothing to scan.",
    );
    if (previousStats !== null) {
      writeStats({
        ...previousStats,
        lastUpdated: new Date().toISOString(),
      });
    }
    await cleanupResources(indexerWs, handlersMap);
    return true;
  }
  if (startBlockHeightEnv !== undefined && startHeight >= targetHeight) {
    console.error(
      `[ERROR] - START_BLOCK_HEIGHT (${startBlockHeightEnv}) must be less than latest block height (${targetHeight})`,
    );
    await cleanupResources(indexerWs, handlersMap);
    return false;
  }
  if (startHeight < 0) {
    console.error("[ERROR] - Start height must be non-negative");
    await cleanupResources(indexerWs, handlersMap);
    return false;
  }

  const isResume = startBlockHeightEnv === undefined && previousStats !== null;
  const blocksFilePath = path.join(
    CONFIG.TMP_DIR,
    `${TARGET_ENV}_blocks.jsonl`,
  );
  const blocksFile = fs.createWriteStream(blocksFilePath, {
    flags: isResume ? "a" : "w",
  });

  // Create scan manager for handling subscription logic
  const scanManager = new BlockScanManager(
    handlersMap,
    blockSubscriptionHandler,
    spinner,
  );
  scanManager.setTargetHeight(targetHeight);

  const reachedPromise = scanManager.createReachedPromise();
  const errorPromise = scanManager.createErrorPromise();

  const blockOffset: BlockOffset = { height: startHeight };
  const unsubscribe = subscribeToBlockEvents(
    indexerWs,
    blockOffset,
    scanManager.getWrappedHandler(),
  );

  const blocksToStream = targetHeight - startHeight + 1;
  console.info("[INFO ] - Subscribed to block updates!");
  console.info(
    `[INFO ] - Streaming ${blocksToStream} blocks (from height ${startHeight} to ${targetHeight}, or ${TIMEOUT_MS / 1000}s timeout) ...`,
  );
  console.info(`[INFO ] - ... this might take a while`);

  await Promise.race([
    reachedPromise,
    errorPromise,
    new Promise((res) => setTimeout(res, TIMEOUT_MS)),
  ]);

  // Check if we exited due to an error
  const subscriptionError = scanManager.getSubscriptionError();
  if (subscriptionError) {
    // Clear the spinner line
    spinner.clear();

    console.error(
      `[ERROR] - Block scanning failed due to indexer subscription error: ${subscriptionError.message}`,
    );

    // Clean up and exit without throwing
    await cleanupResources(indexerWs, handlersMap);
    return false;
  }

  // Clear the spinner line before final messages
  spinner.clear();

  console.info("[INFO ] - Unsubscribing from block updates");
  unsubscribe();

  console.debug("[DEBUG] - Closing websocket connection");

  // Clean up all resources
  await cleanupResources(indexerWs, handlersMap);

  // Calculate scan duration
  const endTime = Date.now();
  const scanDurationSeconds = Math.round((endTime - startTime) / 1000);
  const lastScannedBlockHeight =
    receivedBlocks.length > 0
      ? Number(receivedBlocks[receivedBlocks.length - 1].height)
      : startHeight - 1;

  console.info("[INFO ] - Block fetching completed!");
  console.info(`[INFO ] - Summary report:
    - Total blocks scanned   : ${receivedBlocks.length}
    - Blocks with txs        : ${blocksWithTransactions}
    - Total txs found        : ${transactionsFound}
    - Contract actions found : ${contractActionsFound}
    - Scan duration          : ${scanDurationSeconds} seconds`);

  const runStats: ScanStats = {
    lastScannedBlockHeight,
    totalBlocksScanned: receivedBlocks.length,
    blocksWithTransactions,
    totalTransactionsFound: transactionsFound,
    contractActionsFound,
    totalScanDurationSeconds: scanDurationSeconds,
    lastUpdated: new Date().toISOString(),
  };
  const mergedStats: ScanStats =
    previousStats !== null
      ? {
          lastScannedBlockHeight: runStats.lastScannedBlockHeight,
          totalBlocksScanned:
            previousStats.totalBlocksScanned + runStats.totalBlocksScanned,
          blocksWithTransactions:
            previousStats.blocksWithTransactions +
            runStats.blocksWithTransactions,
          totalTransactionsFound:
            previousStats.totalTransactionsFound +
            runStats.totalTransactionsFound,
          contractActionsFound:
            previousStats.contractActionsFound + runStats.contractActionsFound,
          totalScanDurationSeconds:
            previousStats.totalScanDurationSeconds +
            runStats.totalScanDurationSeconds,
          lastUpdated: runStats.lastUpdated,
        }
      : runStats;
  writeStats(mergedStats);
  console.info(`[INFO ] - Stats written to ${getStatsPath()}`);

  // Update test data files if folder path was provided
  if (testDataFolder) {
    const sourceBlockDataFile = `${CONFIG.TMP_DIR}/${TARGET_ENV}_blocks.jsonl`;
    console.info(
      `[INFO ] - Using block info stored in: ./${sourceBlockDataFile}`,
    );
    console.info(
      `[INFO ] - Updating test data files in: ${testDataFolder}/${TARGET_ENV}`,
    );

    await updateTestDataFiles(testDataFolder, sourceBlockDataFile);
  }

  return true;
}

await main()
  .then((success) => {
    if (success) {
      console.info("[INFO ] - Process completed successfully");
      process.exit(0);
    } else {
      console.error("[ERROR] - Process completed with errors");
      process.exit(1);
    }
  })
  .catch((error) => {
    console.error("[ERROR] - Process failed:", error);
    process.exit(1);
  });
