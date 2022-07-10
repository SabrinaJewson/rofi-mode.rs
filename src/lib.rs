//! `rofi-mode` provides a high-level ergonomic wrapper around Rofi's C plugin API.
//!
//! # Getting started
//!
//! First of all,
//! create a new library with `cargo new --lib my_awesome_plugin`
//! and add these lines to the `Cargo.toml`:
//!
//! ```toml
//! [lib]
//! crate-type = ["cdylib"]
//! ```
//!
//! That will force Cargo to generate your library as a `.so` file,
//! which is what Rofi loads its plugins from.
//!
//! Now in your `lib.rs`,
//! create a struct and implement the [`Mode`] trait for it.
//! For example, here is a no-op mode with no entries:
//!
//! ```no_run
//! struct Mode;
//!
//! impl rofi_mode::Mode<'_> for Mode {
//!     const NAME: &'static str = "an-example-mode\0";
//!     const DISPLAY_NAME: &'static str = "My example mode\0";
//!     fn init(_api: rofi_mode::Api<'_>) -> Result<Self, ()> {
//!         Ok(Self)
//!     }
//!     fn entries(&mut self) -> usize { 0 }
//!     fn entry_content(&self, _line: usize) -> rofi_mode::String { unreachable!() }
//!     fn react(
//!         &mut self,
//!         _event: rofi_mode::Event,
//!         _input: &mut rofi_mode::String,
//!     ) -> rofi_mode::Action {
//!         rofi_mode::Action::Exit
//!     }
//!     fn matches(&self, _line: usize, _matcher: rofi_mode::Matcher<'_>) -> bool {
//!         unreachable!()
//!     }
//! }
//! ```
//!
//! You then need to export your mode to Rofi via the [`export_mode!`] macro:
//!
//! ```ignore
//! rofi_mode::export_mode!(Mode);
//! ```
//!
//! Build your library using `cargo build`
//! then copy the resulting dylib file
//! (e.g. `/target/debug/libmy_awesome_plugin.so`)
//! into `/lib/rofi`
//! so that Rofi will pick up on it
//! when it starts up
//! (alternatively,
//! you can set the `ROFI_PLUGIN_PATH` environment variable
//! to the directory your `.so` file is in).
//! You can then run your mode from Rofi's command line:
//!
//! ```sh
//! rofi -modi an-example-mode -show an-example-mode
//! ```
//!
//!
//! [`Mode`]: https://docs.rs/rofi-mode/latest/rofi_mode/trait.Mode.html
//! [`export_mode!`]: https://docs.rs/rofi-mode/latest/rofi_mode/macro.export_mode.html
#![warn(
    noop_method_call,
    trivial_casts,
    trivial_numeric_casts,
    unused_import_braces,
    unused_lifetimes,
    unused_qualifications,
    unsafe_op_in_unsafe_fn,
    missing_docs,
    missing_debug_implementations,
    clippy::pedantic
)]
#![allow(
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap
)]

use ::{
    bitflags::bitflags,
    cairo::ffi as cairo_sys,
    pango::{
        ffi as pango_sys,
        glib::{ffi as glib_sys, translate::ToGlibPtrMut},
    },
    std::{
        ffi::{c_void, CStr, CString},
        mem::{self, ManuallyDrop},
        os::raw::{c_char, c_int, c_uint},
        panic, process, ptr,
    },
};

pub use {cairo, pango, rofi_plugin_sys as ffi};

mod string;
pub use string::{format, String};

pub mod api;
pub use api::Api;

