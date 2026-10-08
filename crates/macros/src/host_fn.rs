use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{
  Attribute, Expr, ExprClosure, ExprLit, FnArg, ItemFn, Lit, Meta, Pat, ReturnType, Token, Type,
  TypeParamBound,
  parse::{Parse, ParseStream},
};

// The generated code names `Named` by its path in the corona crate, the only user of these macros.

/// `#[host_fn] fn set_mute(pw: Glob<Pipewire>, id: u32, mute: bool) -> R { .. }` becomes a
/// `const set_mute: Named<fn(..) -> R>` carrying the script name `setMute` and the parameter names.
pub fn attribute(item: TokenStream2) -> TokenStream2 {
  let item = match syn::parse2::<ItemFn>(item) {
    Ok(item) => item,
    Err(e) => return e.into_compile_error(),
  };
  if !item.sig.generics.params.is_empty() {
    return syn::Error::new_spanned(&item.sig, "#[host_fn] needs a non-generic fn")
      .into_compile_error();
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
          .into_compile_error();
      }
    }
  }
  let names = names(pats.iter().copied());
  let name = camel_case(&ident.to_string());
  let docs = docs(&item.attrs);

  let future = if item.sig.asyncness.is_some() {
    let ret = match output {
      ReturnType::Default => quote! { () },
      ReturnType::Type(_, ty) => quote! { #ty },
    };
    quote! { ::std::future::Future<Output = #ret> + ::std::marker::Send }
  } else if let ReturnType::Type(_, ty) = output
    && let Type::ImplTrait(ty) = &**ty
  {
    let bounds = ty
      .bounds
      .iter()
      .filter(|bound| !matches!(bound, TypeParamBound::PreciseCapture(_)));
    quote! { #(#bounds)+* }
  } else {
    return quote! {
      #[allow(non_upper_case_globals)]
      #vis const #ident: crate::host_fn::Named<fn(#(#tys),*) #output> = {
        #item
        // Typed `let`, so the fn item coerces to the fn pointer before `.docs` is called on it.
        let named: crate::host_fn::Named<fn(#(#tys),*) #output> =
          crate::host_fn::Named::new(#name, #names, #ident);
        named.docs(#docs)
      };
    };
  };
  let future = quote! { ::std::pin::Pin<::std::boxed::Box<dyn #future>> };
  let args: Vec<_> = (0..tys.len()).map(|i| format_ident!("arg{i}")).collect();
  quote! {
    #[allow(non_upper_case_globals)]
    #vis const #ident: crate::host_fn::Named<fn(#(#tys),*) -> #future> = {
      #item
      fn boxed(#(#args: #tys),*) -> #future {
        ::std::boxed::Box::pin(#ident(#(#args),*))
      }
      let named: crate::host_fn::Named<fn(#(#tys),*) -> #future> =
        crate::host_fn::Named::new(#name, #names, boxed);
      named.docs(#docs)
    };
  }
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
pub fn closure(input: TokenStream2) -> TokenStream2 {
  let NamedClosure {
    name,
    attrs,
    closure,
  } = match syn::parse2(input) {
    Ok(input) => input,
    Err(e) => return e.into_compile_error(),
  };
  let names = names(closure.inputs.iter());
  let docs = docs(&attrs);
  if closure.asyncness.is_none() {
    return quote! { crate::host_fn::Named::new(#name, #names, #closure).docs(#docs) };
  }

  let mut args = Vec::new();
  let mut tys = Vec::new();
  for (i, pat) in closure.inputs.iter().enumerate() {
    let Pat::Type(typed) = pat else {
      return syn::Error::new_spanned(pat, "named! async closure params need a type")
        .into_compile_error();
    };
    args.push(format_ident!("arg{i}"));
    tys.push(&*typed.ty);
  }
  quote! {
    crate::host_fn::Named::new(#name, #names, {
      let f = ::std::sync::Arc::new(#closure);
      move |#(#args: #tys),*| {
        let f = ::std::sync::Arc::clone(&f);
        async move { (*f)(#(#args),*).await }
      }
    })
    .docs(#docs)
  }
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
  use quote::quote;
  use syn::parse_quote;

  use super::*;

  fn names_of(pats: &[Pat]) -> String {
    names(pats.iter()).to_string()
  }

  #[test]
  fn camel_case() {
    for (snake, camel) in [
      ("set_mute", "setMute"),
      ("list_sinks_now", "listSinksNow"),
      ("target", "target"),
      ("_private_fn", "privateFn"),
      ("r#type", "type"),
      ("a__b", "aB"),
      ("trailing_", "trailing"),
      ("_", ""),
      ("", ""),
      ("already_Camel", "alreadyCamel"),
    ] {
      assert_eq!(super::camel_case(snake), camel, "{snake}");
    }
  }

  #[test]
  fn names_strip_underscores_and_number_patterns() {
    let f: ExprClosure =
      parse_quote!(|_cx: Glob<A>, id: u32, (a, b): (u8, u8), _: bool, untyped, __x| ());
    let pats: Vec<Pat> = f.inputs.into_iter().collect();
    let want = quote!(&["cx", "id", "arg2", "arg3", "untyped", "x"]).to_string();
    assert_eq!(names_of(&pats), want);
    assert_eq!(names_of(&[]), quote!(&[]).to_string());
  }

  #[test]
  fn docs_join_trimmed_lines() {
    let item: ItemFn = parse_quote! {
      /// First line
      ///   indented
      #[doc = "  raw  "]
      #[inline]
      #[doc(hidden)]
      fn f() {}
    };
    assert_eq!(docs(&item.attrs), "First line\nindented\nraw");
    assert_eq!(docs(&[]), "");
  }

  #[test]
  fn sync_fn_is_named_const() {
    let out = attribute(quote! {
      /// Mutes it
      pub fn set_mute(_pw: Glob<P>, id: u32, mute: bool) -> R { todo!() }
    });
    let item: syn::ItemConst = syn::parse2(out.clone()).unwrap();
    assert_eq!(item.ident, "set_mute");
    assert!(matches!(item.vis, syn::Visibility::Public(_)));
    let ty = &item.ty;
    let ty = quote!(#ty).to_string();
    assert!(ty.contains("fn (Glob < P > , u32 , bool) -> R"), "{ty}");
    let out = out.to_string();
    assert!(out.contains("\"setMute\""), "{out}");
    assert!(out.contains(r#"& ["pw" , "id" , "mute"]"#), "{out}");
    assert!(out.contains("\"Mutes it\""), "{out}");
    assert!(!out.contains("boxed"), "{out}");
  }

  #[test]
  fn async_fn_is_boxed() {
    let out = attribute(quote! { async fn fetch(a: u32, b: String) -> u8 { 0 } });
    let item: syn::ItemConst = syn::parse2(out.clone()).unwrap();
    let ty = &item.ty;
    let ty = quote!(#ty).to_string();
    assert!(
      ty.contains("Pin < :: std :: boxed :: Box < dyn :: std :: future :: Future < Output = u8 >"),
      "{ty}"
    );
    assert!(ty.contains("Send"), "{ty}");
    let out = out.to_string();
    assert!(
      out.contains("fn boxed (arg0 : u32 , arg1 : String)"),
      "{out}"
    );
    assert!(out.contains("fetch (arg0 , arg1)"), "{out}");

    // no return type is `()`
    let out = attribute(quote! { async fn run() {} }).to_string();
    assert!(out.contains("Output = ()"), "{out}");
  }

  #[test]
  fn impl_future_is_boxed_without_use_bound() {
    let out = attribute(quote! {
      fn later(x: u8) -> impl Future<Output = u8> + Send + use<> { async move { x } }
    });
    let item: syn::ItemConst = syn::parse2(out).unwrap();
    let ty = &item.ty;
    let ty = quote!(#ty).to_string();
    assert!(ty.contains("dyn Future < Output = u8 > + Send >"), "{ty}");
    assert!(!ty.contains("use"), "{ty}");
  }

  #[test]
  fn attribute_rejects_generic_self_and_non_fn() {
    for item in [
      quote! { fn f<T>(t: T) {} },
      quote! { fn f(&self) {} },
      quote! { struct S; },
    ] {
      let out = attribute(item.clone()).to_string();
      assert!(out.contains("compile_error"), "{item} -> {out}");
    }
  }

  #[test]
  fn sync_closure_is_wrapped_as_is() {
    let out = closure(quote! {
      "setMute",
      /// Mutes
      move |_pw: Glob<P>, id| id,
    })
    .to_string();
    assert!(
      out.starts_with("crate :: host_fn :: Named :: new (\"setMute\""),
      "{out}"
    );
    assert!(out.contains(r#"& ["pw" , "id"]"#), "{out}");
    assert!(out.contains("move | _pw : Glob < P > , id | id"), "{out}");
    assert!(out.contains(". docs (\"Mutes\")"), "{out}");
    assert!(!out.contains("Arc"), "{out}");
  }

  #[test]
  fn async_closure_is_shared_behind_arc() {
    let out = closure(quote! { NAME, async |a: u8, b: String| a }).to_string();
    assert!(out.contains("Named :: new (NAME"), "{out}");
    assert!(
      out.contains("Arc :: new (async | a : u8 , b : String | a)"),
      "{out}"
    );
    assert!(out.contains("move | arg0 : u8 , arg1 : String |"), "{out}");
    assert!(out.contains("(* f) (arg0 , arg1) . await"), "{out}");
  }

  #[test]
  fn closure_errors() {
    for input in [
      quote! { "f", async |a: u8, b| a },
      quote! { "f" async |a: u8| a },
      quote! { "f", not_a_closure },
    ] {
      let out = closure(input.clone()).to_string();
      assert!(out.contains("compile_error"), "{input} -> {out}");
    }
  }
}
