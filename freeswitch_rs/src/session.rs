use freeswitch_sys::*;
use std::ffi::c_void;
use std::ffi::CStr;
use std::ffi::CString;
use std::mem;
use std::ops::Deref;
use std::ptr;

use crate::fs_new_type;
use crate::fs_session_owned_type;
use crate::utils::call_with_meta_suffix;
use crate::utils::FSNewType;
use crate::Result;

#[derive(Debug, Clone, PartialEq)]
pub struct SessionUUID(String);

// ------------

pub struct LocateGuard(Session);

impl Drop for LocateGuard {
    fn drop(&mut self) {
        // SAFETY: In theory this is safe because ptr is valid since we located Session
        // to create Session struct
        unsafe { switch_core_session_rwunlock(self.0.as_ptr()) }
    }
}

impl Deref for LocateGuard {
    type Target = Session;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

// ------------
//
fs_new_type!(Session, *mut switch_core_session_t);

impl Session {
    #[track_caller]
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
                Some(LocateGuard(Session::from_ptr(ptr)))
            }
        }
    }

    pub fn get_channel(&self) -> Option<Channel<'_>> {
        unsafe {
            let ptr = switch_core_session_get_channel(self.as_ptr());
            if ptr.is_null() {
                return None;
            };
            Some(Channel::from_ptr(ptr))
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
                self.as_ptr(),
                function.map(|f| f.as_ptr()).unwrap_or(ptr::null()),
                target.map(|t| t.as_ptr()).unwrap_or(ptr::null()),
                Some(bug_extern_callback::<F>),
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
    fn from_value(ptr: *const c_void) -> Self;
}

// Blanket impl for all FS wrapper types so they store their wrapped pointer in a channel
unsafe impl<T, U> IntoChannelValue for T
where
    T: FSNewType<Inner = *mut U>,
{
    fn from_value(ptr: *const c_void) -> T {
        Self::from_ptr(ptr as *mut U)
    }
    fn into_value(self) -> *const c_void {
        self.as_ptr() as *const c_void
    }
}

// Channel type lifetime is tied to session
fs_session_owned_type!(Channel, *mut switch_channel_t);
type ChannelStateHandlerIndex = usize;

impl<'a> Channel<'a> {
    // We should treat values within a channel as 'shared'
    // since the channels themselve make no claim over ownership.
    // To this end, we require values to implement clone
    // to avoid inadverntanly invalidating the ptr stored.
    pub fn set_private<T>(&self, key: &CStr, data: T) -> Result<()>
    where
        T: IntoChannelValue + Clone,
        T: 'a, // data must be equal or outlive channel/session
    {
        let data_ptr = data.into_value();

        // SAFETY:
        // FS will take a lock on channel insert / read so its safe to call
        // with shared reference
        unsafe {
            match switch_channel_set_private(self.as_ptr(), key.as_ptr(), data_ptr) {
                switch_status_t::SWITCH_STATUS_SUCCESS => Ok(()),
                other => Err(other.into()),
            }
        }
    }

    pub fn get_private<T>(&self, key: &CStr) -> Option<T>
    where
        T: IntoChannelValue + Clone,
        T: 'a, // data must be equal or outlive channel/session
    {
        unsafe {
            let ptr = self.get_private_raw_ptr(key)?;
            let t = T::from_value(ptr);
            let ret = Some(t.clone());
            t.into_value();
            ret
        }
    }

    /// # Safety
    ///
    /// Channels do not own or cleanup their data, so caller must ensure ptrs
    /// to rust allocated structs are cleaned up IF required ie call drop!
    pub unsafe fn set_private_raw_ptr<T>(&self, key: &CStr, data: *const T) -> Result<()> {
        // SAFETY:
        // FS will take a lock on channel insert / read so its safe to call
        // with shared reference
        unsafe {
            match switch_channel_set_private(self.as_ptr(), key.as_ptr(), data as *const c_void) {
                switch_status_t::SWITCH_STATUS_SUCCESS => Ok(()),
                other => Err(other.into()),
            }
        }
    }

    /// # Safety
    ///
    /// Care must be taken to not invalidate the stored pointer whilst the channel holds the value
    /// ie ensure T is not dropped
    pub unsafe fn get_private_raw_ptr<T>(&self, key: &CStr) -> Option<*mut T> {
        // SAFETY:
        // FS will take a lock on channel insert / read so its safe to call
        // with shared reference
        unsafe {
            let ptr = switch_channel_get_private(self.as_ptr(), key.as_ptr());
            if ptr.is_null() {
                return None;
            }
            Some(ptr as *mut T)
        }
    }

    pub fn remove_private_with_key<T: IntoChannelValue>(&self, key: &CStr) {
        unsafe {
            switch_channel_set_private(self.as_ptr(), key.as_ptr(), ptr::null());
        }
    }

    pub fn add_state_handler(
        &self,
        table: &'static StateHandlerTable,
    ) -> Result<ChannelStateHandlerIndex> {
        unsafe {
            match switch_channel_add_state_handler(self.as_ptr(), table) {
                n if n < 0 => Err(switch_status_t::SWITCH_STATUS_GENERR.into()),
                n => Ok(n.try_into().unwrap()),
            }
        }
    }
}

// =====

pub type MediaBugFlags = freeswitch_sys::switch_media_bug_flag_enum_t;

fs_session_owned_type!(MediaBug, *mut switch_media_bug_t);
fs_new_type!(MediaBugHandle, *mut switch_media_bug_t, derive(Clone));

unsafe impl Send for MediaBugHandle {}

unsafe extern "C" fn bug_extern_callback<F>(
    arg1: *mut switch_media_bug_t,
    arg2: *mut ::std::os::raw::c_void,
    arg3: switch_abc_type_t,
) -> switch_bool_t
where
    F: FnMut(&mut MediaBug, switch_abc_type_t) -> bool,
{
    let callback_ptr = arg2 as *mut F;
    let callback = &mut *callback_ptr;
    let mut bug = MediaBug::from_ptr(arg1);

    let res = if callback(&mut bug, arg3) {
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

impl<'a> MediaBug<'a> {
    pub fn get_session(&self) -> &Session {
        // SAFETY
        // Media bug lifetime is tied to session, so should be safe
        // assuming media bug ptr is ok which in the context of bug callback, should be
        unsafe {
            let ptr = switch_core_media_bug_get_session(self.0);
            &*(ptr as *mut Session)
        }
    }

    pub fn read_frame(&mut self, frame: &mut Frame) -> Result<usize> {
        let res = unsafe { switch_core_media_bug_read(self.0, &mut frame.0, true.into()) };
        if res != switch_status_t::SWITCH_STATUS_SUCCESS {
            Err(res.into())
        } else {
            Ok(frame.0.datalen as usize)
        }
    }
}

pub struct Frame<'a>(switch_frame_t, &'a mut [u8]);

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
