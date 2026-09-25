use proc_macro::TokenStream;

mod all_tuples;
mod host_fn;

#[proc_macro]
pub fn all_tuples(input: TokenStream) -> TokenStream {
  all_tuples::all_tuples(input)
}

/// Turns a fn into a `Named` host fn, so `Module` can declare it with its parameter names.
#[proc_macro_attribute]
pub fn host_fn(_: TokenStream, item: TokenStream) -> TokenStream {
  host_fn::attribute(item)
}

/// Wraps a closure in a `Named` host fn, so `Module` can declare it with its parameter names.
#[proc_macro]
pub fn named(input: TokenStream) -> TokenStream {
  host_fn::closure(input)
}
