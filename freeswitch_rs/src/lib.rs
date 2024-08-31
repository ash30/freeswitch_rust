pub mod fslog;
pub use fslog::FSLogger;
pub use log;
pub use fslog::SWITCH_CHANNEL_ID_LOG_CLEAN;
pub use fslog::SWITCH_CHANNEL_ID_LOG;
pub use fslog::SWITCH_CHANNEL_ID_EVENT;
pub use fslog::SWITCH_CHANNEL_ID_SESSION;

use std::arch::aarch64::int64x1_t;
use std::ffi::c_void;
use std::ops::Deref;
use std::ptr;
use std::{ffi::CString, marker::PhantomData};
use freeswitch_sys::*;

pub use freeswitch_rs_macros::switch_module_define;
pub use freeswitch_rs_macros::switch_api_define;

// We will need a macro to transform trait into extern C functions ....
// and call RUST function
pub trait LoadableModule {
    fn load(module: FSModuleInterface, pool: FSModulePool) -> switch_status_t ;
    //fn shutdown() -> bool { true }
    //fn runtime() -> bool { true }
}

pub type FSModuleInterface<'a> = FSObject<'a,*mut switch_loadable_module_interface>;
pub type FSModulePool<'a> = FSObject<'a,switch_memory_pool_t>;

impl<'a> FSModuleInterface<'a> {
    // Internally FS locks so safe to use &self 
    pub fn add_api<T:ApiInterface>(&self, _i:T) {
       let t = switch_module_interface_name_t::SWITCH_API_INTERFACE;
        // SAFETY: We assume the module ptr given to us is valid 
        // also we restrict access to the builder to ONLY the load function
        unsafe {
            let ptr = switch_loadable_module_create_interface(*(self.ptr), t) as *mut switch_api_interface_t;
            let interface = &mut *ptr;
            interface.interface_name = CString::new(T::NAME).unwrap().into_raw();
            interface.desc = CString::new(T::DESC).unwrap().into_raw();
            interface.function = Some(T::api_fn_raw);
        }
    }
}

// =====
//pub struct StreamHandle {}
pub type StreamHandle<'a> = FSObject<'a,switch_stream_handle_t>;

impl<'a> std::io::Write for StreamHandle<'a> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let n = 0x1Ausize;
        Ok(n)
        
    }
    fn flush(&mut self) -> std::io::Result<()> {
       Ok(())
    }
}


pub trait ApiInterface {
    const NAME:&'static str;
    const DESC:&'static str;
    fn api_fn(cmd:&str, session:Option<Session>, stream:StreamHandle) -> freeswitch_sys::switch_status_t;
    unsafe extern "C" fn api_fn_raw(
        cmd: *const ::std::os::raw::c_char,
        session: *mut freeswitch_sys::switch_core_session_t,
        stream: *mut freeswitch_sys::switch_stream_handle_t,
    ) -> freeswitch_sys::switch_status_t;
}

// ==============

pub struct FSHandle<T>{ 
    ptr: *mut T,
}
unsafe impl<T> Send for FSHandle<T> where T:Send {}

pub struct FSObject<'a, T> {
    ptr: *mut T,
    lifetime: PhantomData<&'a T>
}

impl <'a, T> FSObject<'a,T> {
    pub fn from_raw(ptr:* mut T) -> FSObject<'a,T> {
        Self {
            ptr,
            lifetime: PhantomData {}
        }
    }
}

pub struct FSObjectMut<'a, T> {
    ptr: *mut T,
    lifetime: PhantomData<&'a mut T>
}

// =====

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
pub type MediaBugHandle = FSHandle<switch_media_bug_t>;
pub type MediaBug<'a> = FSObject<'a,switch_media_bug_t>;
impl <'a> MediaBug<'a> {

    unsafe extern "C" fn callback<F>(arg1: *mut switch_media_bug_t, arg2: *mut ::std::os::raw::c_void, arg3: switch_abc_type_t) -> switch_bool_t 
    where F: FnMut(MediaBug, switch_abc_type_t) -> bool,
    {
        let callback_ptr = arg2 as *mut F;
        let callback = &mut *callback_ptr;
        let bug = MediaBug { ptr:arg1, lifetime: PhantomData {} };

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
            Session { ptr, lifetime: PhantomData {} }
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
                let s = Session { ptr, lifetime:PhantomData {} };
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
        Some(FSObject { ptr:k.ptr, lifetime:PhantomData{} })
    }

    pub fn get_mut<T>(&mut self, k:&FSHandle<T>) -> Option<FSObjectMut<T>> {
        Some(FSObjectMut { ptr:k.ptr, lifetime:PhantomData{} })
    }

    pub fn add_media_bug<F>(&self, name:String, target:String, flags: switch_media_bug_flag_t, callback:F) 
        -> Result<MediaBugHandle, SessionError>
        where F: FnMut(MediaBug, switch_abc_type_t) -> bool + 'static + Send,
    {
        let data = Box::into_raw(Box::new(callback));
        let func = CString::new(name).unwrap();
        let target = CString::new(target).unwrap();

        let mut bug: *mut switch_media_bug = ptr::null_mut();

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

