// This file is part of midnight-node.
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

/**
 * Expand secret-file flags into in-memory argv values.
 *
 * The Rust toolkit hands secrets over via owner-only temp files instead of
 * argv, because a process's argv is world-readable on Linux via
 * `/proc/<pid>/cmdline` for its whole lifetime. Mutating the argv array here
 * is invisible to the kernel: `/proc/<pid>/cmdline` is a snapshot taken at
 * exec time, so the secret never appears in it.
 *
 * - `--signing-file <path>`       -> `--signing <contents>`
 * - `--new-authority-file <path>` -> `<contents>` (positional, in place, as
 *   maintain-contract takes the new authority positionally)
 */
export function expandSecretFileArgs(argv: string[], readSecret: (path: string) => string): void {
  // Backwards, so splices don't shift unvisited indices; each flag consumes
  // the element after it, hence `length - 2`.
  for (let i = argv.length - 2; i >= 0; i--) {
    if (argv[i] === '--signing-file') {
      argv.splice(i, 2, '--signing', readSecret(argv[i + 1]).trim());
    } else if (argv[i] === '--new-authority-file') {
      argv.splice(i, 2, readSecret(argv[i + 1]).trim());
    }
  }
}