/// A mode supported by Rofi.
///
/// You can implement this trait on your own type to define a mode,
/// then export it in the shared library using [`export_mode!`].
pub trait Mode<'rofi>: Sized + Send + Sync {
    /// The name of the mode.
    ///
    /// This string must be null-terminated
    /// and contain no intermediate null characters.
    const NAME: &'static str;

    /// The display name of the mode to be shown as the prompt before the colon in Rofi.
    ///
    /// This string must be null-terminated
    /// and contain no intermediate null characters.
    const DISPLAY_NAME: &'static str;

    /// Initialize the mode.
    ///
    /// # Errors
    ///
    /// This function is allowed to error,
    /// in which case Rofi will display a message:
    ///
    /// ```text
    /// Failed to initialize the mode: {your mode name}
    /// ```
    #[allow(clippy::result_unit_err)]
    fn init(api: Api<'rofi>) -> Result<Self, ()>;

    /// Get the number of entries offered by the mode.
    fn entries(&mut self) -> usize;

    /// Get the text content of a particular entry in the list.
    fn entry_content(&self, line: usize) -> String;

    /// Get the text style of an entry in the list.
    ///
    /// The default implementation returns [`Style::NORMAL`].
    fn entry_style(&self, _line: usize) -> Style {
        Style::NORMAL
    }

    /// Get the text attributes associated with a particular entry in the list.
    ///
    /// The default implementation returns an empty attribute list.
    fn entry_attributes(&self, _line: usize) -> Attributes {
        Attributes::new()
    }

    /// Get the icon of a particular entry in the list, if it has one.
    ///
    /// The default implementation always returns [`None`].
    ///
    /// You can load icons using [`Api::query_icon`].
    fn entry_icon(&mut self, _line: usize, _height: u32) -> Option<cairo::Surface> {
        None
    }

    /// Process the result of a user's selection
    /// in response to them pressing enter, escape etc,
    /// returning the next action to be taken.
    ///
    /// `input` contains the current state of the input text box
    /// and can be mutated to change its contents.
    fn react(&mut self, event: Event, input: &mut String) -> Action;

    /// Find whether a specific line matches the given matcher.
    fn matches(&self, line: usize, matcher: Matcher<'_>) -> bool;

    /// Get the completed value of an entry.
    ///
    /// This is called when the user triggers the `kb-row-select` keybind
    /// (control+space by default)
    /// which sets the content of the input box to the selected item.
    /// It is also used by the sorting algorithm.
    ///
    /// Note that it is _not_ called on an [`Event::Complete`],
    /// [`Self::react`] is called then instead.
    ///
    /// The default implementation forwards to [`Self::entry_content`].
    fn completed(&self, line: usize) -> String {
        self.entry_content(line)
    }

    /// Preprocess the user's input before using it to filter and/or sort.
    ///
    /// This is typically used to strip markup.
    ///
    /// The default implementation returns the input unchanged.
    fn preprocess_input(&mut self, input: &str) -> String {
        input.into()
    }

    /// Get the message to show in the message bar.
    ///
    /// The returned string must be valid [Pango markup].
    ///
    /// The default implementation returns an empty string.
    ///
    /// [Pango markup]: https://docs.gtk.org/Pango/pango_markup.html
    fn message(&mut self) -> String {
        String::new()
    }
}

/// Declare a mode to be exported by this crate.
///
/// This declares a public `#[no_mangle]` static item named `mode`
/// which Rofi reads in from your plugin cdylib.
#[macro_export]
macro_rules! export_mode {
    ($t:ty $(,)?) => {
        #[no_mangle]
        pub static mut mode: $crate::ffi::Mode = $crate::raw_mode::<fn(&()) -> $t>();
    };
}

/// Convert an implementation of [`Mode`] to its raw FFI `Mode` struct.
///
/// You generally do not want to call this function unless you're doing low-level stuff -
/// most of the time the [`export_mode!`] macro is what you want.
///
/// # Panics
///
/// This function panics if the implementation of [`Mode`] is invalid.
#[must_use]
pub const fn raw_mode<T>() -> ffi::Mode
where
    // Workaround to get trait bounds in `const fn` on stable
    <[T; 0] as IntoIterator>::Item: GivesMode,
{
    assert_c_str(<<T as GivesModeLifetime<'_>>::Mode as Mode>::DISPLAY_NAME);
    <RawModeHelper<T>>::VALUE
}

mod sealed {
    use crate::Mode;

    pub trait GivesMode: for<'rofi> GivesModeLifetime<'rofi> {}
    impl<T: ?Sized + for<'rofi> GivesModeLifetime<'rofi>> GivesMode for T {}

    pub trait GivesModeLifetime<'rofi> {
        type Mode: Mode<'rofi>;
    }
    impl<'rofi, F: FnOnce(&'rofi ()) -> O, O: Mode<'rofi>> GivesModeLifetime<'rofi> for F {
        type Mode = O;
    }
}
use sealed::{GivesMode, GivesModeLifetime};

