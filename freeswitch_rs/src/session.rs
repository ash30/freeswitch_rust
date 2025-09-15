use freeswitch_sys::*;
use std::any::TypeId;
use std::ffi::c_void;
use std::ffi::CString;
use std::hash;
use std::hash::Hash;
use std::hash::Hasher;
use std::io::Read;
use std::mem;
use std::ops::Deref;
use std::ptr;

use crate::utils::call_with_meta_suffix;
use crate::Result;

#[derive(Debug, Clone, PartialEq)]
pub struct SessionUUID(String);

// ------------

pub struct LocateGuard(*mut switch_core_session_t);

impl Drop for LocateGuard {
    fn drop(&mut self) {
        // SAFETY: In theory this is safe because ptr is valid since we located Session
        // to create Session struct
        if !self.0.is_null() {
            unsafe { switch_core_session_rwunlock(self.0) }
        }
    }
}

impl Deref for LocateGuard {
    type Target = Session;
    fn deref(&self) -> &Self::Target {
        unsafe { &*(self.0 as *const Session) }
    }
}

// ------------
#[repr(transparent)]
pub struct Session(*mut switch_core_session_t);

impl Session {
    pub fn locate(id: &str) -> Option<LocateGuard> {
        let s: CString = CString::new(id.to_owned()).unwrap();

        // SAFETY
        // Locating will take a read lock of any found session
        // so the reference to session can live as long as you own the guard
        unsafe {
            let ptr = call_with_meta_suffix!(switch_core_session_perform_locate, s.as_ptr());
            if ptr.is_null() {
                None
            } else {
                Some(LocateGuard(ptr))
            }
        }
    }
}

impl Session {
    pub fn get_channel(&self) -> Option<&Channel> {
        unsafe {
            let c = switch_core_session_get_channel(self.0);
            if c.is_null() {
                return None;
            };
            Some(&*(c as *const Channel))
        }
    }

    pub fn remove_media_bug(&self, mut bug: MediaBugHandle) -> Result<()> {
        // SAFETY:
        // FS will nullify the media bug ptr, so all me
        unsafe {
            if bug.0.is_null() {
                // Bug has already been removed
                return Ok(());
            }
            match switch_core_media_bug_remove(self.0, &mut bug.0) {
                switch_status_t::SWITCH_STATUS_SUCCESS => Ok(()),
                other => Err(other.into()),
            }
        }
    }

    pub fn add_media_bug<F>(
        &self,
        name: String,
        target: String,
        flags: switch_media_bug_flag_t,
        callback: F,
    ) -> Result<MediaBugHandle>
    where
        F: FnMut(&mut MediaBug, switch_abc_type_t) -> bool + 'static + Send,
    {
        let data = Box::into_raw(Box::new(callback));
        let func = CString::new(name).unwrap();
        let target = CString::new(target).unwrap();

        let mut bug: *mut switch_media_bug_t = ptr::null_mut();

        // SAFETY:
        unsafe {
            let res = switch_core_media_bug_add(
                self.0,
                func.as_ptr(),
                target.as_ptr(),
                Some(MediaBug::callback::<F>),
                data as *mut c_void,
                0,
                flags,
                &mut bug as *mut *mut switch_media_bug_t,
            );

            match res {
                switch_status_t::SWITCH_STATUS_SUCCESS => {
                    if bug.is_null() {
                        return Err(switch_status_t::SWITCH_STATUS_MEMERR.into());
                    }
                    Ok(MediaBugHandle(bug))
                }
                other => Err(other.into()),
            }
        }
    }
}
// =====

#[repr(transparent)]
pub struct Channel(pub *mut switch_channel_t);

