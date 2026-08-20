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

declare module 'vitest' {
  interface Assertion {
    toBeError(message?: string): void;
    toBeSuccess(message?: string): void;
  }

  // Custom fields we attach via `ctx.task.meta` in tests. Augment TaskMeta
  // (the type of Test.meta) rather than redefining TestContext, so vitest's
  // own TestContext members (skip, task, signal, …) are preserved.
  interface TaskMeta {
    done?: boolean;
    custom?: Record<string, unknown>;
  }
}

// This file must be a module for `declare module 'vitest'` to *augment*
// (merge with) vitest's types rather than shadow them.
export {};
