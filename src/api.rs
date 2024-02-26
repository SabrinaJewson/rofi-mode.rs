//! Interface to Rofi's API.
#![allow(clippy::unused_self)] // It's needed for the lifetime

/// The Rofi API,
/// controlled by a lifetime
/// to be only accessible while Rofi is running.
#[derive(Debug)]
pub struct Api<'rofi> {
    display_name: ptr::NonNull<*mut u8>,
    // Values are irrelevant when `*display_name == NULL`
    display_name_len: usize,
    display_name_capacity: usize,
    lifetime: PhantomData<&'rofi ()>,
}

// SAFETY: All the methods take `&self` or `&mut self` appropriately to enforce thread-safety.
// Additionally, this type's lifetime ensures that it can't be used on a separate thread outside of
// when `Mode`'s methods run (since scoped threads only work inside a scope).
unsafe impl Send for Api<'_> {}
unsafe impl Sync for Api<'_> {}

impl Api<'_> {
    pub(crate) unsafe fn new(display_name: ptr::NonNull<*mut u8>) -> Self {
        Self {
            display_name,
            display_name_len: 0,
            display_name_capacity: 0,
            lifetime: PhantomData,
        }
    }

    /// Get the display name of the current mode (the text displayed before the colon).
    ///
    /// Returns [`None`] if there isn't one,
    /// in which case Rofi shows the [mode name] instead.
    ///
    /// [mode name]: crate::Mode::NAME
    #[must_use]
    pub fn display_name(&self) -> Option<&str> {
        // SAFETY: Rofi never mutates the display name, and we only mutate it with an `&mut Api`.
        let ptr = *unsafe { self.display_name.as_ref() };

        if ptr.is_null() {
            return None;
        }

        let slice = unsafe { slice::from_raw_parts(ptr, self.display_name_len) };

        Some(unsafe { str::from_utf8_unchecked(slice) })
    }

    fn change_display_name(&mut self, display_name: Option<String>) -> Option<String> {
        // SAFETY: In order for functions on this type to be called, we must be inside one of
        // `Mode`'s methods. This means that Rofi guarantees us it won't be reading the display
        // name at this point in time.
        let ptr = unsafe { self.display_name.as_mut() };

        let old_len = self.display_name_len;
        let old_capacity = self.display_name_capacity;
        let old_ptr = *ptr;

        if let Some(display_name) = &display_name {
            self.display_name_len = display_name.len();
            self.display_name_capacity = display_name.capacity();
        }
        *ptr = display_name.map_or_else(ptr::null_mut, String::into_raw);

        if old_ptr.is_null() {
            None
        } else {
            Some(unsafe { String::from_raw_parts(old_ptr, old_len, old_capacity) })
        }
    }

    /// Take the current display name,
    /// leaving [`None`] in its place
    /// and returning the previous display name.
    /// This will cause Rofi to display the [mode name](crate::Mode::NAME) instead.
    ///
    /// Returns [`None`] if there was no previous display name.
    pub fn take_display_name(&mut self) -> Option<String> {
        self.change_display_name(None)
    }

    /// Replace the current display name,
    /// returning the previous one.
    ///
    /// Returns [`None`] if there was no previous display name.
    pub fn replace_display_name(&mut self, display_name: String) -> Option<String> {
        self.change_display_name(Some(display_name))
    }

    /// Set the display name of the current mode.
    ///
    /// # Panics
    ///
    /// Panics if the given string contains any interior nul bytes.
    pub fn set_display_name<T: Display>(&mut self, display_name: T) {
        let mut buf = self.take_display_name().unwrap_or_default();
        buf.clear();
        write!(buf, "{display_name}").unwrap();
        self.replace_display_name(buf);
    }

    /// Check whether the given file path is an image in one of Rofi's supported formats,
    /// by looking at its file extension.
    #[must_use]
    pub fn supports_image<P: AsRef<Path>>(&self, path: P) -> bool {
        let mut path = path.as_ref().as_os_str().as_bytes().to_owned();
        path.push(b'\0');

        let res = unsafe { ffi::icon_fetcher::file_is_image(path.as_ptr().cast()) };

        res != 0
    }

    /// Query the icon theme for an icon with a specific name and size.
    ///
    /// `name` can also be a full path.
    ///
    /// # Panics
    ///
    /// Panics if `name` contains interior nul bytes.
    #[must_use]
    pub fn query_icon(&mut self, name: &str, size: u32) -> IconRequest {
        let name = CString::new(name).expect("name contained nul bytes");
        self.query_icon_cstr(&name, size)
    }

    /// Query the icon theme for an icon with a specific name and size.
    ///
    /// `name` can also be a full path.
    #[must_use]
    pub fn query_icon_cstr(&mut self, name: &CStr, size: u32) -> IconRequest {
        let uid = unsafe {
            ffi::icon_fetcher::query(name.as_ptr(), size.try_into().unwrap_or(c_int::MAX))
        };
        IconRequest { uid }
    }

    /// Query the icon theme for an icon with a specific name and size.
    ///
    /// `name` can also be a full path.
    ///
    /// # Panics
    ///
    /// Panics if `name` contains interior nul bytes.
    #[must_use]
    pub fn query_icon_wh(&mut self, name: &str, width: u32, height: u32) -> IconRequest {
        let name = CString::new(name).expect("name contained nul bytes");
        self.query_icon_wh_cstr(&name, width, height)
    }

    /// Query the icon theme for an icon with a specific name and size.
    ///
    /// `name` can also be a full path.
    #[must_use]
    pub fn query_icon_wh_cstr(&mut self, name: &CStr, width: u32, height: u32) -> IconRequest {
        let uid = unsafe {
            ffi::icon_fetcher::query_advanced(
                name.as_ptr(),
                width.try_into().unwrap_or(c_int::MAX),
                height.try_into().unwrap_or(c_int::MAX),
            )
        };
        IconRequest { uid }
    }

    /// Finalize an icon request and retrieve the inner icon.
    ///
    /// The returned icon will be the best match for the requested size,
    /// but you may need to resize it to desired size.
    ///
    /// It may be ergonomically preferable to use [`IconRequest::wait`] instead of this function.
    ///
    /// # Errors
    ///
    /// Errors if the icon was not found, or an error occurred inside the returned Cairo surface.
    #[allow(clippy::needless_pass_by_value)]
    pub fn retrieve_icon(&mut self, request: IconRequest) -> Result<cairo::Surface, IconError> {
        let ptr = unsafe { ffi::icon_fetcher::get(request.uid) };
        if ptr.is_null() {
            return Err(IconError::NotFound);
        }
        unsafe { cairo::Surface::from_raw_full(ptr) }.map_err(IconError::Surface)
    }
}

