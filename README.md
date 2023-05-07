# rofi-mode

[![crates.io](https://img.shields.io/crates/v/rofi-mode.svg)](https://crates.io/crates/rofi-mode)
![License](https://img.shields.io/badge/License-MIT-green.svg)

A high-level Rust library for creating Rofi plugins and custom modes

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

Then, add this crate as a dependency using the following command:

```bash
cargo add rofi-mode
```

Now in your `lib.rs`,
create a struct and implement the [`Mode`] trait for it.
For example, here is a no-op mode with no entries:

```rust
struct Mode;

impl rofi_mode::Mode<'_> for Mode {
    const NAME: &'static str = "an-example-mode\0";
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
when it starts up
(alternatively,
you can set the `ROFI_PLUGIN_PATH` environment variable
to the directory your `.so` file is in).
You can then run your mode from Rofi's command line:

```sh
rofi -modi an-example-mode -show an-example-mode
```

## Examples

- See [examples/basic] for a basic example of a non-trivial Rofi mode,
    which allows the user to add to the list of entries by writing in the Rofi box.
- See [examples/file-browser] for a Rofi mode implementing a simple file browser.

[`Mode`]: https://docs.rs/rofi-mode/latest/rofi_mode/trait.Mode.html
[`export_mode!`]: https://docs.rs/rofi-mode/latest/rofi_mode/macro.export_mode.html
[examples/basic]: https://github.com/SabrinaJewson/rofi-mode.rs/tree/main/examples/basic
[examples/file-browser]: https://github.com/SabrinaJewson/rofi-mode.rs/tree/main/examples/file-browser

License: MIT
