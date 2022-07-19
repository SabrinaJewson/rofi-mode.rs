use ::{
    cairo::glib::GString,
    std::{
        borrow::Borrow,
        cmp,
        ffi::CStr,
        fmt::{self, Debug, Display, Formatter, Write as _},
        hash::{Hash, Hasher},
        mem::ManuallyDrop,
        ops::Deref,
        ptr, slice, str,
    },
};

#[cfg(not(miri))]
use crate::glib_sys::{
    g_free as free, g_malloc as malloc, g_malloc0_n as calloc, g_realloc as realloc,
};

#[cfg(miri)]
use ::libc::{calloc, free, malloc, realloc};

/// A UTF-8-encoded growable string buffer suitable for FFI with Rofi.
///
/// In constrast to the standard library's [`std::string::String`] type,
/// this string type:
/// - Cannot contain any intermediary nul bytes.
/// - Is always nul-terminated.
/// - Is allocated using glib's allocator
///     (`g_malloc`, `g_realloc` and `g_free`).
///
/// You can use our [`format!`](crate::format!) macro to format these strings,
/// just like with the standard library.
pub struct String {
    ptr: ptr::NonNull<u8>,
    // Doesn't include the nul terminator, so is always < capacity.
    len: usize,
    capacity: usize,
}

const PTR_TO_NULL: *const u8 = &0;

unsafe impl Send for String {}
unsafe impl Sync for String {}

impl String {
    /// Create a new empty string.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            ptr: unsafe { ptr::NonNull::new_unchecked(PTR_TO_NULL as *mut u8) },
            len: 0,
            capacity: 0,
        }
    }

    /// Create a new empty string with at least the specified capacity.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        let mut this = Self::new();
        this.reserve(capacity);
        this
    }

    /// Get the length of the string in bytes,
    /// excluding the nul terminator.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Retrieve whether the string is empty or not.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Obtain the current capacity of the string.
    ///
    /// If the value is zero, no allocation has been made yet.
    #[must_use]
    pub const fn capacity(&self) -> usize {
        self.capacity
    }

    /// Extract a string slice containing the entire string,
    /// excluding the nul terminator.
    ///
    /// This is equivalent to the `Deref` implementation.
    #[must_use]
    pub fn as_str(&self) -> &str {
        unsafe { str::from_utf8_unchecked(slice::from_raw_parts(self.ptr.as_ptr(), self.len)) }
    }

    /// Extract a string slice containing the entire string,
    /// including the nul terminator.
    #[must_use]
    pub fn as_str_nul(&self) -> &str {
        unsafe { str::from_utf8_unchecked(slice::from_raw_parts(self.ptr.as_ptr(), self.len + 1)) }
    }

    /// Construct an owned, allocated string from its raw parts.
    ///
    /// # Safety
    ///
    /// - `capacity` must be nonzero.
    /// - `len` must be < `capacity`.
    /// - `ptr` must be non-null.
    /// - `ptr` must point to the start of
    ///     an allocation in the glib allocator
    ///     of at least `capacity` bytes.
    /// - `ptr` must have provenance over at least `capacity` bytes.
    /// - The first `len` bytes at `*ptr` must be initialized and valid UTF-8,
    ///     and not contain any nul characters.
    /// - The byte at `ptr[len]` must be zero.
    #[must_use]
    pub unsafe fn from_raw_parts(ptr: *mut u8, len: usize, capacity: usize) -> Self {
        debug_assert!(!ptr.is_null());
        debug_assert_ne!(capacity, 0);
        debug_assert!(len < capacity);
        Self {
            ptr: unsafe { ptr::NonNull::new_unchecked(ptr) },
            len,
            capacity,
        }
    }

    /// Take ownership of the string,
    /// giving back a raw pointer to its contents
    /// that can be freed with `g_free`.
    ///
    /// This may allocate if the string has not allocated yet.
    #[must_use]
    pub fn into_raw(self) -> *mut u8 {
        let this = ManuallyDrop::new(self);
        if this.capacity == 0 {
            unsafe { calloc(1, 1) }.cast()
        } else {
            this.ptr.as_ptr()
        }
    }

    /// Reserve `n` bytes of free space in the string.
    ///
    /// `n` doesn't include the nul terminator,
    /// meaning `.reserve(5)` will reserve enough capacity
    /// to push five more bytes of content.
    /// This means that `.reserve(0)` will allocate space for one byte on an empty string.
    pub fn reserve(&mut self, n: usize) {
        // no point in small strings
        const MIN_NON_ZERO_CAP: usize = 8;

        // Use less-than to take into account the nul byte.
        if n < self.capacity - self.len {
            return;
        }

        let min_capacity =
            (|| self.len.checked_add(n)?.checked_add(1))().expect("string length overflowed");
        let new_capacity = Ord::max(self.capacity * 2, min_capacity);
        let new_capacity = Ord::max(MIN_NON_ZERO_CAP, new_capacity);

        let ptr = if self.capacity == 0 {
            let ptr = unsafe { malloc(new_capacity) }.cast::<u8>();
            // Null-terminate the newly-allocated string
            unsafe { *ptr = b'\0' };
            ptr
        } else {
            unsafe { realloc(self.ptr.as_ptr().cast(), new_capacity) }.cast::<u8>()
        };

        self.ptr = ptr::NonNull::new(ptr).expect("glib allocation failed");

        self.capacity = new_capacity;
    }

    /// Push a string onto the end of this string.
    ///
    /// # Panics
    ///
    /// Panics if the string contains intermediary nul bytes.
    pub fn push_str(&mut self, s: &str) {
        assert!(
            !s.as_bytes().contains(&b'\0'),
            "push_str called on string with nuls"
        );

        if s.is_empty() {
            return;
        }

        self.reserve(s.len());
        unsafe {
            ptr::copy_nonoverlapping(s.as_ptr(), self.ptr.as_ptr().add(self.len), s.len());
            self.len += s.len();
            *self.ptr.as_ptr().add(self.len) = b'\0';
        }
    }

    /// Shrinks the capacity of this string with a lower bound.
    ///
    /// The capacity will remain at least as large as both the length and the supplied value.
    ///
    /// If the current capacity is `<=` than the lower limit, this is a no-op.
    pub fn shrink_to(&mut self, min_capacity: usize) {
        let min_capacity = Ord::max(self.len + 1, min_capacity);
        if self.capacity <= min_capacity {
            return;
        }

        // At this point we know that we already have a heap allocation,
        // because if `self.capacity` was 0 the above branch would be taken.

        let ptr = unsafe { realloc(self.ptr.as_ptr().cast(), min_capacity) }.cast::<u8>();

        self.ptr = ptr::NonNull::new(ptr).expect("glib allocation failed");

        self.capacity = min_capacity;
    }

    /// Shrinks the capacity of this string to match its length,
    /// plus one for the nul terminator.
    pub fn shrink_to_fit(&mut self) {
        self.shrink_to(0);
    }

    /// Truncate this string, removing all its contents.
    ///
    /// This does not touch the string's capacity.
    pub fn clear(&mut self) {
        if self.len() > 0 {
            unsafe { *self.ptr.as_ptr() = b'\0' };
            self.len = 0;
        }
    }
}

