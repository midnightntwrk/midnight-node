// This file is part of midnight-indexer.
// Copyright (C) Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
// http://www.apache.org/licenses/LICENSE-2.0
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Schema-level GraphQL directives for indexer-api.

use async_graphql::TypeDirective;

/// Marks a schema field or type as in-flight / unstable. Consumers should
/// expect the marked surface to change without notice; stability is signalled
/// by removal of the directive.
///
/// Currently used for the dust-generation API surface that's in mid-redesign
/// pending #1181. See #1173.
#[TypeDirective(name = "beta", location = "FieldDefinition", location = "Object")]
pub fn beta() {}
