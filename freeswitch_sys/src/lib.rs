#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use std::{ffi::CString, marker::PhantomData};
use std::os::raw::c_void;
//use std::ffi::c_int;

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

struct SessionData<T> {
   ptr: *mut c_void,
   session_id: SessionUUID,
   t: PhantomData<T>
}

impl<T> SessionData<T> {
    fn new(p:*mut c_void, session_id:SessionUUID) -> Self {
        Self {
            ptr:p,
            session_id,
            t:PhantomData
        }
    }
}

enum SessionError {
    AllocationError
}

#[derive(Debug, Clone, PartialEq)]
struct SessionUUID(String);

struct Session {
    ptr: *mut switch_core_session_t,
    id: SessionUUID
}

impl Drop for Session {
    fn drop(&mut self) {
        // SAFETY: In theory this is safe because ptr is valid since we located Session
        // to create Session struct 
        unsafe { 
            if !self.ptr.is_null() { switch_core_session_rwunlock(self.ptr) }
        }
    }
}

impl Session {
    pub fn locate(id:&SessionUUID) -> Option<Self> {
        let file = CString::new(std::file!()).unwrap();
        let line = std::line!().try_into().unwrap_or(0);
        let func = CString::new("").unwrap();
        let s:CString = CString::new(id.0.to_owned()).unwrap();

        // SAFETY: 
        unsafe {
            let ptr = switch_core_session_perform_locate(s.as_ptr(), file.as_ptr(), func.as_ptr(), line);
            if ptr.is_null() { None }
            else { 
               Some(Session { ptr, id: id.clone() })
            }
        }
    }

    pub fn insert<T:Sized>(&self, data:T) -> Result<SessionData<T>, SessionError> {
            let file = CString::new(std::file!()).unwrap();
            let line = std::line!().try_into().unwrap_or(0);
            let func = CString::new("").unwrap();

        // SAFETY: 
        unsafe {
            let ptr = crate::switch_core_perform_session_alloc(
                self.ptr, std::mem::size_of::<T>(), 
                file.as_ptr(),
                func.as_ptr(),
                line
            );
            if !ptr.is_null(){
                let t = &mut*(ptr as *mut T);
                std::mem::replace(t, data);
                Ok(SessionData::new(ptr, self.id.to_owned()))
            }
            else {
                Err(SessionError::AllocationError)
            }
        }
    }

    pub fn get<'a,T>(&self, k:SessionData<T>) -> Option<&'a T> {
        if k.ptr.is_null() { return None };
        if k.session_id != self.id { return None };
        // SAFETY: 
        unsafe { 
            let r = &*(k.ptr as *mut T);
            Some(r)
        }
    }

    pub fn get_mut<'a,T>(&self, k:SessionData<T>) -> Option<&'a mut T> {
        if k.ptr.is_null() { return None };
        if k.session_id != self.id { return None };
        // SAFETY: 
        unsafe { 
            let r = &mut*(k.ptr as *mut T);
            Some(r)
        }
    }
}

