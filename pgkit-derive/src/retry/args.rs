use proc_macro2::{Ident, Span, TokenStream as TokenStream2};
use quote::quote;
use syn::{Error, LitInt, LitStr};

const DEFAULT_TRIES: u32 = 3;
const DEFAULT_DELAY_MS: u64 = 100;
const DEFAULT_MAX_DELAY_MS: u64 = 10_000;

pub(super) struct RetryArgs {
    tries: u32,
    backoff: Ident,
    delay_ms: u64,
    max_delay_ms: u64,
    idempotent: bool,
    jitter: bool,
}

impl Default for RetryArgs {
    fn default() -> Self {
        Self {
            tries: DEFAULT_TRIES,
            backoff: Ident::new("Linear", Span::call_site()),
            delay_ms: DEFAULT_DELAY_MS,
            max_delay_ms: DEFAULT_MAX_DELAY_MS,
            idempotent: false,
            jitter: false,
        }
    }
}

impl RetryArgs {
    pub(super) fn parse_one(&mut self, meta: syn::meta::ParseNestedMeta) -> syn::Result<()> {
        if meta.path.is_ident("tries") {
            let lit: LitInt = meta.value()?.parse()?;
            self.tries = lit.base10_parse()?;
            if self.tries == 0 {
                return Err(Error::new(lit.span(), "tries must be at least 1"));
            }
            return Ok(());
        }

        if meta.path.is_ident("backoff") {
            let lit: LitStr = meta.value()?.parse()?;
            let variant = match lit.value().as_str() {
                "fixed" => "Fixed",
                "linear" => "Linear",
                "exponential" => "Exponential",
                other => {
                    return Err(Error::new(
                        lit.span(),
                        format!(
                            "unknown backoff \"{other}\"; expected one of: \
                             \"fixed\", \"linear\", \"exponential\""
                        ),
                    ));
                }
            };
            self.backoff = Ident::new(variant, lit.span());
            return Ok(());
        }

        if meta.path.is_ident("delay_ms") {
            let lit: LitInt = meta.value()?.parse()?;
            self.delay_ms = lit.base10_parse()?;
            return Ok(());
        }

        if meta.path.is_ident("max_delay_ms") {
            let lit: LitInt = meta.value()?.parse()?;
            self.max_delay_ms = lit.base10_parse()?;
            return Ok(());
        }

        if meta.path.is_ident("idempotent") {
            self.idempotent = true;
            return Ok(());
        }

        if meta.path.is_ident("jitter") {
            self.jitter = true;
            return Ok(());
        }

        Err(meta.error(
            "unknown #[pgkit::retry] argument; expected one of: \
             tries, backoff, delay_ms, max_delay_ms, idempotent, jitter",
        ))
    }

    /// The `RetryPolicy` construction expression for the generated code.
    pub(super) fn policy_expr(&self) -> TokenStream2 {
        let Self {
            tries,
            backoff,
            delay_ms,
            max_delay_ms,
            idempotent,
            jitter,
        } = self;
        let idempotent_call = idempotent.then(|| quote! { .idempotent(true) });
        let jitter_call = jitter.then(|| quote! { .jitter(true) });
        quote! {
            ::pgkit::retry::RetryPolicy::new(
                #tries,
                ::pgkit::retry::BackoffPolicy::#backoff,
                ::core::time::Duration::from_millis(#delay_ms),
                ::core::time::Duration::from_millis(#max_delay_ms),
            )
            #idempotent_call
            #jitter_call
        }
    }
}
