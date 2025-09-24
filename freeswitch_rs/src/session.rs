use freeswitch_sys::*;
use std::ffi::c_void;
use std::ffi::CStr;
use std::ffi::CString;
use std::io::Read;
use std::mem;
use std::ops::Deref;
use std::ptr;

use crate::fs_wrapper_type;
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
pub struct Session(pub *mut switch_core_session_t);

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
        function: Option<CString>,
        target: Option<CString>,
        flags: MediaBugFlags,
        callback: F,
    ) -> Result<MediaBugHandle>
    where
        F: FnMut(&mut MediaBug, switch_abc_type_t) -> bool + 'static + Send,
    {
        let data = Box::into_raw(Box::new(callback));
        let mut bug: *mut switch_media_bug_t = ptr::null_mut();

        // SAFETY:
        unsafe {
            let res = switch_core_media_bug_add(
                self.0,
                function.map(|f| f.as_ptr()).unwrap_or(ptr::null()),
                target.map(|t| t.as_ptr()).unwrap_or(ptr::null()),
                Some(MediaBug::callback::<F>),
                data as *mut c_void,
                0,
                flags.0,
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

/// # Safety
///
/// Implementors should be aware that channels take no ownership of values
/// and so it up to you to free if needed at suitable point in time.
/// Additionally as Channels store void ptrs, its on calling code to cast correctly
pub unsafe trait IntoChannelValue {
    fn into_value(self) -> *const c_void;
    fn from_ptr(ptr: *const c_void) -> Self;
}

#[repr(transparent)]
pub struct Channel(pub *mut switch_channel_t);

type ChannelStateHandlerIndex = usize;

impl Channel {
    pub fn set_private_with_key<T: IntoChannelValue>(&self, key: &CStr, data: T) -> Result<()> {
        let data_ptr = data.into_value();

        // SAFETY:
        // FS will take a lock on channel insert / read so its safe to call
        // with shared reference
        unsafe {
            match switch_channel_set_private(self.0, key.as_ptr(), data_ptr) {
                switch_status_t::SWITCH_STATUS_SUCCESS => Ok(()),
                other => Err(other.into()),
            }
        }
    }

    /// # Safety
    ///
    /// Callers must enure channel key was set to T previously
    pub unsafe fn get_private_with_key<T: IntoChannelValue>(&self, key: &CStr) -> Option<&T> {
        // SAFETY:
        // FS will take a lock on channel insert / read so its safe to call
        // with shared reference
        unsafe {
            let ptr = switch_channel_get_private(self.0, key.as_ptr());
            if ptr.is_null() {
                return None;
            }
            Some(&*(ptr as *const T))
        }
    }

    pub fn remove_private_with_key<T: IntoChannelValue>(&self, key: &CStr) {
        unsafe {
            switch_channel_set_private(self.0, key.as_ptr(), ptr::null());
        }
    }

    pub fn add_state_handler(
        &self,
        table: &'static StateHandlerTable,
    ) -> Result<ChannelStateHandlerIndex> {
        unsafe {
            match switch_channel_add_state_handler(self.0, table) {
                n if n < 0 => Err(switch_status_t::SWITCH_STATUS_GENERR.into()),
                n => Ok(n.try_into().unwrap()),
            }
        }
    }
}

// =====

pub type MediaBugFlags = freeswitch_sys::switch_media_bug_flag_enum_t;

fs_wrapper_type!(MediaBugHandle, *mut switch_media_bug_t, derive(Clone));
fs_wrapper_type!(MediaBug, *mut switch_media_bug_t);

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

// ====
pub type StateHandlerTable = switch_state_handler_table_t;
pub const DEFAULT_STATE_HANDLER_TABLE: StateHandlerTable = StateHandlerTable {
    on_init: None,
    on_execute: None,
    on_hibernate: None,
    on_destroy: None,
    on_consume_media: None,
    on_routing: None,
    on_hangup: None,
    on_exchange_media: None,
    on_soft_execute: None,
    on_reset: None,
    on_park: None,
    on_reporting: None,
    padding: [ptr::null_mut(); 10],
    flags: 0,
};
