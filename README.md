**derive-into-owned**

Proof of concept Rust procedural macro for deriving methods like:

```
import std::borrow::Cow;

struct Foo<'a> {
	field: Cow<'a, str>,
}

impl<'a> Foo<'a> {
	/// This method would be derived using #[derive(IntoOwned)]
	fn into_owned(self) -> Foo<'static> {
		Foo {
			field: Cow::Owned(self.field.into_owned()),
		}
	}
}
```

Currently it is just an edited version of [deep-clone-derive](https://github.com/asajeffrey/deep-clone/blob/master/deep-clone-derive/lib.rs) example but supports:

 * [tuple structs](./blob/tests/tuple_struct.rs)
 * normal [structs](./blob/tests/struct.rs)
 * enums with tuple variants [tuple enums](./blob/tests/simple_enum.rs)
 * `IntoOwned` alike fields (actually assumes all fields with types with lifetimes are `IntoOwned` alike)

## Limitations

Currently it will fail miserably for at least but not limited to:

 * borrowed fields like `&'a str`
 * options of Cow types `Option<Cow<'a, str>>`
 * options of Cow-like types `Option<Cow<'a, str>>`

## Types with lifetimes

If your struct has a field with type `Bar<'a>`, it is assumed to have a method `fn into_owned(self) -> Bar<'static>`.
