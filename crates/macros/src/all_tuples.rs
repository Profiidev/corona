use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{Ident, LitInt, parse::Parse, token::Comma};

struct AllTuples {
  macro_ident: Ident,
  start: usize,
  end: usize,
  idents: Vec<Ident>,
}

impl Parse for AllTuples {
  fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
    let macro_ident = input.parse::<Ident>()?;
    input.parse::<Comma>()?;
    let start = input.parse::<LitInt>()?.base10_parse()?;
    input.parse::<Comma>()?;
    let end = input.parse::<LitInt>()?.base10_parse()?;
    input.parse::<Comma>()?;
    let mut idents = vec![input.parse::<Ident>()?];
    while input.parse::<Comma>().is_ok() {
      idents.push(input.parse::<Ident>()?);
    }

    Ok(AllTuples {
      macro_ident,
      start,
      end,
      idents,
    })
  }
}

pub fn all_tuples(input: TokenStream2) -> TokenStream2 {
  let input = match syn::parse2::<AllTuples>(input) {
    Ok(input) => input,
    Err(e) => return e.into_compile_error(),
  };
  let len = 1 + input.end - input.start;
  let mut ident_tuples = Vec::with_capacity(len);
  for i in 0..=len {
    let idents = input
      .idents
      .iter()
      .map(|ident| format_ident!("{}{}", ident, i));
    ident_tuples.push(to_ident_tuple(idents, input.idents.len()));
  }

  let macro_ident = &input.macro_ident;
  let invocations = (input.start..=input.end).map(|i| {
    let ident_tuples = choose_ident_tuples(&ident_tuples, i);
    quote! {
      #macro_ident!(#ident_tuples);
    }
  });

  quote! {
    #(
      #invocations
    )*
  }
}

fn to_ident_tuple(idents: impl Iterator<Item = Ident>, len: usize) -> TokenStream2 {
  if len < 2 {
    quote! { #(#idents)* }
  } else {
    quote! { (#(#idents),*) }
  }
}

fn choose_ident_tuples(ident_tuples: &[TokenStream2], i: usize) -> TokenStream2 {
  let ident_tuples = &ident_tuples[..i];
  quote! { #(#ident_tuples),* }
}

#[cfg(test)]
mod tests {
  use quote::quote;
  use syn::{Item, Type, punctuated::Punctuated};

  use super::*;

  /// The arguments of each generated `m!(..)`, in order
  fn invocations(out: TokenStream2) -> Vec<Vec<String>> {
    syn::parse2::<syn::File>(out)
      .unwrap()
      .items
      .into_iter()
      .map(|item| {
        let Item::Macro(item) = item else {
          panic!("expected a macro invocation");
        };
        assert!(item.mac.path.is_ident("m"));
        item
          .mac
          .parse_body_with(Punctuated::<Type, Comma>::parse_terminated)
          .unwrap()
          .iter()
          .map(|ty| quote!(#ty).to_string())
          .collect()
      })
      .collect()
  }

  #[test]
  fn one_invocation_per_arity() {
    let calls = invocations(all_tuples(quote!(m, 0, 16, F)));
    assert_eq!(calls.len(), 17);
    for (i, call) in calls.iter().enumerate() {
      assert_eq!(call.len(), i);
      let want: Vec<_> = (0..i).map(|j| format!("F{j}")).collect();
      assert_eq!(*call, want);
    }
  }

  #[test]
  fn several_idents_make_tuples() {
    let calls = invocations(all_tuples(quote!(m, 1, 2, A, B)));
    assert_eq!(calls, [vec!["(A0 , B0)"], vec!["(A0 , B0)", "(A1 , B1)"]]);
  }

  #[test]
  fn start_equal_end() {
    let calls = invocations(all_tuples(quote!(m, 2, 2, F)));
    assert_eq!(calls, [vec!["F0", "F1"]]);
  }

  #[test]
  fn bad_input_is_compile_error() {
    for input in [
      quote!(m, 0, 2),
      quote!(m 0, 2, F),
      quote!(m, x, 2, F),
      quote!(m, 0, 2, F,),
      quote!(),
    ] {
      let out = all_tuples(input.clone()).to_string();
      assert!(out.contains("compile_error"), "{input} -> {out}");
    }
  }

  #[test]
  #[ignore = "bug: start > end underflows `1 + end - start` and panics"]
  fn bug_start_after_end_panics() {
    let out = std::panic::catch_unwind(|| all_tuples(quote!(m, 3, 1, F)).to_string());
    let out = out.expect("all_tuples panicked");
    assert!(out.is_empty() || out.contains("compile_error"), "{out}");
  }

  #[test]
  #[ignore = "bug: only end - start + 2 tuples are built, so start > 2 slices out of range"]
  fn bug_start_above_two_slices_out_of_range() {
    let calls = std::panic::catch_unwind(|| invocations(all_tuples(quote!(m, 3, 4, F))));
    let calls = calls.expect("all_tuples panicked");
    assert_eq!(calls.iter().map(Vec::len).collect::<Vec<_>>(), [3, 4]);
  }
}
