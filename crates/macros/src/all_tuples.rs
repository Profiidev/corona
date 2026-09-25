use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{Ident, LitInt, parse::Parse, parse_macro_input, token::Comma};

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

pub fn all_tuples(input: TokenStream) -> TokenStream {
  let input = parse_macro_input!(input as AllTuples);
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

  TokenStream::from(quote! {
    #(
      #invocations
    )*
  })
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