struct RawModeHelper<T>(T);
impl<T: GivesMode> RawModeHelper<T> {
    const VALUE: ffi::Mode = ffi::Mode {
        name: assert_c_str(<<T as GivesModeLifetime<'_>>::Mode as Mode>::NAME),
        _init: Some(init::<T>),
        _destroy: Some(destroy::<T>),
        _get_num_entries: Some(get_num_entries::<T>),
        _result: Some(result::<T>),
        _get_display_value: Some(get_display_value::<T>),
        _token_match: Some(token_match::<T>),
        _get_icon: Some(get_icon::<T>),
        _get_completion: Some(get_completion::<T>),
        _preprocess_input: Some(preprocess_input::<T>),
        _get_message: Some(get_message::<T>),
        ..ffi::Mode::default()
    };
}

const fn assert_c_str(s: &'static str) -> *mut c_char {
    let mut i = 0;
    while i + 1 < s.len() {
        assert!(s.as_bytes()[i] != 0, "string contains intermediary null");
        i += 1;
    }
    assert!(s.as_bytes()[i] == 0, "string is not null-terminated");
    s.as_ptr() as _
}

type ModeOf<'a, T> = <T as GivesModeLifetime<'a>>::Mode;

unsafe extern "C" fn init<T: GivesMode>(sw: *mut ffi::Mode) -> c_int {
    if unsafe { ffi::mode_get_private_data(sw) }.is_null() {
        let api = unsafe { Api::new() };

        let boxed: Box<ModeOf<'_, T>> =
            match catch_panic(|| <ModeOf<'_, T>>::init(api).map(Box::new)) {
                Ok(Ok(boxed)) => boxed,
                Ok(Err(())) | Err(()) => return false.into(),
            };
        let ptr = Box::into_raw(boxed).cast::<c_void>();
        unsafe { ffi::mode_set_private_data(sw, ptr) };

        let display_name = <ModeOf<'_, T>>::DISPLAY_NAME;
        unsafe { (*sw).display_name = glib_sys::g_strdup(display_name.as_ptr().cast()) };
    }
    true.into()
}

unsafe extern "C" fn destroy<T: GivesMode>(sw: *mut ffi::Mode) {
    let ptr = unsafe { ffi::mode_get_private_data(sw) };
    if ptr.is_null() {
        return;
    }
    let boxed = unsafe { <Box<ModeOf<'_, T>>>::from_raw(ptr.cast()) };
    let _ = catch_panic(|| drop(boxed));
    unsafe { ffi::mode_set_private_data(sw, ptr::null_mut()) };
}

unsafe extern "C" fn get_num_entries<T: GivesMode>(sw: *const ffi::Mode) -> c_uint {
    let mode: &mut ModeOf<'_, T> = unsafe { &mut *ffi::mode_get_private_data(sw).cast() };
    catch_panic(|| mode.entries().try_into().unwrap_or(c_uint::MAX)).unwrap_or(0)
}

unsafe extern "C" fn result<T: GivesMode>(
    sw: *mut ffi::Mode,
    mretv: c_int,
    input: *mut *mut c_char,
    selected_line: c_uint,
) -> c_int {
    let mode: &mut ModeOf<'_, T> = unsafe { &mut *ffi::mode_get_private_data(sw).cast() };
    let action = catch_panic(|| {
        let selected = if selected_line == c_uint::MAX {
            None
        } else {
            Some(selected_line as usize)
        };

        let event = match mretv {
            ffi::menu::CANCEL => Event::Cancel { selected },
            _ if mretv & ffi::menu::OK != 0 => Event::Ok {
                alt: mretv & ffi::menu::CUSTOM_ACTION != 0,
                selected: selected.expect("Ok event without selected line"),
            },
            _ if mretv & ffi::menu::CUSTOM_INPUT != 0 => Event::CustomInput {
                alt: mretv & ffi::menu::CUSTOM_ACTION != 0,
                selected,
            },
            ffi::menu::COMPLETE => Event::Complete { selected },
            ffi::menu::ENTRY_DELETE => Event::DeleteEntry {
                selected: selected.expect("DeleteEntry event without selected line"),
            },
            _ if mretv & ffi::menu::CUSTOM_COMMAND != 0 => Event::CustomCommand {
                number: (mretv & ffi::menu::LOWER_MASK) as u8,
                selected,
            },
            _ => panic!("unexpected mretv {mretv:X}"),
        };

        let input: &mut *mut c_char = unsafe { &mut *input };
        let input_ptr: *mut c_char = mem::replace(&mut *input, ptr::null_mut());
        let len = unsafe { libc::strlen(input_ptr) };
        let mut input_string = unsafe { String::from_raw_parts(input_ptr.cast(), len, len + 1) };

        let action = mode.react(event, &mut input_string);

        if !input_string.is_empty() {
            *input = input_string.into_raw().cast::<c_char>();
        }

        action
    })
    .unwrap_or(Action::Exit);

    match action {
        Action::SetMode(mode) => mode.into(),
        Action::Next => ffi::NEXT_DIALOG,
        Action::Previous => ffi::PREVIOUS_DIALOG,
        Action::Reload => ffi::RELOAD_DIALOG,
        Action::Reset => ffi::RESET_DIALOG,
        Action::Exit => ffi::EXIT,
    }
}

unsafe extern "C" fn get_display_value<T: GivesMode>(
    sw: *const ffi::Mode,
    selected_line: c_uint,
    state: *mut c_int,
    attr_list: *mut *mut glib_sys::GList,
    get_entry: c_int,
) -> *mut c_char {
    let mode: &ModeOf<'_, T> = unsafe { &mut *ffi::mode_get_private_data(sw).cast() };
    catch_panic(|| {
        let line = selected_line as usize;

        if !state.is_null() {
            let style = mode.entry_style(line);
            unsafe { *state = style.bits() as c_int };
        }

        if !attr_list.is_null() {
            assert!(unsafe { *attr_list }.is_null());
            let attributes = mode.entry_attributes(line);
            unsafe { *attr_list = ManuallyDrop::new(attributes).list };
        }

        if get_entry == 0 {
            ptr::null_mut()
        } else {
            mode.entry_content(line).into_raw().cast()
        }
    })
    .unwrap_or(ptr::null_mut())
}

unsafe extern "C" fn token_match<T: GivesMode>(
    sw: *const ffi::Mode,
    tokens: *mut *mut ffi::RofiIntMatcher,
    index: c_uint,
) -> c_int {
    let mode: &ModeOf<'_, T> = unsafe { &*ffi::mode_get_private_data(sw).cast() };
    catch_panic(|| {
        let matcher = unsafe { Matcher::from_ffi(tokens) };
        mode.matches(index as usize, matcher)
    })
    .unwrap_or(false)
    .into()
}

unsafe extern "C" fn get_icon<T: GivesMode>(
    sw: *const ffi::Mode,
    selected_line: c_uint,
    height: c_int,
) -> *mut cairo_sys::cairo_surface_t {
    let mode: &mut ModeOf<'_, T> = unsafe { &mut *ffi::mode_get_private_data(sw).cast() };
    catch_panic(|| {
        const NEGATIVE_HEIGHT: &str = "negative height passed into get_icon";

        let height: u32 = height.try_into().expect(NEGATIVE_HEIGHT);

        mode.entry_icon(selected_line as usize, height)
            .map_or_else(ptr::null_mut, |surface| {
                ManuallyDrop::new(surface).to_raw_none()
            })
    })
    .unwrap_or(ptr::null_mut())
}

unsafe extern "C" fn get_completion<T: GivesMode>(
    sw: *const ffi::Mode,
    selected_line: c_uint,
) -> *mut c_char {
    let mode: &ModeOf<'_, T> = unsafe { &mut *ffi::mode_get_private_data(sw).cast() };
    abort_on_panic(|| {
        mode.completed(selected_line as usize)
            .into_raw()
            .cast::<c_char>()
    })
}

unsafe extern "C" fn preprocess_input<T: GivesMode>(
    sw: *mut ffi::Mode,
    input: *const c_char,
) -> *mut c_char {
    let mode: &mut ModeOf<'_, T> = unsafe { &mut *ffi::mode_get_private_data(sw).cast() };
    abort_on_panic(|| {
        let input = unsafe { CStr::from_ptr(input) }
            .to_str()
            .expect("Input is not valid UTF-8");
        let processed = mode.preprocess_input(input);
        if processed.is_empty() {
            ptr::null_mut()
        } else {
            processed.into_raw().cast::<c_char>()
        }
    })
}

unsafe extern "C" fn get_message<T: GivesMode>(sw: *const ffi::Mode) -> *mut c_char {
    let mode: &mut ModeOf<'_, T> = unsafe { &mut *ffi::mode_get_private_data(sw).cast() };
    catch_panic(|| {
        let message = mode.message();
        if message.is_empty() {
            return ptr::null_mut();
        }
        message.into_raw().cast::<c_char>()
    })
    .unwrap_or(ptr::null_mut())
}

struct AbortOnDrop;
impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        process::abort();
    }
}

