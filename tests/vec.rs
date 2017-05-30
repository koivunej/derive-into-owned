#![allow(dead_code)]

#[macro_use]
extern crate derive_into_owned;

use std::borrow::Cow;

#[derive(IntoOwned, Borrowed)]
struct Foo<'a> {
    bees: Vec<Bar<'a>>,
    cees: Vec<Cow<'a, str>>,
}

#[derive(IntoOwned, Borrowed)]
struct Bar<'a> {
    s: Cow<'a, str>,
}

#[test]
fn vec() {
    let local = "asdf".to_string();
    let foo = Foo { bees: vec![Bar { s: Cow::Borrowed(&local) }], cees: vec![] };
    accept_static(foo.into_owned());
}

fn accept_static(foo: Foo<'static>) {
    drop(foo);
}
