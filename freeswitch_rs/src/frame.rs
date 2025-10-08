use crate::types::switch_frame_t;
use std::ffi::c_void;
use std::mem;

/// A wrapper around FreeSWITCH's `switch_frame_t` structure with an associated buffer.
///
/// This structure combines a raw FreeSWITCH frame with a mutable byte buffer,
/// ensuring the buffer remains valid for the lifetime of the frame.
pub struct Frame<'a>(pub(crate) switch_frame_t, pub(crate) &'a mut [u8]);

impl<'a> Frame<'a> {
    /// Creates a new `Frame` initialized with the provided buffer.
    ///
    /// # Examples
    ///
    /// ```
    /// use freeswitch_rs::Frame;
    ///
    /// let mut buffer = vec![0u8; 1024];
    /// let frame = Frame::new(&mut buffer);
    /// ```
    pub fn new(buf: &'a mut [u8]) -> Self {
        let mut f = unsafe { mem::MaybeUninit::<switch_frame_t>::zeroed().assume_init() };
        f.buflen = buf.len().min(u32::MAX as usize) as u32;
        f.data = buf.as_mut_ptr() as *mut c_void;
        Self(f, buf)
    }
}

impl<'a> Frame<'a> {
    /// Returns a reference to the frame's data buffer.
    pub fn data(&'a self) -> &'a [u8] {
        self.1
    }
}