fn abort_on_panic<O, F: FnOnce() -> O>(f: F) -> O {
    let guard = AbortOnDrop;
    let res = f();
    mem::forget(guard);
    res
}

fn catch_panic<O, F: FnOnce() -> O>(f: F) -> Result<O, ()> {
    panic::catch_unwind(panic::AssertUnwindSafe(f)).map_err(|e| {
        let guard = AbortOnDrop;
        drop(e);
        mem::forget(guard);
    })
}

/// An event triggered by the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    /// The user cancelled the operation, for example by pressing escape.
    Cancel {
        /// The line that was selected at the time of cancellation,
        /// if one was selected.
        selected: Option<usize>,
    },
    /// The user accepted an option from the list (ctrl+j, ctrl+m or enter by default).
    Ok {
        /// Whether the alt binding was used (shift+enter by default).
        alt: bool,
        /// The line that was selected.
        selected: usize,
    },
    /// The user entered an input not on the list (ctrl+return by default).
    CustomInput {
        /// Whether the alt binding was used (ctrl+shift+return by default).
        alt: bool,
        /// The line that was selected at the time of the event,
        /// if one was selected.
        selected: Option<usize>,
    },
    /// The user used the `kb-mode-complete` binding (control+l by default).
    ///
    /// If this happens,
    /// you should set the `input` value
    /// to the currently selected entry
    /// if there is one.
    Complete {
        /// The line that was selected at the time of the event,
        /// if one was selected.
        selected: Option<usize>,
    },
    /// The user used the `kb-delete-entry` binding (shift+delete by default).
    DeleteEntry {
        /// The index of the entry that was selected to be deleted,
        selected: usize,
    },
    /// The user ran a custom command.
    CustomCommand {
        /// The number of the custom cuommand, in the range [0, 18].
        number: u8,
        /// The line that was selected at the time of the event,
        /// if one was selected.
        selected: Option<usize>,
    },
}

