use freeswitch_sys::*;
use std::borrow::Borrow;
use std::borrow::BorrowMut;
use std::ffi::c_void;
use std::ffi::CStr;
use std::ffi::CString;
use std::mem;
use std::ops::Deref;
use std::ptr;

use crate::utils::call_with_meta;
use crate::utils::FSHandle;
use crate::utils::FSScopedHandle;
use crate::utils::FSScopedHandleMut;

// ------------

#[derive(Debug, Clone, PartialEq)]
pub enum SessionError {
    AllocationError,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SessionUUID(String);

// ------------

pub type Session<'a> = FSScopedHandle<'a, switch_core_session_t>;

// ------------

pub struct LocateGuard<'a> {
    s: Session<'a>,
}

impl<'a> Drop for LocateGuard<'a> {
    fn drop(&mut self) {
        // SAFETY: In theory this is safe because ptr is valid since we located Session
        // to create Session struct
        unsafe {
            if !self.s.ptr.is_null() {
                switch_core_session_rwunlock(self.s.ptr)
            }
        }
    }
}

impl<'a> Deref for LocateGuard<'a> {
    type Target = Session<'a>;
    fn deref(&self) -> &Self::Target {
        &self.s
    }
}

// ------------
impl<'a> Session<'a> {
    pub fn locate(id: &str) -> Option<LocateGuard<'static>> {
        let s: CString = CString::new(id.to_owned()).unwrap();

        // SAFETY
        // Locating will take a read lock of any found session
        // which we model as 'static shared reference ( via scoped handle )
        unsafe {
            let ptr = call_with_meta!(switch_core_session_perform_locate, s.as_ptr());
            if ptr.is_null() {
                None
            } else {
                let s = Session::from_raw(ptr);
                Some(LocateGuard { s })
            }
        }
    }
}

impl<'a> Session<'a> {
    pub fn insert<T: Sized + 'static>(&self, data: T) -> Result<FSHandle<T>, SessionError> {
        // SAFETY:
        unsafe {
            let ptr = call_with_meta!(
                switch_core_perform_session_alloc,
                self.ptr,
                std::mem::size_of::<T>()
            );

            if !ptr.is_null() {
                let p = ptr.cast::<T>();
                let r = &mut *p;
                _ = std::mem::replace(r, data);
                Ok(FSHandle { ptr: p })
            } else {
                Err(SessionError::AllocationError)
            }
        }
    }

    // SAFETY:
    // The api requires an valid session ptr in order to read back the ptrs
    // and also tie the life time to hence proving the session pool is still
    // valid

    pub fn get<T>(&'a self, k: &FSHandle<T>) -> Option<FSScopedHandle<'a, T>> {
        Some(FSScopedHandle::from_raw(k.ptr))
    }

    pub fn get_mut<T>(&'a mut self, k: &FSHandle<T>) -> Option<FSScopedHandleMut<'a, T>> {
        Some(FSScopedHandleMut::from_raw(k.ptr))
    }

    pub fn get_channel(&self) -> Channel<'_> {
        unsafe {
            let c = switch_core_session_get_channel(self.ptr);
            FSScopedHandleMut::from_raw(c)
        }
    }

    // this will consume the handle... good!
    pub fn remove_media_bug(&self, mut bug: MediaBugHandle) {
        if bug.ptr.is_null() {
            return;
        }
        unsafe {
            let p: *mut *mut switch_media_bug_t = &mut bug.ptr;
            switch_core_media_bug_remove(self.ptr, p);
        }
    }

    pub fn add_media_bug<F>(
        &self,
        name: String,
        target: String,
        flags: switch_media_bug_flag_t,
        callback: F,
    ) -> Result<MediaBugHandle, SessionError>
    where
        F: FnMut(MediaBug, switch_abc_type_t) -> bool + 'static + Send,
    {
        let data = Box::into_raw(Box::new(callback));
        let func = CString::new(name).unwrap();
        let target = CString::new(target).unwrap();

        let mut bug: *mut switch_media_bug_t = ptr::null_mut();

        // SAFETY:
        unsafe {
            let res = switch_core_media_bug_add(
                self.ptr,
                func.as_ptr(),
                target.as_ptr(),
                Some(MediaBug::callback::<F>),
                data as *mut c_void,
                0,
                flags,
                &mut bug as *mut *mut switch_media_bug_t,
            );
            if res == switch_status_t::SWITCH_STATUS_SUCCESS && !bug.is_null() {
                let h = MediaBugHandle { ptr: bug };
                Ok(h)
            } else {
                Err(SessionError::AllocationError)
            }
        }
    }
}
// =====