impl Channel {
    pub fn set_private<T>(&self, data: Box<T>) -> Result<&T>
    where
        T: Sized + 'static,
    {
        let mut h = hash::DefaultHasher::new();
        TypeId::of::<T>().hash(&mut h);
        let key = CString::new(h.finish().to_string()).unwrap();
        let data_ptr = Box::into_raw(data);

        // SAFETY:
        // FS will take a lock on channel insert / read so its safe to call
        // with shared reference
        // We store the raw box ptr and reconstruct it as a shared refererence on READ...
        // the theory being, BOX<T> should be compatible with T* and hence we provide that as a
        // reference - which means box drop shouldn't happen
        unsafe {
            match switch_channel_set_private(self.0, key.as_ptr(), data_ptr as *mut c_void) {
                switch_status_t::SWITCH_STATUS_SUCCESS => Ok(&*(data_ptr as *const T)),
                other => Err(other.into()),
            }
        }
    }

    pub fn get_private<T>(&self) -> Option<&T>
    where
        T: Sized + 'static,
    {
        let mut h = hash::DefaultHasher::new();
        TypeId::of::<T>().hash(&mut h);
        let key = CString::new(h.finish().to_string()).unwrap();

        // SAFETY:
        // We can sure be sure of the cast because key == typeid
        // Memory is owned by session so we assume channel existing proves
        // mem pool is still there.
        // FS locks on read so is safe to call with shared reference
        unsafe {
            let ptr = switch_channel_get_private(self.0, key.as_ptr());
            if ptr.is_null() {
                return None;
            }
            Some(&*(ptr as *const T))
        }
    }
}

// =====

#[repr(transparent)]
pub struct MediaBug(*mut switch_media_bug_t);

#[repr(transparent)]
pub struct MediaBugHandle(*mut switch_media_bug_t);

impl MediaBug {
    unsafe extern "C" fn callback<F>(
        arg1: *mut switch_media_bug_t,
        arg2: *mut ::std::os::raw::c_void,
        arg3: switch_abc_type_t,
    ) -> switch_bool_t
    where
        F: FnMut(&mut MediaBug, switch_abc_type_t) -> bool,
    {
        let callback_ptr = arg2 as *mut F;
        let callback = &mut *callback_ptr;
        let bug = &mut *(arg1 as *mut MediaBug);

        let res = if callback(bug, arg3) {
            switch_bool_t_SWITCH_TRUE
        } else {
            switch_bool_t_SWITCH_FALSE
        };

        // take back ownership of box so we can clean up
        if arg3 == switch_abc_type_t::SWITCH_ABC_TYPE_CLOSE {
            let _ = Box::from_raw(arg2);
        }
        res
    }

    pub fn get_session(&self) -> &Session {
        // SAFETY
        // Media bug lifetime is tied to session, so should be safe
        // assuming media bug ptr is ok which in the context of bug callback, should be
        unsafe {
            let ptr = switch_core_media_bug_get_session(self.0);
            &*(ptr as *mut Session)
        }
    }
}

impl Read for MediaBug {
    // Read may potentially return Zero and its ok to read again ( waiting on packets etc )
    //
    // The FS api doesn't offer much inspection of the error when calling
    // switch_core_media_bug_read - so how to understand IF its error or just not ready to read?
    // seems like a generic error value based on switch_status_t is the best we can do ?
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let mut f = unsafe { mem::MaybeUninit::<switch_frame_t>::zeroed().assume_init() };
        f.data = buf.as_mut_ptr() as *mut c_void;
        f.buflen = buf.len().try_into().unwrap();

        // SAFETY:
        // Implementation needs to ensure that fs is given correct buf len
        // technically its callers responsiblity to initialise the buffer, so we don't fill for now
        let res = unsafe { switch_core_media_bug_read(self.0, &mut f, switch_bool_t_SWITCH_FALSE) };
        if res != switch_status_t::SWITCH_STATUS_SUCCESS {
            Err(std::io::Error::other(format!("switch status: {:?}", res)))
        } else {
            Ok(f.datalen.try_into().unwrap())
        }
    }
}

// ====
// TODO: import properly
pub const SWITCH_RECOMMENDED_BUFFER_SIZE: usize = 8192;
