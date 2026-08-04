// This file is part of midnight-node.
// Copyright (C) Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// You may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Proc-macros for the `midnight-node-e2e` integration test suite.

use proc_macro::TokenStream;
use quote::quote;
use syn::{ItemFn, parse_macro_input};

/// Drop-in replacement for `#[tokio::test]` that also initialises the e2e
/// tracing subscriber and attaches a span tagged with the function name to
/// the test future via `tracing::Instrument`.
///
/// `Instrument` is used rather than `Span::entered()` so the span follows
/// the future across `.await` points instead of sitting on the thread-local
/// span stack — otherwise auxiliary tasks polled on the same thread while
/// the guard is alive would be misattributed to the current test, and that
/// would silently break the moment a test switches to a multi-threaded
/// runtime flavour.
///
/// ```ignore
/// #[e2e_test]
/// async fn my_test() {
///     tracing::info!("emitted with `my_test:` prefix");
/// }
/// ```
///
/// Other attributes (e.g. `#[ignore]`) compose normally:
///
/// ```ignore
/// #[e2e_test]
/// #[ignore = "manual run only"]
/// async fn slow_test() { /* ... */ }
/// ```
#[proc_macro_attribute]
pub fn e2e_test(args: TokenStream, item: TokenStream) -> TokenStream {
    let args = proc_macro2::TokenStream::from(args);
    let input = parse_macro_input!(item as ItemFn);

    let attrs = &input.attrs;
    let vis = &input.vis;
    let sig = &input.sig;
    let block = &input.block;
    let name_lit = sig.ident.to_string();

    let tokio_test = if args.is_empty() {
        quote! { #[::tokio::test] }
    } else {
        quote! { #[::tokio::test(#args)] }
    };

    quote! {
        #tokio_test
        #(#attrs)*
        #vis #sig {
            ::midnight_node_e2e::logger::init();
            ::tracing::Instrument::instrument(
                async move #block,
                ::tracing::info_span!(#name_lit),
            )
            .await
        }
    }
    .into()
}
