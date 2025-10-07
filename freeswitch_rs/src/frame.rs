use crate::types::switch_frame_t;
use std::ffi::c_void;
use std::mem;

pub struct Frame<'a>(pub(crate) switch_frame_t, pub(crate) &'a mut [u8]);

impl<'a> Frame<'a> {
    pub fn new(buf: &'a mut [u8]) -> Self {
        let mut f = unsafe { mem::MaybeUninit::<switch_frame_t>::zeroed().assume_init() };
        f.buflen = buf.len().min(u32::MAX as usize) as u32;
        f.data = buf.as_mut_ptr() as *mut c_void;
        Self(f, buf)
    }
}

impl<'a> Frame<'a> {
    pub fn data(&'a self) -> &'a [u8] {
        self.1
    }
}
