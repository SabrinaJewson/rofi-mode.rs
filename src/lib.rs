//! `rofi-mode` provides a high-level ergonomic wrapper around Rofi's C plugin API.
//!
//! # Getting started
//!
//! First of all,
//! create a new library with `cargo new --lib my_awesome_plugin`.
//! In its `Cargo.toml`, make sure to put this:
//!
//! ```toml
//! [lib]
//! crate-type = ["cdylib"]
//! ```
//!
//! That will force Cargo to generate your library as a `.so` file,
//! which is which Rofi loads its plugins from.
//!
//! Now in your `lib.rs`,
//! create a struct and implement the [`Mode`] trait for it.
//!
//! [`Mode`]: https://docs.rs/rofi-mode/latest/rofi_mode/trait.Mode.html
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
    pango::{
        ffi as pango_sys,
        glib::{ffi as glib_sys, translate::ToGlibPtrMut},
    },
    std::{
        ffi::{c_void, CString},
        marker::PhantomData,
        mem::{self, ManuallyDrop},
        os::raw::{c_char, c_int, c_uint},
        panic, process, ptr,
    },
};

pub use rofi_plugin_sys as ffi;

pub use pango;

mod string;
pub use string::String;

/// A mode supported by Rofi.
///
/// You can implement this trait on your own type.
pub trait Mode: 'static + Sized + Send + Sync {
    /// The name of the mode.
    ///
    /// This string must be null-terminated
    /// and contain no intermediate null characters.
    const NAME: &'static str;

    /// The display name of the mode.
    ///
    /// This must be null-terminated,
    /// <= 128 bytes long (including the terminator)
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
    fn init() -> Result<Self, ()>;

    /// Get the number of entries offered by the mode.
    fn entries(&self) -> usize;

    /// Get the text style of an entry in the list.
    fn entry_style(&self, line: usize) -> Style;

    /// Get the style and value to display of an entry in the list.
    fn entry(&self, line: usize) -> (Style, Attributes, String);

    /// Process the result of a user's selection
    /// in response to them pressing enter, escape etc,
    /// returning the next action to be taken.
    fn react(&mut self, event: Event, input: &mut String, selected_line: usize) -> Action;

    /// Find whether a specific line matches the given matcher.
    fn matches(&self, line: usize, matcher: Matcher<'_>) -> bool;
}

/// Declare a mode to be exported by this crate.
#[macro_export]
macro_rules! export_mode {
    ($t:ty $(,)?) => {
        #[no_mangle]
        pub static mut mode: $crate::ffi::Mode = $crate::raw_mode::<$t>();
    };
}

/// Convert an implementation of [`Mode`] to its raw FFI `Mode` struct.
#[must_use]
pub const fn raw_mode<T>() -> ffi::Mode
where
    // Workaround to get trait bounds in `const fn` on stable
    <[T; 0] as IntoIterator>::Item: Mode,
{
    <RawModeHelper<T>>::VALUE
}

