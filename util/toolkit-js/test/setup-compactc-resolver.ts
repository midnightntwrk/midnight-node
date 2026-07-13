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

// Install the same module-resolution hook the CLI uses (`src/bin.ts`), so that `compact-js*` /
// `compact-runtime` imports — including the transitive ones reached while a test loads a contract
// configuration — resolve against the variant pinned for the active COMPACTC_VERSION. This makes the
// tests exercise the exact version-dispatch behaviour as production.
import { installCompactcResolver, resolveCompactcVersion } from '../src/compactc-resolver.js';

const version = resolveCompactcVersion();
installCompactcResolver(version);
// Exposed for tests that want to report/assert which variant they ran against.
process.env.RESOLVED_COMPACTC_VERSION = version;
