//! Experimental macros for Actix Web.

use quote::{format_ident, quote};
use syn::{parse_macro_input, punctuated::Punctuated, token::Comma, DeriveInput};

/// Derive a `FromRequest` implementation for an aggregate struct extractor.
///
/// All fields of the struct need to implement `FromRequest`.
///
/// # Examples
/// ```
/// use actix_web::{Responder, http, get, web};
/// use actix_web_lab::FromRequest;
///
/// #[derive(Debug, FromRequest)]
/// struct RequestParts {
///     method: http::Method,
///     pool: web::Data<u32>,
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

    let field_fut_names_joined = fields
        .iter()
        .map(|f| format_ident!("{}_fut", f.ident.clone().unwrap()))
        .collect::<Punctuated<_, Comma>>();

    let field_futs = fields.iter().map(|field| {
        let syn::Field { ident, ty, .. } = field;

        let varname = format_ident!("{}_fut", ident.clone().unwrap());

        quote! {
            let #varname = <#ty>::from_request(&req, pl).map_err(Into::into);
        }
    });

    let output = quote! {
        impl ::actix_web::FromRequest for #name {
            type Error = ::actix_web::Error;
            type Future = ::std::pin::Pin<Box<
                dyn ::std::future::Future<Output = ::std::result::Result<Self, Self::Error>>
            >>;

            fn from_request(req: &::actix_web::HttpRequest, pl: &mut ::actix_web::dev::Payload) -> Self::Future {
                use ::actix_web_lab::__reexports::actix_web::FromRequest as _;
                use ::actix_web_lab::__reexports::futures_util::{FutureExt as _, TryFutureExt as _};
                use ::actix_web_lab::__reexports::tokio::try_join;

                #(#field_futs)*

                Box::pin(
                    async move { try_join!( #field_fut_names_joined ) }
                    .map_ok(|( #field_names_joined )| Self { #field_names_joined })
                )
           }
        }
    };

    proc_macro::TokenStream::from(output)
}
