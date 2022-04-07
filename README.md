# rofi-mode

`rofi-mode` provides a high-level ergonomic wrapper around Rofi's C plugin API.

## Getting started

First of all,
create a new library with `cargo new --lib my_awesome_plugin`
and add these lines to the `Cargo.toml`:

```toml
[lib]
crate-type = ["cdylib"]
```

That will force Cargo to generate your library as a `.so` file,
which is what Rofi loads its plugins from.

Now in your `lib.rs`,
create a struct and implement the [`Mode`] trait for it.
For example, here is a no-op mode with no entries:

```rust
struct Mode;

impl rofi_mode::Mode<'_> for Mode {
    const NAME: &'static str = "an-example-mode\0";
    const DISPLAY_NAME: &'static str = "My example mode\0";
    fn init(_api: rofi_mode::Api<'_>) -> Result<Self, ()> {
        Ok(Self)
    }
    fn entries(&mut self) -> usize { 0 }
    fn entry_content(&self, _line: usize) -> rofi_mode::String { unreachable!() }
    fn react(
        &mut self,
        _event: rofi_mode::Event,
        _input: &mut rofi_mode::String,
    ) -> rofi_mode::Action {
        rofi_mode::Action::Exit
    }
    fn matches(&self, _line: usize, _matcher: rofi_mode::Matcher<'_>) -> bool {
        unreachable!()
    }
}
```

You then need to export your mode to Rofi via the [`export_mode!`] macro:

```rust
rofi_mode::export_mode!(Mode);
```

Build your library using `cargo build`
then copy the resulting dylib file
(e.g. `/target/debug/libmy_awesome_plugin.so`)
into `/lib/rofi`
so that Rofi will pick up on it
when it starts up.
You can then run your mode from Rofi's command line:

```sh
rofi -modi an-example-mode -show an-example-mode
```


[`Mode`]: https://docs.rs/rofi-mode/latest/rofi_mode/trait.Mode.html
[`export_mode!`]: https://docs.rs/rofi-mode/latest/rofi_mode/macro.export_mode.html

License: MIT