impl Drop for String {
    fn drop(&mut self) {
        if self.capacity != 0 {
            unsafe { free(self.ptr.as_ptr().cast()) };
        }
    }
}

impl Deref for String {
    type Target = str;
    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

// No `DerefMut` impl because users could write in nul bytes

impl AsRef<str> for String {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl AsRef<CStr> for String {
    fn as_ref(&self) -> &CStr {
        let bytes = self.as_str_nul().as_bytes();
        if cfg!(debug_assertions) {
            CStr::from_bytes_with_nul(bytes).unwrap()
        } else {
            unsafe { CStr::from_bytes_with_nul_unchecked(bytes) }
        }
    }
}

impl Debug for String {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        Debug::fmt(self.as_str(), f)
    }
}

impl Display for String {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        Display::fmt(self.as_str(), f)
    }
}

impl fmt::Write for String {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.push_str(s);
        Ok(())
    }
}

macro_rules! impl_from_stringlike {
    ($($t:ty,)*) => { $(
        impl From<$t> for String {
            fn from(s: $t) -> Self {
                let mut this = Self::new();
                this.push_str(&*s);
                this
            }
        }
    )* };
}
impl_from_stringlike!(
    &String,
    &str,
    &mut str,
    std::string::String,
    &std::string::String,
);

macro_rules! impl_into_std_string {
    ($($t:ty),*) => { $(
        impl From<$t> for std::string::String {
            fn from(string: $t) -> Self {
                std::string::String::from(string.as_str())
            }
        }
    )* }
}
impl_into_std_string!(String, &String, &mut String);

impl From<GString> for String {
    fn from(s: GString) -> Self {
        let len = s.len();
        // We don't know the actual capacity but it doesn't matter,
        // since a lower value is always fine.
        // We also add one for the nul teminator.
        let capacity = len + 1;

        unsafe { Self::from_raw_parts(s.into_raw().cast(), len, capacity) }
    }
}

impl Default for String {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for String {
    fn clone(&self) -> Self {
        Self::from(self.as_str())
    }
}

impl PartialEq for String {
    fn eq(&self, other: &Self) -> bool {
        self.as_str() == other.as_str()
    }
}
impl Eq for String {}

impl PartialOrd for String {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for String {
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        self.as_str().cmp(other.as_str())
    }
}

impl Hash for String {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_str().hash(state);
    }
}

impl Borrow<str> for String {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

impl Extend<char> for String {
    fn extend<T: IntoIterator<Item = char>>(&mut self, iter: T) {
        for c in iter {
            self.push_str(c.encode_utf8(&mut [0; 4]));
        }
    }
}

impl<'a> Extend<&'a char> for String {
    fn extend<T: IntoIterator<Item = &'a char>>(&mut self, iter: T) {
        for c in iter {
            self.push_str(c.encode_utf8(&mut [0; 4]));
        }
    }
}

