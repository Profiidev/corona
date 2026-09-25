use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{
  Attribute, Expr, ExprClosure, ExprLit, FnArg, ItemFn, Lit, Meta, Pat, ReturnType, Token,
  parse::{Parse, ParseStream},
  parse_macro_input,
};

// The generated code names `Named` by its path in the corona crate, the only user of these macros.

/// `#[host_fn] fn set_mute(pw: Glob<Pipewire>, id: u32, mute: bool) -> R { .. }` becomes a
/// `const set_mute: Named<fn(..) -> R>` carrying the script name `setMute` and the parameter names.
pub fn attribute(item: TokenStream) -> TokenStream {
  let item = parse_macro_input!(item as ItemFn);
  if !item.sig.generics.params.is_empty() {
    return syn::Error::new_spanned(&item.sig, "#[host_fn] needs a non-generic fn")
      .into_compile_error()
      .into();
  }

  let vis = &item.vis;
  let ident = &item.sig.ident;
  let output = &item.sig.output;
  let mut pats = Vec::new();
  let mut tys = Vec::new();
  for arg in &item.sig.inputs {
    match arg {
      FnArg::Typed(arg) => {
        pats.push(&*arg.pat);
        tys.push(&*arg.ty);
      }
      FnArg::Receiver(receiver) => {
        return syn::Error::new_spanned(receiver, "#[host_fn] cannot take `self`")
          .into_compile_error()
          .into();
      }
    }
  }
  let names = names(pats.iter().copied());
  let name = camel_case(&ident.to_string());
  let docs = docs(&item.attrs);

  if item.sig.asyncness.is_none() {
    return quote! {
      #[allow(non_upper_case_globals)]
      #vis const #ident: crate::script::host_fn::Named<fn(#(#tys),*) #output> = {
        #item
        // Typed `let`, so the fn item coerces to the fn pointer before `.docs` is called on it.
        let named: crate::script::host_fn::Named<fn(#(#tys),*) #output> =
          crate::script::host_fn::Named::new(#name, #names, #ident);
        named.docs(#docs)
      };
    }
    .into();
  }

  // An `async fn`'s future has no nameable type, so a sync wrapper boxes it. Params the future
  // borrows (`Cx`, `Glob`) make it non-'static, which the box type rejects.
  let ret = match output {
    ReturnType::Default => quote! { () },
    ReturnType::Type(_, ty) => quote! { #ty },
  };
  let future = quote! {
    ::std::pin::Pin<::std::boxed::Box<dyn ::std::future::Future<Output = #ret> + ::std::marker::Send>>
  };
  let args: Vec<_> = (0..tys.len()).map(|i| format_ident!("arg{i}")).collect();
  quote! {
    #[allow(non_upper_case_globals)]
    #vis const #ident: crate::script::host_fn::Named<fn(#(#tys),*) -> #future> = {
      #item
      fn boxed(#(#args: #tys),*) -> #future {
        ::std::boxed::Box::pin(#ident(#(#args),*))
      }
      let named: crate::script::host_fn::Named<fn(#(#tys),*) -> #future> =
        crate::script::host_fn::Named::new(#name, #names, boxed);
      named.docs(#docs)
    };
  }
  .into()
}

struct NamedClosure {
  name: Expr,
  attrs: Vec<Attribute>,
  closure: ExprClosure,
}

impl Parse for NamedClosure {
  fn parse(input: ParseStream) -> syn::Result<Self> {
    let name = input.parse()?;
    input.parse::<Token![,]>()?;
    let attrs = input.call(Attribute::parse_outer)?;
    let closure = input.parse()?;
    input.parse::<Option<Token![,]>>()?;
    Ok(Self {
      name,
      attrs,
      closure,
    })
  }
}

/// `named!("setMute", /// docs \n move |pw: Glob<Pipewire>, id: u32| ..)` wraps the closure in a
/// `Named`. The name is any `&'static str` expression; doc comments before the closure become the
/// function's JSDoc.
pub fn closure(input: TokenStream) -> TokenStream {
  let NamedClosure {
    name,
    attrs,
    closure,
  } = parse_macro_input!(input as NamedClosure);
  let names = names(closure.inputs.iter());
  let docs = docs(&attrs);
  quote! { crate::script::host_fn::Named::new(#name, #names, #closure).docs(#docs) }.into()
}

/// The `///` lines, joined; each line keeps the leading space `///` leaves.
fn docs(attrs: &[Attribute]) -> String {
  let lines: Vec<String> = attrs
    .iter()
    .filter(|attr| attr.path().is_ident("doc"))
    .filter_map(|attr| match &attr.meta {
      Meta::NameValue(meta) => match &meta.value {
        Expr::Lit(ExprLit {
          lit: Lit::Str(doc), ..
        }) => Some(doc.value().trim().to_owned()),
        _ => None,
      },
      _ => None,
    })
    .collect();
  lines.join("\n")
}

fn camel_case(snake: &str) -> String {
  let snake = snake.trim_start_matches("r#").trim_start_matches('_');
  let mut words = snake.split('_').filter(|word| !word.is_empty());
  let mut name = words.next().unwrap_or_default().to_owned();
  for word in words {
    let mut chars = word.chars();
    name.extend(chars.next().map(|c| c.to_ascii_uppercase()));
    name.push_str(chars.as_str());
  }
  name
}

/// Every parameter name, context params included; `_` prefixes are dropped, patterns become `argN`.
fn names<'a>(pats: impl Iterator<Item = &'a Pat>) -> TokenStream2 {
  let names = pats.enumerate().map(|(i, pat)| {
    let pat = match pat {
      Pat::Type(typed) => &*typed.pat,
      pat => pat,
    };
    match pat {
      Pat::Ident(ident) => ident.ident.to_string().trim_start_matches('_').to_owned(),
      _ => format!("arg{i}"),
    }
  });
  quote! { &[#(#names),*] }
}

#[cfg(test)]
mod tests {
  #[test]
  fn camel_case() {
    assert_eq!(super::camel_case("set_mute"), "setMute");
    assert_eq!(super::camel_case("list_sinks_now"), "listSinksNow");
    assert_eq!(super::camel_case("target"), "target");
    assert_eq!(super::camel_case("_private_fn"), "privateFn");
    assert_eq!(super::camel_case("r#type"), "type");
  }
}