impl Event {
    /// Get the index of the line that was selected at the time of the event,
    /// if one was selected.
    #[must_use]
    pub const fn selected(&self) -> Option<usize> {
        match *self {
            Self::Cancel { selected }
            | Self::CustomInput { selected, .. }
            | Self::Complete { selected }
            | Self::CustomCommand { selected, .. } => selected,
            Self::Ok { selected, .. } | Self::DeleteEntry { selected } => Some(selected),
        }
    }
}

/// An action caused by reacting to an [`Event`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Change the active mode to one with the given index.
    ///
    /// The index must be < 1000.
    SetMode(u16),
    /// Switch to the next mode.
    Next,
    /// Switch to the previous mode.
    Previous,
    /// Reload the current mode.
    Reload,
    /// Reset the current mode: this reloads the mode and unsets user input.
    Reset,
    /// Exit Rofi.
    Exit,
}

bitflags! {
    /// The style of a text entry in the list.
    #[derive(Default)]
    pub struct Style: u32 {
        /// The normal style.
        const NORMAL = 0;
        /// The text in the box is urgent.
        const URGENT = 1;
        /// The text in the box is active.
        const ACTIVE = 2;
        /// The text in the box is selected.
        const SELECTED = 4;
        /// The text in the box has [Pango markup].
        ///
        /// [Pango markup]: https://docs.gtk.org/Pango/pango_markup.html
        const MARKUP = 8;

        /// The text is on an alternate row.
        const ALT = 16;
        /// The text has inverted colors.
        const HIGHLIGHT = 32;
    }
}

