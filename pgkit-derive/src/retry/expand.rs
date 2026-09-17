use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{Block, Error, ItemFn, parse_macro_input};

use super::args::RetryArgs;
use super::async_trait::find_async_block;

pub(crate) fn expand(attr: TokenStream, item: TokenStream) -> TokenStream {
    let mut args = RetryArgs::default();
    let parser = syn::meta::parser(|meta| args.parse_one(meta));
    parse_macro_input!(attr with parser);

    let func = parse_macro_input!(item as ItemFn);
    match try_expand(args, func) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

fn try_expand(args: RetryArgs, mut func: ItemFn) -> syn::Result<TokenStream2> {
    let policy = args.policy_expr();

    if func.sig.asyncness.is_some() {
        let ItemFn {
            attrs,
            vis,
            sig,
            block,
        } = func;
        return Ok(quote! {
            #(#attrs)*
            #vis #sig {
                ::pgkit::retry::run(#policy, || async #block).await
            }
        });
    }

    // Not async: accept the #[async_trait] desugaring, where the body
    // builds a `Box::pin(async move { ... })` future, and wrap inside it.
    let Some(future_block) = find_async_block(&mut func.block) else {
        return Err(Error::new_spanned(
            func.sig.fn_token,
            "#[pgkit::retry] requires an async fn or an #[async_trait] method \
             (the retry loop awaits between attempts)",
        ));
    };

    let attempt_block = std::mem::replace(
        future_block,
        Block {
            brace_token: Default::default(),
            stmts: Vec::new(),
        },
    );
    *future_block = syn::parse_quote!({
        ::pgkit::retry::run(#policy, || async #attempt_block).await
    });

    let ItemFn {
        attrs,
        vis,
        sig,
        block,
    } = func;
    Ok(quote! {
        #(#attrs)*
        #vis #sig #block
    })
}