impl<'a> Extend<&'a str> for String {
    fn extend<T: IntoIterator<Item = &'a str>>(&mut self, iter: T) {
        for s in iter {
            self.push_str(s);
        }
    }
}

impl FromIterator<char> for String {
    fn from_iter<T: IntoIterator<Item = char>>(iter: T) -> Self {
        let mut this = Self::new();
        this.extend(iter);
        this
    }
}

impl<'a> FromIterator<&'a char> for String {
    fn from_iter<T: IntoIterator<Item = &'a char>>(iter: T) -> Self {
        let mut this = Self::new();
        this.extend(iter);
        this
    }
}

impl<'a> FromIterator<&'a str> for String {
    fn from_iter<T: IntoIterator<Item = &'a str>>(iter: T) -> Self {
        let mut this = Self::new();
        this.extend(iter);
        this
    }
}

/// Format a Rofi [`String`] using interpolation of runtime expressions.
///
/// See the documentation of [`std::format!`] for more details.
#[macro_export]
macro_rules! format {
    ($($tt:tt)*) => { $crate::format(::core::format_args!($($tt)*)) };
}

/// Format a Rofi [`String`] using a set of format arguments.
///
/// Usually you will want to use the [`format!`](crate::format!) macro instead of this function.
#[must_use]
pub fn format(args: fmt::Arguments<'_>) -> String {
    let mut s = String::new();
    s.write_fmt(args)
        .expect("a formatting trait implementation returned an error");
    s
}

#[cfg(test)]
mod tests {
    use {super::String, cairo::glib::GString};

    #[test]
    fn empty() {
        let s = String::new();
        assert_eq!(unsafe { *s.ptr.as_ptr() }, b'\0');
        assert_eq!(s.len, 0);
        assert_eq!(s.len(), 0);
        assert!(s.is_empty());
        assert_eq!(s.capacity, 0);
        assert_eq!(s.capacity(), 0);
        assert_eq!(s.as_str(), "");
        assert_eq!(s.as_str_nul(), "\0");
    }

    #[test]
    fn into_raw_allocates() {
        unsafe { super::free(String::new().into_raw().cast()) };
    }

    #[test]
    fn reserve_none() {
        let mut s = String::new();
        s.reserve(0);
        assert!(s.is_empty());
        assert_eq!(s.as_str_nul(), "\0");
        assert_eq!(s.capacity(), 8);
    }

    #[test]
    fn reserve() {
        let mut s = String::new();
        s.reserve(2);

        assert!(s.is_empty());
        assert_eq!(s.as_str_nul(), "\0");
        assert_eq!(s.capacity(), 8);

        s.reserve(7);
        assert_eq!(s.as_str_nul(), "\0");
        assert_eq!(s.capacity(), 8);

        s.reserve(8);
        assert_eq!(s.as_str_nul(), "\0");
        assert_eq!(s.capacity(), 16);
    }

    #[test]
    fn push_str() {
        let mut s = String::new();

        s.push_str("a");
        assert_eq!(s.as_str_nul(), "a\0");
        assert_eq!(s.capacity(), 8);

        s.push_str("bcdefg");
        assert_eq!(s.as_str_nul(), "abcdefg\0");
        assert_eq!(s.capacity(), 8);

        s.push_str("h");
        assert_eq!(s.as_str_nul(), "abcdefgh\0");
        assert_eq!(s.capacity(), 16);
    }

    #[test]
    fn shrink() {
        let mut s = String::new();
        s.shrink_to_fit();
        s.shrink_to(0);
        s.shrink_to(400);
        assert_eq!(s.capacity(), 0);

        s.push_str("foo");

        s.shrink_to(5);
        assert_eq!(s.capacity(), 5);
        assert_eq!(s.as_str_nul(), "foo\0");

        s.shrink_to_fit();
        assert_eq!(s.capacity(), 4);
        assert_eq!(s.as_str_nul(), "foo\0");
    }

    #[test]
    fn clear() {
        let mut s = String::new();
        assert_eq!(s.as_str_nul(), "\0");
        s.clear();
        assert_eq!(s.as_str_nul(), "\0");
        assert_eq!(s.capacity(), 0);

        s.push_str("hello world!");
        s.clear();
        assert_eq!(s.as_str_nul(), "\0");
        assert_eq!(s.capacity(), 13);
    }

    #[test]
    #[cfg(not(miri))]
    fn from_gstring() {
        let s = String::from(GString::from("hello world"));
        assert_eq!(s.as_str(), "hello world");
        assert_eq!(s.as_str_nul(), "hello world\0");
    }

    #[test]
    fn formatting() {
        assert_eq!(format!("PI = {}", 3).as_str_nul(), "PI = 3\0");
    }
}