/// A collection of attributes that can be applied to text.
#[derive(Debug)]
pub struct Attributes {
    list: *mut glib_sys::GList,
}

unsafe impl Send for Attributes {}
unsafe impl Sync for Attributes {}

impl Attributes {
    /// Create a new empty collection of attributes.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            list: ptr::null_mut(),
        }
    }

    /// An an attribute to the list.
    pub fn push<A: Into<pango::Attribute>>(&mut self, attribute: A) {
        let attribute: pango::Attribute = attribute.into();
        // Convert the attribute into its raw form without copying.
        let raw: *mut pango_sys::PangoAttribute = ManuallyDrop::new(attribute).to_glib_none_mut().0;
        self.list = unsafe { glib_sys::g_list_prepend(self.list, raw.cast()) };
    }
}

impl Default for Attributes {
    fn default() -> Self {
        Self::new()
    }
}

impl From<pango::Attribute> for Attributes {
    fn from(attribute: pango::Attribute) -> Self {
        let mut this = Self::new();
        this.push(attribute);
        this
    }
}

impl Drop for Attributes {
    fn drop(&mut self) {
        unsafe extern "C" fn free_attribute(ptr: *mut c_void) {
            unsafe { pango_sys::pango_attribute_destroy(ptr.cast()) }
        }

        unsafe { glib_sys::g_list_free_full(self.list, Some(free_attribute)) };
    }
}

impl<A: Into<pango::Attribute>> Extend<A> for Attributes {
    fn extend<T: IntoIterator<Item = A>>(&mut self, iter: T) {
        iter.into_iter().for_each(|item| self.push(item));
    }
}

impl<A: Into<pango::Attribute>> FromIterator<A> for Attributes {
    fn from_iter<T: IntoIterator<Item = A>>(iter: T) -> Self {
        let mut this = Self::new();
        this.extend(iter);
        this
    }
}

/// A pattern matcher.
#[derive(Debug, Clone, Copy)]
pub struct Matcher<'a> {
    ptr: Option<&'a *mut ffi::RofiIntMatcher>,
}

unsafe impl Send for Matcher<'_> {}
unsafe impl Sync for Matcher<'_> {}

impl Matcher<'_> {
    pub(crate) unsafe fn from_ffi(ffi: *const *mut ffi::RofiIntMatcher) -> Self {
        Self {
            ptr: if ffi.is_null() {
                None
            } else {
                Some(unsafe { &*ffi })
            },
        }
    }

    /// Check whether this matcher matches the given string.
    ///
    /// # Panics
    ///
    /// Panics if the inner string contains null bytes.
    #[must_use]
    pub fn matches(self, s: &str) -> bool {
        let s = CString::new(s).expect("string contains null bytes");
        self.matches_c_str(&*s)
    }

    /// Check whether this matches matches the given C string.
    #[must_use]
    pub fn matches_c_str(self, s: &CStr) -> bool {
        let ptr: *const *mut ffi::RofiIntMatcher = match self.ptr {
            Some(ptr) => ptr,
            None => return true,
        };
        0 != unsafe { ffi::helper::token_match(ptr, s.as_ptr()) }
    }
}
