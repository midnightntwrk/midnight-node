// This file is part of midnight-node.
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

use proc_macro::TokenStream;
use quote::{ToTokens, quote};
use syn::{Attribute, Data, DeriveInput, ExprLit, Fields, FieldsNamed, parse_macro_input};

fn get_doc_string(attrs: &[Attribute]) -> String {
	let docs = attrs
		.iter()
		.filter(|attr| attr.path().is_ident("doc"))
		.filter_map(|attr| {
			let meta = attr.meta.require_name_value().unwrap();
			if let syn::Expr::Lit(ExprLit { lit: syn::Lit::Str(ref lit), .. }) = meta.value {
				Some(lit.value().trim().to_string())
			} else {
				None
			}
		})
		.collect::<Vec<_>>();

	docs.join("\n")
}

fn get_tags(attrs: &[Attribute]) -> Vec<String> {
	attrs
		.iter()
		.filter_map(|attr| {
			if attr.path().is_ident("doc_tag") {
				let mut tags = Vec::new();
				let meta = attr.meta.require_list().unwrap();
				tags.push(meta.tokens.to_string());
				Some(tags)
			} else {
				None
			}
		})
		.fold(Vec::new(), |mut acc, tags| {
			acc.extend(tags);
			acc
		})
}

#[proc_macro_derive(Documented, attributes(doc_tag))]
pub fn derive_documented(input: TokenStream) -> TokenStream {
	let input = parse_macro_input!(input as DeriveInput);
	let name = &input.ident;

	let fields = match &input.data {
		Data::Struct(data_struct) => match &data_struct.fields {
			Fields::Named(FieldsNamed { named, .. }) => named,
			_ => panic!("Documented can only be derived for structs with named fields"),
		},
		_ => panic!("Documented can only be derived for structs"),
	};

	let field_docs = fields.iter().map(|field| {
		let field_name = &field.ident.as_ref().unwrap().to_string();
		let field_type = &field.ty.to_token_stream().to_string();
		let doc = get_doc_string(&field.attrs);
		let tags = get_tags(&field.attrs);

		quote! {
			documented::FieldInfo {
				name: #field_name.to_string(),
				field_type: #field_type.to_string(),
				doc: #doc.to_string(),
				tags: vec![ #(#tags.to_string()),* ]
			}
		}
	});

	let expanded = quote! {

		impl documented::DocumentedFields for #name {
			fn field_docs() -> Vec<documented::FieldInfo> {
				vec![
					#(#field_docs),*
				]
			}
		}
	};

	TokenStream::from(expanded)
}
