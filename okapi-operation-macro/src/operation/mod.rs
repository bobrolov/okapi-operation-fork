use darling::{ast::NestedMeta, FromMeta};
use proc_macro2::TokenStream;
use quote::{format_ident, quote, ToTokens};
use syn::{Ident, ItemFn, Visibility};

use self::{external_docs::ExternalDocs, request_body::RequestBody, response::Responses};
use crate::{
    error::Error,
    operation::{parameters::Parameters, security::Security},
    utils::{attribute_to_args, quote_option, take_attributes},
    OPENAPI_FUNCTION_NAME_SUFFIX,
};

mod cookie;
mod external_docs;
mod header;
mod parameters;
mod path;
mod query;
mod reference;
mod request_body;
mod response;
mod security;

// TODO:
//  - support examples ??
//  - support all fields from Operation/Parameters/Responses
//  - support ToResponses for Result
//  - support for generic functions

static DEFAULT_OPENAPI_ATTRIBUTE_NAME: &str = "openapi";
static DEFAULT_CRATE_NAME: &str = "okapi_operation";

#[derive(Debug, FromMeta)]
struct OperationAttrs {
    #[darling(default)]
    summary: Option<String>,
    #[darling(default)]
    description: Option<String>,
    #[darling(default)]
    operation_id: Option<String>,
    #[darling(default)]
    tags: Option<String>,
    #[darling(default)]
    deprecated: bool,
    #[darling(default)]
    external_docs: Option<ExternalDocs>,
    #[darling(default)]
    parameters: Parameters,
    #[darling(default)]
    responses: Responses,
    #[darling(default)]
    security: Option<Security>,

    #[darling(default = "OperationAttrs::default_crate_name", rename = "crate")]
    crate_name: String,
}

impl ToTokens for OperationAttrs {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let summary = quote_option(&self.summary);
        let description = quote_option(&self.description);
        let operation_id = quote_option(&self.operation_id);
        let external_docs = quote_option(&self.external_docs);
        let deprecated = &self.deprecated;
        let tags = {
            let base_str = self.tags.as_deref().unwrap_or_default();
            let values = if !base_str.is_empty() {
                base_str.split(',').map(|y| y.trim()).collect::<Vec<_>>()
            } else {
                vec![]
            };
            quote! {
                vec![
                    #(#values.into()),*
                ]
            }
        };
        let security = quote_option(&self.security);

        let new_tokens = quote! {
            summary: #summary,
            description: #description,
            external_docs: #external_docs,
            operation_id: #operation_id,
            deprecated: #deprecated,
            tags: #tags,
            security: #security,
        };
        tokens.extend(new_tokens);
    }
}

impl OperationAttrs {
    fn default_crate_name() -> String {
        DEFAULT_CRATE_NAME.into()
    }

    fn default_attribute_name() -> String {
        DEFAULT_OPENAPI_ATTRIBUTE_NAME.into()
    }
}

pub(crate) fn openapi(
    attrs: proc_macro::TokenStream,
    mut input: ItemFn,
) -> Result<TokenStream, Error> {
    let mut attrs = NestedMeta::parse_meta_list(attrs.into())?;

    for attr in take_attributes(&mut input.attrs, DEFAULT_OPENAPI_ATTRIBUTE_NAME) {
        attrs.extend(attribute_to_args(&attr)?);
    }
    let mut operation_attrs = OperationAttrs::from_list(&attrs)?;
    operation_attrs
        .responses
        .add_return_type(&input, operation_attrs.responses.ignore_return_type);
    let request_body = RequestBody::from_item_fn(&mut input)?;
    let openapi_generator_fn =
        build_openapi_generator_fn(&input.sig.ident, &input.vis, operation_attrs, request_body)?;
    let output = quote! {
        #input

        #openapi_generator_fn
    };
    Ok(output)
}

fn build_openapi_generator_fn(
    handler_name: &Ident,
    vis: &Visibility,
    attrs: OperationAttrs,
    request_body: Option<RequestBody>,
) -> Result<TokenStream, Error> {
    let name = format_ident!("{}{}", handler_name, OPENAPI_FUNCTION_NAME_SUFFIX);

    let crate_name: proc_macro2::TokenStream = attrs
        .crate_name
        .parse()
        .map_err(|err| Error::custom(format!("Failed to parse provided crate rename: {err}")))?;

    let request_body = request_body.map(|x| {
        quote! {
            request_body: Some(okapi::openapi3::RefOr::Object(#x)),
        }
    });
    let parameters = &attrs.parameters;
    let responses = &attrs.responses;
    Ok(quote! {
        #[allow(non_snake_case, unused)]
        #vis fn #name(
            components: &mut #crate_name::Components
        ) -> std::result::Result<#crate_name::okapi::openapi3::Operation, anyhow::Error> {
            use #crate_name::_macro_prelude::*;

            let mut operation = okapi::openapi3::Operation {
                #attrs
                #request_body
                #responses
                #parameters
                ..Default::default()
            };
            Ok(operation)
        }
    })
}
