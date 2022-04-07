//! Interface to Rofi's API.
#![allow(clippy::unused_self)] // It's needed for the lifetime

use {
    crate::ffi,
    ::std::{
        ffi::{CStr, CString},
        marker::PhantomData,
        os::{raw::c_int, unix::ffi::OsStrExt},
        path::Path,
    },
};

/// The Rofi API,
/// controlled by a lifetime
/// to be only accessible while Rofi is running.
#[derive(Debug)]
pub struct Api<'rofi> {
    lifetime: PhantomData<&'rofi ()>,
}

impl Api<'_> {
    pub(crate) unsafe fn new() -> Self {
        Self {
            lifetime: PhantomData,
        }
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
    /// `name` can also be a full path, if prefixed with `file://`.
    ///
    /// # Panics
    ///
    /// Panics if `name` contains interior null bytes.
    #[must_use]
    pub fn query_icon(&mut self, name: &str, size: u32) -> IconRequest {
        let name = CString::new(name).expect("name contained null bytes");
        self.query_icon_cstr(&*name, size)
    }

    /// Query the icon theme for an icon with a specific name and size.
    ///
    /// `name` can also be a full path, if prefixed with `file://`.
    #[must_use]
    pub fn query_icon_cstr(&mut self, name: &CStr, size: u32) -> IconRequest {
        let uid = unsafe {
            ffi::icon_fetcher::query(name.as_ptr(), size.try_into().unwrap_or(c_int::MAX))
        };
        IconRequest { uid }
    }

    /// Query the icon theme for an icon with a specific name and size.
    ///
    /// `name` can also be a full path, if prefixed with `file://`.
    ///
    /// # Panics
    ///
    /// Panics if `name` contains interior null bytes.
    #[must_use]
    pub fn query_icon_wh(&mut self, name: &str, width: u32, height: u32) -> IconRequest {
        let name = CString::new(name).expect("name contained null bytes");
        self.query_icon_wh_cstr(&*name, width, height)
    }

    /// Query the icon theme for an icon with a specific name and size.
    ///
    /// `name` can also be a full path, if prefixed with `file://`.
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
    #[must_use]
    #[allow(clippy::missing_panics_doc, clippy::needless_pass_by_value)]
    pub fn retrieve_icon(&mut self, request: IconRequest) -> Option<cairo::Surface> {
        let ptr = unsafe { ffi::icon_fetcher::get(request.uid) };
        if ptr.is_null() {
            None
        } else {
            Some(unsafe { cairo::Surface::from_raw_full(ptr) }.unwrap())
        }
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
    /// This is a wrapper around [`Api::retrieve_icon`].
    #[must_use]
    pub fn wait(self, api: &mut Api<'_>) -> Option<cairo::Surface> {
        api.retrieve_icon(self)
    }
}
