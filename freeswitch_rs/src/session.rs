use freeswitch_sys::*;
use std::borrow::Borrow;
use std::borrow::BorrowMut;
use std::borrow::Cow;
use std::ffi::c_void;
use std::fmt::Pointer;
use std::marker::PhantomData;
use std::mem;
use std::ops::Deref;
use std::ops::DerefMut;
use std::ptr;
use std::ffi::CString;

use crate::utils::FSObject;
use crate::utils::FSObjectMut;
use crate::utils::FSHandle;
pub struct SessionData<T>(T);

unsafe impl<T> Send for SessionData<T> where T:Send {}

impl <'a,T> AsRef<T> for FSObject<'a,SessionData<T>> {
    fn as_ref(&self) -> &T {
        unsafe { 
            &((*self.ptr).0)
        }
    }
}

impl <'a,T> AsMut<T> for FSObjectMut<'a,SessionData<T>> {
    fn as_mut(&mut self) -> &mut T {
        unsafe { 
            &mut((*self.ptr).0)
        }
    }
}

// =====

pub enum SessionError {
    AllocationError
}

#[derive(Debug, Clone, PartialEq)]
pub struct SessionUUID(String);

pub type Session<'a> = FSObject<'a, switch_core_session_t>;

// ===========
pub struct LocateGuard<'a> {
    s: Session<'a> 
}

impl<'a> Drop for LocateGuard<'a> {
    fn drop(&mut self) {
        // SAFETY: In theory this is safe because ptr is valid since we located Session
        // to create Session struct 
        unsafe { 
            if !self.s.ptr.is_null() { switch_core_session_rwunlock(self.s.ptr) }
        }
    }
}

impl<'a> Deref for LocateGuard<'a> {
    type Target = Session<'a>;
    fn deref(&self) -> &Self::Target {
        &self.s
    }
}


impl<'a> Session<'a> {
    pub fn locate(id:&str) -> Option<LocateGuard<'static>> {
        let file = CString::new(std::file!()).unwrap();
        let line = std::line!().try_into().unwrap_or(0);
        let func = CString::new("").unwrap();
        let s:CString = CString::new(id.to_owned()).unwrap();

        // SAFETY: 
        unsafe {
            let ptr = switch_core_session_perform_locate(s.as_ptr(), file.as_ptr(), func.as_ptr(), line);
            if ptr.is_null() { None }
            else { 
                let s = Session::from_raw(ptr);
                Some(LocateGuard { s })
            }
        }
    }
}

impl<'a> Session<'a> {
    pub fn insert<T:Sized + 'static>(&self, data:T) -> Result<FSHandle<SessionData<T>>, SessionError> {
            let file = CString::new(std::file!()).unwrap();
            let line = std::line!().try_into().unwrap_or(0);
            let func = CString::new("").unwrap();

        // SAFETY: 
        unsafe {
            let ptr = switch_core_perform_session_alloc(
                self.ptr, std::mem::size_of::<SessionData<T>>(), 
                file.as_ptr(),
                func.as_ptr(),
                line
            );
            if !ptr.is_null(){
                let p = ptr.cast::<SessionData<T>>();
                let r = &mut *p;
                _ = std::mem::replace(r, SessionData(data));
                Ok(FSHandle{ ptr: p})
            }
            else {
                Err(SessionError::AllocationError)
            }
        }
    }

    pub fn get<T>(&self, k:&FSHandle<T>) -> Option<FSObject<T>> {
        // This should have same life time as &self right?
        Some(FSObject::from_raw(k.ptr))

    }

    pub fn get_mut<T>(&mut self, k:&FSHandle<T>) -> Option<FSObjectMut<T>> {
        Some(FSObjectMut::from_raw(k.ptr))
    }

    pub fn add_media_bug<F>(&self, name:String, target:String, flags: switch_media_bug_flag_t, callback:F) 
        -> Result<MediaBugHandle, SessionError>
        where F: FnMut(MediaBug, switch_abc_type_t) -> bool + 'static + Send,
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
                &mut bug as *mut *mut switch_media_bug_t
            );
            if res == switch_status_t::SWITCH_STATUS_SUCCESS && !bug.is_null() {
                let h = MediaBugHandle { ptr: bug };
                Ok(h)
            }
            else {
                Err(SessionError::AllocationError)
            }
        }
    }
}

// =====
pub type MediaBugHandle = FSHandle<switch_media_bug_t>;
pub type MediaBug<'a> = FSObject<'a,switch_media_bug_t>;
impl <'a> MediaBug<'a> {

    unsafe extern "C" fn callback<F>(arg1: *mut switch_media_bug_t, arg2: *mut ::std::os::raw::c_void, arg3: switch_abc_type_t) -> switch_bool_t
    where F: FnMut(MediaBug, switch_abc_type_t) -> bool,
    {
        let callback_ptr = arg2 as *mut F;
        let callback = &mut *callback_ptr;
        let bug = MediaBug::from_raw(arg1);

        let res = if callback(bug, arg3) {
            switch_bool_t_SWITCH_TRUE
        } else {
            switch_bool_t_SWITCH_FALSE
        };
        
        if arg3 == switch_abc_type_t::SWITCH_ABC_TYPE_CLOSE{
            let _ = Box::from_raw(arg2);
        }
        res
    }

    pub fn get_session(&self) -> Session {
        unsafe {
            let ptr = switch_core_media_bug_get_session(self.ptr);
            Session::from_raw(ptr)
        }
    }

    pub fn remove(mut self) {
        let s = self.get_session();
        unsafe {
            switch_core_media_bug_remove(s.ptr, &mut self.ptr);
        }
    }

    // The FS api doesn't offer much inspection of the error when calling
    // switch_core_media_bug_read - so how to understand IF its error or just not ready to read? 
    // seems like a generic error value based on switch_status_t is the best we can do ?
    pub fn read_frame(&mut self, buf:&mut[u8])  -> Result<usize,()> {
       let mut f = unsafe { mem::MaybeUninit::<switch_frame_t>::zeroed().assume_init() };
       f.data = buf.as_mut_ptr() as *mut c_void;
       f.buflen = buf.len().try_into().unwrap();
       let res = unsafe { switch_core_media_bug_read(self.ptr, &mut f, 1) };
       if res != switch_status_t::SWITCH_STATUS_SUCCESS {
            Err(())
       }
       else {
            Ok(f.datalen.try_into().unwrap())
       }
    }
}


// ====
// TODO: import properly 
pub const SWITCH_RECOMMENDED_BUFFER_SIZE:usize = 8192;
pub struct FrameBuffer([u8;SWITCH_RECOMMENDED_BUFFER_SIZE]);

impl Default for FrameBuffer {
    fn default() -> Self {
       Self([0;SWITCH_RECOMMENDED_BUFFER_SIZE])
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

