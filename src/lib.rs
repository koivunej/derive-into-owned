extern crate proc_macro;
extern crate syn;
#[macro_use]
extern crate quote;

use proc_macro::TokenStream;

#[proc_macro_derive(IntoOwned)]
pub fn into_owned(input: TokenStream) -> TokenStream {
    let source = input.to_string();

    let ast = syn::parse_derive_input(&source).unwrap();

    let expanded = impl_into_owned(&ast);

    expanded.parse().unwrap()
}

fn impl_into_owned(ast: &syn::DeriveInput) -> quote::Tokens {
    // this is based heavily on https://github.com/asajeffrey/deep-clone/blob/master/deep-clone-derive/lib.rs
    let name = &ast.ident;

    let borrowed_lifetime_params = ast.generics.lifetimes.iter().map(|alpha| quote! { #alpha });
    let borrowed_type_params = ast.generics.ty_params.iter().map(|ty| quote! { #ty });
    let borrowed_params = borrowed_lifetime_params.chain(borrowed_type_params).collect::<Vec<_>>();
    let borrowed = if borrowed_params.is_empty() {
        quote! { }
    } else {
        quote! { < #(#borrowed_params),* > }
    };


    let params = ast.generics.lifetimes.iter().map(|alpha| quote! { #alpha }).chain(ast.generics.ty_params.iter().map(|ty| { let ref ident = &ty.ident; quote! { #ident } })).collect::<Vec<_>>();
    let params = if params.is_empty() {
        quote! {}
    } else {
        quote! { < #(#params),* > }
    };

    let owned_lifetime_params = ast.generics.lifetimes.iter().map(|_| quote! { 'static });
    let owned_type_params = ast.generics.ty_params.iter().map(|ty| { let ref ident = &ty.ident; quote! { #ident } });
    let owned_params = owned_lifetime_params.chain(owned_type_params).collect::<Vec<_>>();
    let owned = if owned_params.is_empty() {
        quote! { }
    } else {
        quote! { < #(#owned_params),* > }
    };

    let into_owned = match ast.body {
        syn::Body::Struct(ref variant) => {
            let inner = ctor_fields(variant);
            quote! { #name #inner }
        },
        syn::Body::Enum(ref body) => {
            let cases = body.iter()
                .map(|case| {
                    let unqualified_ident = &case.ident;
                    let ident = quote! { #name::#unqualified_ident };
                    match case.data {
                        syn::VariantData::Struct(ref body) => {
                            let idents = body.iter()
                                .map(|field| field.ident.as_ref().unwrap())
                                .collect::<Vec<_>>();
                            let cloned = body.iter()
                                .map(|field| {
                                    let ref ident = field.ident.as_ref().unwrap();
                                    let ident = quote! { #ident };
                                    let code = FieldKind::resolve(field).move_or_clone_field(&ident);
                                    quote! { #ident: #code }
                                })
                                .collect::<Vec<_>>();
                            quote! { #ident { #(#idents),* } => #ident { #(#cloned),* } }
                        },
                        syn::VariantData::Tuple(ref body) => {
                            let idents = (0..body.len())
                                .map(|index| syn::Ident::from(format!("x{}", index)))
                                .collect::<Vec<_>>();
                            let cloned = idents.iter().zip(body.iter())
                                .map(|(ident, field)| {
                                    let ident = quote! { #ident };
                                    FieldKind::resolve(field).move_or_clone_field(&ident)
                                })
                                .collect::<Vec<_>>();
                            quote! { #ident ( #(#idents),* ) => #ident ( #(#cloned),* ) }
                        },
                        syn::VariantData::Unit => {
                            quote! { #ident => #ident }
                        },
                    }
                })
                .collect::<Vec<_>>();
            quote! { match self { #(#cases),* } }
        },
    };

    quote! {
        impl #borrowed #name #params {
            pub fn into_owned(self) -> #name #owned { #into_owned }
        }
    }
}

fn ctor_fields(data: &syn::VariantData) -> quote::Tokens {
    match *data {
        syn::VariantData::Struct(ref body) => {
            let fields = body.iter()
                .map(|field| {
                    let ident = field.ident.as_ref().unwrap();
                    let field_ref = quote! { self.#ident };
                    let code = FieldKind::resolve(field).move_or_clone_field(&field_ref);
                    quote! { #ident: #code }
                })
                .collect::<Vec<_>>();
            quote! { { #(#fields),* } }
        },
        syn::VariantData::Tuple(ref body) => {
            let fields = body.iter()
                .enumerate()
                .map(|(index, field)| {
                    let index = syn::Ident::from(index);
                    let index = quote! { self.#index };
                    FieldKind::resolve(field).move_or_clone_field(&index)
                })
                .collect::<Vec<_>>();
            quote! { ( #(#fields),* ) }
        },
        syn::VariantData::Unit => {
            quote! {}

        }
    }
}

enum FieldKind {
    PlainCow,
    AssumedCow,
    /// Option fields with either PlainCow or AssumedCow
    OptField(usize, Box<FieldKind>),
    JustMoved
}

impl FieldKind {

    fn resolve(field: &syn::Field) -> Self {
        match &field.ty {
            &syn::Ty::Path(None, syn::Path { ref segments, .. }) => {
                if is_cow(segments) {
                    FieldKind::PlainCow
                } else if is_cow_alike(segments) {
                    FieldKind::AssumedCow
                } else if let Some(kind) = is_opt_cow(segments) {
                    kind
                } else {
                    FieldKind::JustMoved
                }
            },
            _ => FieldKind::JustMoved,
        }
    }

    fn move_or_clone_field(&self, var: &quote::Tokens) -> quote::Tokens {
        use self::FieldKind::*;

        match self {
            &PlainCow => quote! { ::std::borrow::Cow::Owned(#var.into_owned()) },
            &AssumedCow => quote! { #var.into_owned() },
            &OptField(levels, ref inner) => {
                let next = syn::Ident::from("val");
                let next = quote! { #next };

                let mut tokens = inner.move_or_clone_field(&next);

                for _ in 0..(levels - 1) {
                    tokens = quote! { #next.map(|#next| #tokens) };
                }

                quote! { #var.map(|#next| #tokens) }
            }
            &JustMoved => quote! { #var },
        }
    }
}

fn type_hopefully_is(segments: &Vec<syn::PathSegment>, expected: &str) -> bool {
    let expected = expected.split("::").map(syn::Ident::from).collect::<Vec<_>>();
    if segments.len() > expected.len() {
        return false
    }

    let expected = expected.iter().map(|x| x).collect::<Vec<_>>();
    let segments = segments.iter().map(|x| &x.ident).collect::<Vec<_>>();

    for len in 0..expected.len() {
        if &segments[..] == &expected[expected.len() - len - 1..] {
            return true;
        }
    }

    false
}

fn is_cow(segments: &Vec<syn::PathSegment>) -> bool {
    type_hopefully_is(segments, "std::borrow::Cow")
}

fn is_cow_alike(segments: &Vec<syn::PathSegment>) -> bool {
    if let Some(&syn::PathParameters::AngleBracketed(ref data)) = segments.last().map(|x| &x.parameters) {
        !data.lifetimes.is_empty()
    } else {
        false
    }
}

fn is_opt_cow(mut segments: &Vec<syn::PathSegment>) -> Option<FieldKind> {
    let mut levels = 0;
    loop {
        if type_hopefully_is(segments, "std::option::Option") {
            match *segments.last().unwrap() {
                syn::PathSegment { parameters: syn::PathParameters::AngleBracketed(ref data), .. } => {
                    if !data.lifetimes.is_empty() || !data.bindings.is_empty() {
                        // Option<&'a ?> cannot be moved but let the compiler complain
                        // don't know about data bindings
                        break;
                    }

                    if data.types.len() != 1 {
                        // Option<A, B> probably means some other, movable option
                        break;
                    }

                    match *data.types.first().unwrap() {
                        syn::Ty::Path(None, syn::Path { segments: ref next_segments, ..}) => {
                            levels += 1;
                            segments = next_segments;
                            continue;
                        }
                        _ => break,
                    }
                },
                _ => {}
            }
        } else if is_cow(segments) {
            return Some(FieldKind::OptField(levels, Box::new(FieldKind::PlainCow)));
        } else if is_cow_alike(segments) {
            return Some(FieldKind::OptField(levels, Box::new(FieldKind::AssumedCow)));
        }

        break;
    }
    return None;
}