struct RawModeHelper<T>(T);
impl<T: Mode> RawModeHelper<T> {
    const VALUE: ffi::Mode = ffi::Mode {
        name: assert_c_str(T::NAME),
        _init: Some(init::<T>),
        cfg_name_key: {
            let s = T::DISPLAY_NAME;
            assert_c_str(s);
            display_name(s)
        },
        _destroy: Some(destroy::<T>),
        _get_num_entries: Some(get_num_entries::<T>),
        _result: Some(result::<T>),
        _get_display_value: Some(get_display_value::<T>),
        _token_match: Some(token_match::<T>),
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

const fn display_name(s: &'static str) -> [c_char; 128] {
    assert!(s.len() <= 128, "string is longer than 128 bytes");
    let mut buf = [0; 128];
    let mut i = 0;
    while i < s.len() {
        buf[i] = s.as_bytes()[i] as _;
        i += 1;
    }
    buf
}

unsafe extern "C" fn init<T: Mode>(sw: *mut ffi::Mode) -> c_int {
    if unsafe { ffi::mode_get_private_data(sw) }.is_null() {
        let boxed: Box<T> = match catch_panic(|| T::init().map(Box::new)) {
            Ok(Ok(boxed)) => boxed,
            Ok(Err(())) | Err(()) => return false.into(),
        };
        let ptr = Box::into_raw(boxed).cast::<c_void>();
        unsafe { ffi::mode_set_private_data(sw, ptr) };
    }
    true.into()
}

unsafe extern "C" fn destroy<T: Mode>(sw: *mut ffi::Mode) {
    let ptr = unsafe { ffi::mode_get_private_data(sw) };
    if ptr.is_null() {
        return;
    }
    let boxed = unsafe { <Box<T>>::from_raw(ptr.cast()) };
    let _ = catch_panic(|| drop(boxed));
    unsafe { ffi::mode_set_private_data(sw, ptr::null_mut()) };
}

unsafe extern "C" fn get_num_entries<T: Mode>(sw: *const ffi::Mode) -> c_uint {
    let mode: &T = unsafe { &*ffi::mode_get_private_data(sw).cast() };
    catch_panic(|| mode.entries().try_into().unwrap_or(c_uint::MAX)).unwrap_or(0)
}

unsafe extern "C" fn result<T: Mode>(
    sw: *mut ffi::Mode,
    mretv: c_int,
    input: *mut *mut c_char,
    selected_line: c_uint,
) -> c_int {
    let mode: &mut T = unsafe { &mut *ffi::mode_get_private_data(sw).cast() };
    let action = catch_panic(|| {
        let event = match mretv {
            ffi::menu::CANCEL => Event::Cancel,
            _ if mretv & ffi::menu::OK != 0 => Event::Ok {
                alt: mretv & ffi::menu::CUSTOM_ACTION != 0,
            },
            _ if mretv & ffi::menu::CUSTOM_INPUT != 0 => Event::CustomInput {
                alt: mretv & ffi::menu::CUSTOM_ACTION != 0,
            },
            ffi::menu::COMPLETE => Event::Complete,
            ffi::menu::ENTRY_DELETE => Event::DeleteEntry,
            _ if mretv & ffi::menu::CUSTOM_COMMAND != 0 => {
                Event::CustomCommand((mretv & ffi::menu::LOWER_MASK) as u8)
            }
            _ => panic!("unexpected mretv {mretv:X}"),
        };

        let input: &mut *mut c_char = unsafe { &mut *input };
        let input_ptr: *mut c_char = mem::replace(&mut *input, ptr::null_mut());
        let len = unsafe { libc::strlen(input_ptr) };
        let mut input_string = unsafe { String::from_raw_parts(input_ptr.cast(), len, len) };

        let action = mode.react(event, &mut input_string, selected_line as usize);

        *input = input_string.into_raw().cast::<c_char>();

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

unsafe extern "C" fn get_display_value<T: Mode>(
    sw: *const rofi_plugin_sys::Mode,
    selected_line: c_uint,
    state: *mut c_int,
    attr_list: *mut *mut glib_sys::GList,
    get_entry: c_int,
) -> *mut c_char {
    let mode: &T = unsafe { &*ffi::mode_get_private_data(sw).cast() };
    catch_panic(|| {
        if get_entry == 0 {
            let style = mode.entry_style(selected_line as usize);
            unsafe { *state = style.bits() as _ };
            ptr::null_mut()
        } else {
            let (style, attributes, content) = mode.entry(selected_line as usize);
            unsafe {
                *state = style.bits() as _;
                *attr_list = ManuallyDrop::new(attributes).list;
                content.into_raw().cast()
            }
        }
    })
    .unwrap_or(ptr::null_mut())
}

unsafe extern "C" fn token_match<T: Mode>(
    sw: *const rofi_plugin_sys::Mode,
    tokens: *mut *mut rofi_plugin_sys::RofiIntMatcher,
    index: c_uint,
) -> c_int {
    let mode: &T = unsafe { &*ffi::mode_get_private_data(sw).cast() };
    catch_panic(|| {
        let matcher = unsafe { Matcher::from_ffi(tokens) };
        mode.matches(index as usize, matcher)
    })
    .unwrap_or(false)
    .into()
}

fn catch_panic<O, F: FnOnce() -> O>(f: F) -> Result<O, ()> {
    struct AbortOnDrop;
    impl Drop for AbortOnDrop {
        fn drop(&mut self) {
            process::abort();
        }
    }

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
    Cancel,
    /// The user accepted an option from the list (ctrl+j, ctrl+m or enter by default).
    Ok {
        /// Whether the alt binding was used (shift+enter by default).
        alt: bool,
    },
    /// The user entered an input not on the list (ctrl+return by default).
    CustomInput {
        /// Whether the alt binding was used (ctrl+shift+return by default).
        alt: bool,
    },
    /// The user used the `kb-mode-complete` binding (control+l by default).
    Complete,
    /// The user used the `kb-delete-entry` binding (shift+delete by default).
    DeleteEntry,
    /// The user ran a custom command. The given integer is in the range [0, 18].
    CustomCommand(u8),
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
        /// The text in the box has Pango markup.
        const MARKUP = 8;

        /// The text is on an alternate row.
        const ALT = 16;
        /// The text has inverted colors.
        const HIGHTLIGHT = 32;
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
    /// Create a collection of attributes.
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
    lifetime: PhantomData<&'a *mut ffi::RofiIntMatcher>,
}

unsafe impl Send for Matcher<'_> {}
unsafe impl Sync for Matcher<'_> {}

impl<'a> Matcher<'a> {
    pub(crate) unsafe fn from_ffi(ffi: *const *mut ffi::RofiIntMatcher) -> Self {
        Self {
            ptr: if ffi.is_null() {
                None
            } else {
                Some(unsafe { &*ffi })
            },
            lifetime: PhantomData,
        }
    }
}

impl Matcher<'_> {
    /// Check whether this matcher matches the given string.
    ///
    /// # Panics
    ///
    /// Panics if the inner string contains null bytes.
    #[must_use]
    pub fn matches(self, s: &str) -> bool {
        let ptr: *const *mut ffi::RofiIntMatcher = match self.ptr {
            Some(ptr) => ptr,
            None => return true,
        };
        let c_string = CString::new(s).expect("string contains null bytes");
        0 != unsafe { ffi::helper::token_match(ptr, c_string.as_ptr()) }
    }
}
