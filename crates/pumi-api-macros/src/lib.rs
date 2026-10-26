use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{
    Attribute, Expr, ExprLit, FnArg, GenericArgument, ItemFn, Lit, LitStr, Pat, PathArguments, ReturnType, Type,
    parse_macro_input,
};
#[proc_macro_attribute]
pub fn api(attr: TokenStream, item: TokenStream) -> TokenStream {
    let mut opts = Opts::default();
    let parser = syn::meta::parser(|meta| opts.parse(meta));
    parse_macro_input!(attr with parser);
    let func = parse_macro_input!(item as ItemFn);
    match expand(opts, func) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}
#[derive(Default)]
struct Opts {
    name: Option<LitStr>,
    since: Option<u32>,
    runs_on: Option<LitStr>,
    effect: Option<LitStr>,
    internal: bool,
    platforms: Vec<LitStr>,
    examples: Vec<Example>,
    notes: Vec<LitStr>,
}
struct Example {
    args: LitStr,
    result: Option<LitStr>,
    desc: Option<LitStr>,
}
impl Opts {
    fn parse(&mut self, meta: syn::meta::ParseNestedMeta) -> syn::Result<()> {
        if meta.path.is_ident("name") {
            let lit: LitStr = meta.value()?.parse()?;
            let ok = lit.value().split('.').all(|p| {
                p.chars().next().is_some_and(|c| c.is_ascii_lowercase()) && p.chars().all(|c| c.is_ascii_alphanumeric())
            });
            if !ok {
                return Err(syn::Error::new(
                    lit.span(),
                    "a name is `noun.verb`, camelCase after each dot (`tab.update`), or one word (`call`)",
                ));
            }
            self.name = Some(lit);
        } else if meta.path.is_ident("since") {
            let lit: syn::LitInt = meta.value()?.parse()?;
            self.since = Some(lit.base10_parse()?);
        } else if meta.path.is_ident("runs_on") {
            let lit: LitStr = meta.value()?.parse()?;
            if !["server", "client", "either"].contains(&lit.value().as_str()) {
                return Err(syn::Error::new(lit.span(), "runs_on is \"server\", \"client\" or \"either\""));
            }
            self.runs_on = Some(lit);
        } else if meta.path.is_ident("effect") {
            let lit: LitStr = meta.value()?.parse()?;
            if !["read", "write", "forward"].contains(&lit.value().as_str()) {
                return Err(syn::Error::new(lit.span(), "effect is \"read\", \"write\" or \"forward\""));
            }
            self.effect = Some(lit);
        } else if meta.path.is_ident("internal") {
            self.internal = true;
        } else if meta.path.is_ident("platform") {
            self.platforms.push(meta.value()?.parse()?);
        } else if meta.path.is_ident("note") {
            self.notes.push(meta.value()?.parse()?);
        } else if meta.path.is_ident("example") {
            let (mut args, mut result, mut desc) = (None, None, None);
            meta.parse_nested_meta(|inner| {
                let lit: LitStr = inner.value()?.parse()?;
                if inner.path.is_ident("args") {
                    args = Some(lit);
                } else if inner.path.is_ident("result") {
                    result = Some(lit);
                } else if inner.path.is_ident("desc") {
                    desc = Some(lit);
                } else {
                    return Err(inner.error("an example takes `args`, `result` and `desc`"));
                }
                Ok(())
            })?;
            let args = args.ok_or_else(|| meta.error("an example needs `args`, a JSON array or object"))?;
            self.examples.push(Example { args, result, desc });
        } else {
            return Err(meta.error(
                "unknown key; #[api] takes name, since, runs_on, effect, internal, platform, note and example(args, result, desc)",
            ));
        }
        Ok(())
    }
}
fn expand(opts: Opts, func: ItemFn) -> syn::Result<TokenStream2> {
    let name = &func.sig.ident;
    let ident_str = name.to_string();
    let name_str = opts.name.as_ref().map(LitStr::value).unwrap_or_else(|| ident_str.clone());
    let doc = doc_comment(&func.attrs);
    if doc.is_empty() {
        return Err(syn::Error::new_spanned(
            &func.sig.ident,
            format!("`{name_str}` has no doc comment; every API function is described"),
        ));
    }
    if opts.examples.is_empty() {
        return Err(syn::Error::new_spanned(
            &func.sig.ident,
            format!("`{name_str}` has no example; add `example(args = \"[…]\", result = \"…\")` to #[api]"),
        ));
    }
    let mut inputs = func.sig.inputs.iter();
    match inputs.next() {
        Some(FnArg::Typed(first)) if matches!(&*first.ty, Type::Reference(r) if r.mutability.is_some()) => {}
        _ => {
            return Err(syn::Error::new_spanned(
                &func.sig,
                "an API function takes `cx: &mut pumi_api::Cx` first, then its parameters",
            ));
        }
    }
    let mut params = Vec::new();
    for input in inputs {
        let FnArg::Typed(pat_type) = input else {
            return Err(syn::Error::new_spanned(input, "an API function is a free function"));
        };
        let Pat::Ident(ident) = &*pat_type.pat else {
            return Err(syn::Error::new_spanned(&pat_type.pat, "parameters are plain names, so they can be named"));
        };
        let pname = ident.ident.to_string();
        let pname = pname.strip_prefix("r#").unwrap_or(&pname).to_string();
        params.push(Param { name: pname, ty: (*pat_type.ty).clone(), optional: is_option(&pat_type.ty) });
    }
    let ret = match &func.sig.output {
        ReturnType::Type(_, ty) => result_ok_type(ty)
            .ok_or_else(|| syn::Error::new_spanned(ty, "an API function returns `pumi_api::Result<T>`"))?,
        ReturnType::Default => {
            return Err(syn::Error::new_spanned(&func.sig, "an API function returns `pumi_api::Result<T>`"));
        }
    };
    let since = opts.since.unwrap_or(1);
    let runs_on = match opts.runs_on.as_ref().map(LitStr::value).as_deref() {
        Some("client") => quote!(::pumi_api::RunsOn::Client),
        Some("either") => quote!(::pumi_api::RunsOn::Either),
        _ => quote!(::pumi_api::RunsOn::Server),
    };
    let effect = match opts.effect.as_ref().map(LitStr::value).as_deref() {
        Some("read") => quote!(::pumi_api::Effect::Read),
        Some("forward") => quote!(::pumi_api::Effect::Forward),
        _ => quote!(::pumi_api::Effect::Write),
    };
    let internal = opts.internal;
    let platforms = &opts.platforms;
    let notes = &opts.notes;
    let examples = opts.examples.iter().map(|example| {
        let args = &example.args;
        let result = option_lit(&example.result);
        let desc = option_lit(&example.desc);
        quote!(::pumi_api::Example { args: #args, result: #result, desc: #desc })
    });
    let schema_fns: Vec<_> = (0..params.len()).map(|i| format_ident!("__schema_{}", i)).collect();
    let param_entries = params.iter().zip(&schema_fns).map(|(param, schema_fn)| {
        let pname = &param.name;
        let optional = param.optional;
        quote!(::pumi_api::Param { name: #pname, optional: #optional, schema: #schema_fn })
    });
    let schema_defs = params.iter().zip(&schema_fns).map(|(param, schema_fn)| {
        let ty = &param.ty;
        quote! {
            fn #schema_fn(generator: &mut ::pumi_api::schemars::SchemaGenerator) -> ::pumi_api::schemars::Schema {
                generator.subschema_for::<#ty>()
            }
        }
    });
    let names: Vec<_> = params.iter().map(|param| param.name.as_str()).collect();
    let bindings: Vec<_> = (0..params.len()).map(|i| format_ident!("__arg_{}", i)).collect();
    let takes = params.iter().zip(&bindings).enumerate().map(|(i, (param, binding))| {
        let ty = &param.ty;
        let pname = &param.name;
        let optional = param.optional;
        quote!(let #binding: #ty = __args.take(#i, #pname, #optional)?;)
    });
    let takes_check = takes.clone();
    let static_name = format_ident!("__PUMI_API_{}", ident_str.to_uppercase());
    Ok(quote! {
        #func
        #[doc(hidden)]
        #[allow(non_snake_case)]
        const _: () = {
            #(#schema_defs)*
            fn __returns(generator: &mut ::pumi_api::schemars::SchemaGenerator) -> ::pumi_api::schemars::Schema {
                generator.subschema_for::<#ret>()
            }
            fn __call(
                cx: &mut ::pumi_api::Cx<'_>,
                args: ::pumi_api::serde_json::Value,
            ) -> ::pumi_api::Result<::pumi_api::serde_json::Value> {
                let mut __args = ::pumi_api::Args::new(#name_str, args, &[#(#names),*])?;
                #(#takes)*
                let result: #ret = #name(cx, #(#bindings),*)?;
                ::pumi_api::to_value(result)
            }
            fn __check(args: ::pumi_api::serde_json::Value, result: ::core::option::Option<::pumi_api::serde_json::Value>) -> ::pumi_api::Result<()> {
                let mut __args = ::pumi_api::Args::new(#name_str, args, &[#(#names),*])?;
                #(#takes_check)*
                let _ = (#(#bindings,)*);
                if let ::core::option::Option::Some(result) = result {
                    ::pumi_api::from_value::<#ret>(result, "the example's result")?;
                }
                ::core::result::Result::Ok(())
            }
            #[cfg(not(target_family = "wasm"))]
            #[::pumi_api::linkme::distributed_slice(::pumi_api::FUNCTIONS)]
            #[linkme(crate = ::pumi_api::linkme)]
            static #static_name: ::pumi_api::Function = ::pumi_api::Function {
                name: #name_str,
                desc: #doc,
                since: #since,
                runs_on: #runs_on,
                effect: #effect,
                internal: #internal,
                platforms: &[#(#platforms),*],
                params: &[#(#param_entries),*],
                returns: __returns,
                examples: &[#(#examples),*],
                notes: &[#(#notes),*],
                call: __call,
                check: __check,
                module: ::core::module_path!(),
            };
        };
    })
}
struct Param {
    name: String,
    ty: Type,
    optional: bool,
}
fn option_lit(lit: &Option<LitStr>) -> TokenStream2 {
    match lit {
        Some(lit) => quote!(::core::option::Option::Some(#lit)),
        None => quote!(::core::option::Option::None),
    }
}
fn doc_comment(attrs: &[Attribute]) -> String {
    let mut lines = Vec::new();
    for attr in attrs {
        if !attr.path().is_ident("doc") {
            continue;
        }
        if let syn::Meta::NameValue(nv) = &attr.meta
            && let Expr::Lit(ExprLit { lit: Lit::Str(s), .. }) = &nv.value
        {
            let line = s.value();
            lines.push(line.strip_prefix(' ').unwrap_or(&line).to_string());
        }
    }
    lines.join("\n").trim().to_string()
}
fn last_segment(ty: &Type) -> Option<&syn::PathSegment> {
    match ty {
        Type::Path(path) if path.qself.is_none() => path.path.segments.last(),
        _ => None,
    }
}
/// Whether a parameter may be left out: `Option<T>`, or `pumi_api::Optional<T>`.
fn is_option(ty: &Type) -> bool {
    last_segment(ty).is_some_and(|seg| seg.ident == "Option" || seg.ident == "Optional")
}
fn result_ok_type(ty: &Type) -> Option<Type> {
    let seg = last_segment(ty)?;
    if seg.ident != "Result" {
        return None;
    }
    let PathArguments::AngleBracketed(args) = &seg.arguments else { return None };
    args.args.iter().find_map(|arg| match arg {
        GenericArgument::Type(ty) => Some(ty.clone()),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn expand_source(src: &str) -> syn::Result<TokenStream2> {
        expand(Opts::default(), syn::parse_str::<ItemFn>(src).expect("a function"))
    }
    #[test]
    fn a_function_without_a_doc_comment_does_not_compile() {
        let err = expand_source(
            "#[api(example(args = \"[]\"))] pub fn pumi_undocumented(cx: &mut pumi_api::Cx) -> pumi_api::Result<()> { Ok(()) }",
        )
        .unwrap_err();
        assert!(err.to_string().contains("no doc comment"), "{err}");
    }
    #[test]
    fn a_function_without_an_example_does_not_compile() {
        let err = expand_source(
            "/// Does nothing.\n#[api] pub fn pumi_unexampled(cx: &mut pumi_api::Cx) -> pumi_api::Result<()> { Ok(()) }",
        )
        .unwrap_err();
        assert!(err.to_string().contains("no example"), "{err}");
    }
}
