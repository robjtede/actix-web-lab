//! Experimental macros for Actix Web.

#![forbid(unsafe_code)]
#![cfg_attr(docsrs, feature(doc_auto_cfg))]

use quote::{format_ident, quote};
use syn::{DeriveInput, Ident, parse_macro_input, punctuated::Punctuated, token::Comma};

/// Derive a `FromRequest` implementation for an aggregate struct extractor.
///
/// All fields of the struct need to implement `FromRequest` unless they are marked with annotations
/// that declare different handling is required.
///
/// # Examples
/// ```
/// use actix_web::{Responder, get, http, web};
/// use actix_web_lab::FromRequest;
///
/// #[derive(Debug, FromRequest)]
/// struct RequestParts {
///     // the FromRequest impl is used for these fields
///     method: http::Method,
///     pool: web::Data<u32>,
///     req_body: String,
///
///     // equivalent to `req.app_data::<u64>().copied()`
///     #[from_request(copy_from_app_data)]
///     int: u64,
/// }
///
/// #[get("/")]
/// async fn handler(parts: RequestParts) -> impl Responder {
///     // ...
///     # ""
/// }
/// ```
#[proc_macro_derive(FromRequest, attributes(from_request))]
pub fn derive_from_request(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let name = input.ident;

    let data = match input.data {
        syn::Data::Struct(data) => data,
        syn::Data::Enum(_) | syn::Data::Union(_) => {
            return quote! {
                compile_error!("Deriving FromRequest is only supported on structs for now.");
            }
            .into();
        }
    };

    let fields = match data.fields {
        syn::Fields::Named(fields) => fields.named,
        syn::Fields::Unnamed(_) | syn::Fields::Unit => {
            return quote! {
                compile_error!("Deriving FromRequest is only supported on structs with named fields for now.");
            }
            .into();
        }
    };

    let field_names_joined = fields
        .iter()
        .map(|f| f.ident.clone().unwrap())
        .collect::<Punctuated<_, Comma>>();

    // i.e., field has no special handling, it's just extracted using its FromRequest impl
    let fut_fields = fields.iter().filter(|field| {
        field.attrs.is_empty()
            || field
                .attrs
                .iter()
                .any(|attr| attr.parse_args::<Ident>().is_err())
    });

    let field_fut_names_joined = fut_fields
        .clone()
        .map(|f| format_ident!("{}_fut", f.ident.clone().unwrap()))
        .collect::<Punctuated<_, Comma>>();

    let field_post_fut_names_joined = fut_fields
        .clone()
        .map(|f| f.ident.clone().unwrap())
        .collect::<Punctuated<_, Comma>>();

    let field_futs = fut_fields.clone().map(|field| {
        let syn::Field { ident, ty, .. } = field;

        let varname = format_ident!("{}_fut", ident.clone().unwrap());

        quote! {
            let #varname = <#ty>::from_request(&req, pl).map_err(Into::into);
        }
    });

    let fields_copied_from_app_data = fields
        .iter()
        .filter(|field| {
            field.attrs.iter().any(|attr| {
                attr.parse_args::<Ident>().is_ok_and(|ident| ident == "copy_from_app_data")
            })
        })
        .map(|field| {
            let syn::Field { ident, ty, .. } = field;

            let varname = ident.clone().unwrap();

            quote! {
                let #varname = if let Some(st) = req.app_data::<#ty>().copied() {
                    st
                } else {
                    ::actix_web_lab::__reexports::tracing::debug!(
                        "Failed to extract `{}` for `{}` handler. For this extractor to work \
                        correctly, pass the data to `App::app_data()`. Ensure that types align in \
                        both the set and retrieve calls.",
                        ::std::any::type_name::<#ty>(),
                        req.match_name().unwrap_or_else(|| req.path())
                    );

                    return ::std::boxed::Box::pin(async move {
                        ::std::result::Result::Err(
                            ::actix_web_lab::__reexports::actix_web::error::ErrorInternalServerError(
                            "Requested application data is not configured correctly. \
                            View/enable debug logs for more details.",
                        ))
                    })
                };
            }
        });

    let output = quote! {
        impl ::actix_web::FromRequest for #name {
            type Error = ::actix_web::Error;
            type Future = ::std::pin::Pin<::std::boxed::Box<
                dyn ::std::future::Future<Output = ::std::result::Result<Self, Self::Error>>
            >>;

            fn from_request(req: &::actix_web::HttpRequest, pl: &mut ::actix_web::dev::Payload) -> Self::Future {
                use ::actix_web_lab::__reexports::actix_web::FromRequest as _;
                use ::actix_web_lab::__reexports::futures_util::{FutureExt as _, TryFutureExt as _};
                use ::actix_web_lab::__reexports::tokio::try_join;

                #(#fields_copied_from_app_data)*

                #(#field_futs)*

                ::std::boxed::Box::pin(
                    async move { try_join!( #field_fut_names_joined ) }
                        .map_ok(move |( #field_post_fut_names_joined )| Self { #field_names_joined })
                )
           }
        }
    };

    proc_macro::TokenStream::from(output)
}