/// A request sent to the icon fetcher.
///
/// This can be finalized using [`Api::retrieve_icon`].
#[derive(Debug)]
pub struct IconRequest {
    uid: u32,
}

impl IconRequest {
    /// Wait for the request to be fulfilled.
    ///
    /// This is a wrapper around [`Api::retrieve_icon`] — see that method for more.
    #[allow(clippy::missing_errors_doc)]
    pub fn wait(self, api: &mut Api<'_>) -> Result<cairo::Surface, IconError> {
        api.retrieve_icon(self)
    }
}

/// An error retrieving an icon.
#[derive(Debug)]
pub enum IconError {
    /// The icon was not found.
    #[non_exhaustive]
    NotFound,
    /// An error occurred inside the Cairo surface.
    #[non_exhaustive]
    Surface(cairo::Error),
}

impl Display for IconError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str("failed to retrieve icon")
    }
}

impl Error for IconError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::NotFound => Some(&IconNotFound),
            Self::Surface(e) => Some(e),
        }
    }
}

#[derive(Debug)]
struct IconNotFound;

impl Display for IconNotFound {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str("icon not found")
    }
}

impl Error for IconNotFound {}

use crate::ffi;
use crate::String;
use std::error::Error;
use std::ffi::CStr;
use std::ffi::CString;
use std::fmt;
use std::fmt::Display;
use std::fmt::Formatter;
use std::fmt::Write as _;
use std::marker::PhantomData;
use std::os::raw::c_int;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::ptr;
use std::slice;
use std::str;
