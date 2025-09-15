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

/**
 * EventCoordinator provides a reusable pattern for managing promise-based notifications
 * with timeout support. This is useful in subscription based scenarios where there could
 * be a number of events streamed by the indexer and having a way to notify the tests of
 * such events or conditions is handy
 */
export class EventCoordinator {
  private events: Map<string, { resolve: () => void; promise: Promise<void> }> = new Map();

  /**
   * Creates a named event that can be waited on and triggered
   *
   * @param name - The name of the event
   * @returns A promise that resolves when the event is triggered
   */
  waitFor(name: string): Promise<void> {
    if (this.events.has(name)) {
      return this.events.get(name)!.promise;
    }

    let resolve: () => void;
    const promise = new Promise<void>((res) => {
      resolve = res;
    });

    this.events.set(name, { resolve: resolve!, promise });
    return promise;
  }

  /**
   * Triggers a named event, resolving any waiting promises
   * @param name - The name of the event to trigger
   */
  notify(name: string): void {
    const event = this.events.get(name);
    if (event) {
      event.resolve();
      this.events.delete(name);
    }
  }

  /**
   * Waits for multiple events to occur, with optional timeout
   *
   * @param eventNames - Array of event names to wait for
   * @param timeout - Optional timeout in milliseconds (default 3 seconds)
   * @returns Promise that resolves when all events occur or rejects on timeout
   */
  waitForAll(eventNames: string[], timeout: number = 3000): Promise<void> {
    const promises = eventNames.map((name) => this.waitFor(name));

    return Promise.race([
      Promise.all(promises).then(() => void 0),
      new Promise<void>((_, reject) =>
        setTimeout(
          () => reject(new Error(`Timeout waiting for events: ${eventNames.join(', ')}`)),
          timeout,
        ),
      ),
    ]);
  }

  /**
   * Waits for any of the specified events to occur, with optional timeout
   *
   * @param eventNames - Array of event names to wait for
   * @param timeout - Optional timeout in milliseconds (default 3 seconds)
   * @returns Promise that resolves when any event occurs or rejects on timeout
   */
  waitForAny(eventNames: string[], timeout: number = 3000): Promise<string> {
    const promises = eventNames.map((name) => this.waitFor(name).then(() => name));

    return Promise.race([
      Promise.race(promises),
      new Promise<string>((_, reject) =>
        setTimeout(
          () => reject(new Error(`Timeout waiting for any event: ${eventNames.join(', ')}`)),
          timeout,
        ),
      ),
    ]);
  }

  /**
   * Clears all pending events
   */
  clear(): void {
    this.events.clear();
  }
}