pub type Channel<'a> = FSScopedHandleMut<'a, switch_channel_t>;

//struct ChannelRecord(*mut c_void, std::any::TypeId);

impl<'a> Channel<'a> {
    pub fn set_private<T>(&mut self, key: &'static CStr, data: FSHandle<T>)
    where
        T: Sized,
    {
        unsafe {
            switch_channel_set_private(self.ptr, key.as_ptr(), data.ptr as *mut c_void);
        }
    }

    pub fn get_private_unsafe<T>(&mut self, key: &'static CStr) -> Option<FSHandle<T>> {
        unsafe {
            let ptr = switch_channel_get_private(self.ptr, key.as_ptr());
            if ptr.is_null() {
                return None;
            }
            Some(FSHandle { ptr: ptr as *mut T })
        }
    }
}

// =====
pub type MediaBugHandle = FSHandle<switch_media_bug_t>;
pub type MediaBug<'a> = FSScopedHandle<'a, switch_media_bug_t>;
impl<'a> MediaBug<'a> {
    unsafe extern "C" fn callback<F>(
        arg1: *mut switch_media_bug_t,
        arg2: *mut ::std::os::raw::c_void,
        arg3: switch_abc_type_t,
    ) -> switch_bool_t
    where
        F: FnMut(MediaBug, switch_abc_type_t) -> bool,
    {
        let callback_ptr = arg2 as *mut F;
        let callback = &mut *callback_ptr;
        let bug = MediaBug::from_raw(arg1);

        let res = if callback(bug, arg3) {
            switch_bool_t_SWITCH_TRUE
        } else {
            switch_bool_t_SWITCH_FALSE
        };

        if arg3 == switch_abc_type_t::SWITCH_ABC_TYPE_CLOSE {
            let _ = Box::from_raw(arg2);
        }
        res
    }

    pub fn get_session(&self) -> Session<'_> {
        // SAFETY
        // Media bug lifetime is tied to session, so should be safe
        // assuming media bug ptr is ok.
        // life time of session handle will be tied to bug
        // + assumingly you will be calling method within session thread
        unsafe {
            let ptr = switch_core_media_bug_get_session(self.ptr);
            Session::from_raw(ptr)
        }
    }

    // The FS api doesn't offer much inspection of the error when calling
    // switch_core_media_bug_read - so how to understand IF its error or just not ready to read?
    // seems like a generic error value based on switch_status_t is the best we can do ?
    pub fn read_frame(&mut self, buf: &mut [u8]) -> Result<usize, switch_status_t> {
        let mut f = unsafe { mem::MaybeUninit::<switch_frame_t>::zeroed().assume_init() };
        f.data = buf.as_mut_ptr() as *mut c_void;
        f.buflen = buf.len().try_into().unwrap();
        let res = unsafe { switch_core_media_bug_read(self.ptr, &mut f, 1) };
        if res != switch_status_t::SWITCH_STATUS_SUCCESS {
            Err(res)
        } else {
            Ok(f.datalen.try_into().unwrap())
        }
    }
}

// ====
// TODO: import properly
pub const SWITCH_RECOMMENDED_BUFFER_SIZE: usize = 8192;
pub struct FrameBuffer([u8; SWITCH_RECOMMENDED_BUFFER_SIZE]);

impl Default for FrameBuffer {
    fn default() -> Self {
        Self([0; SWITCH_RECOMMENDED_BUFFER_SIZE])
    }
}

impl Borrow<[u8]> for FrameBuffer {
    fn borrow(&self) -> &[u8] {
        &self.0
    }
}

impl BorrowMut<[u8]> for FrameBuffer {
    fn borrow_mut(&mut self) -> &mut [u8] {
        &mut self.0
    }
}
