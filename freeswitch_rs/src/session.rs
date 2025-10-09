use freeswitch_sys::*;
use std::ffi::c_void;
use std::ffi::CStr;
use std::ffi::CString;
use std::ops::Deref;
use std::ptr;

use crate::prelude::*;

use crate::channel::Channel;
use crate::Frame;

/// RAII guard that unlocks a session on drop.
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

/// Wrapper around FreeSWITCH session.
fs_new_type!(Session, *mut switch_core_session_t);

impl Session {
    /// Locate a session by UUID. See: [`switch_core_session_perform_locate`](../../freeswitch_sys/fn.switch_core_session_perform_locate.html).
    #[track_caller]
    pub fn locate(id: &CStr) -> Option<LocateGuard> {
        // SAFETY
        // Locating will take a read lock of any found session
        // so the reference to session can live as long as you own the guard
        unsafe {
            let ptr = call_with_meta_suffix!(switch_core_session_perform_locate, id.as_ptr());
            if ptr.is_null() {
                None
            } else {
                Some(LocateGuard(Session::from_ptr(ptr)))
            }
        }
    }

    /// Retrieve the unique identifier from a session. See: [`switch_core_session_get_uuid`](../../freeswitch_sys/fn.switch_core_session_get_uuid.html).
    pub fn get_uuid(&self) -> &CStr {
        unsafe { CStr::from_ptr(switch_core_session_get_uuid(self.as_ptr())) }
    }

    /// Retrieve a reference to the channel object associated with a given session. See: [`switch_core_session_get_channel`](../../freeswitch_sys/fn.switch_core_session_get_channel.html).
    pub fn get_channel(&self) -> Option<Channel<'_>> {
        unsafe {
            let ptr = switch_core_session_get_channel(self.as_ptr());
            if ptr.is_null() {
                return None;
            };
            Some(Channel::from_ptr(ptr))
        }
    }

    /// Remove a media bug from the session. See: [`switch_core_media_bug_remove`](../../freeswitch_sys/fn.switch_core_media_bug_remove.html).
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

    /// Add a media bug to the session. See: [`switch_core_media_bug_add`](../../freeswitch_sys/fn.switch_core_media_bug_add.html).
    ///
    /// # Examples
    ///
    /// ```
    /// use freeswitch_rs::{Session, MediaBugFlags};
    /// use freeswitch_sys::{switch_abc_type_t, switch_media_bug_flag_t};
    /// use std::ffi::CString;
    ///
    /// let session = Session::locate("uuid").unwrap();
    /// let handle = session.add_media_bug(
    ///     Some(CString::new("my_function").unwrap()),
    ///     None,
    ///     switch_media_bug_flag_t(switch_media_bug_flag_t::SWITCH_MEDIA_BUG_FLAG_READ),
    ///     |bug, abc_type| {
    ///         match abc_type {
    ///             switch_abc_type_t::SWITCH_ABC_TYPE_INIT => {
    ///                 // Initialize
    ///             }
    ///             switch_abc_type_t::SWITCH_ABC_TYPE_READ => {
    ///                 // Read media
    ///             }
    ///             switch_abc_type_t::SWITCH_ABC_TYPE_CLOSE => {
    ///                 // Cleanup
    ///             }
    ///             _ => {}
    ///         }
    ///         true
    ///     }
    /// ).unwrap();
    /// ```
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

// =====

pub type MediaBugFlags = freeswitch_sys::switch_media_bug_flag_enum_t;

fs_session_owned_type!(MediaBug, *mut switch_media_bug_t);

// Its *probably* safe to make this cloneable
// FS won't dealloc the bug on removal, so double remove is a non op
// and it allows for easy safe storage into Channel
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
    /// Obtain the session from a media bug. See: [`switch_core_media_bug_get_session`](../../freeswitch_sys/fn.switch_core_media_bug_get_session.html).
    pub fn get_session(&self) -> &Session {
        // SAFETY
        // Media bug lifetime is tied to session, so should be safe
        // assuming media bug ptr is ok which in the context of bug callback, should be
        unsafe {
            let ptr = switch_core_media_bug_get_session(self.0);
            &*(ptr as *mut Session)
        }
    }

    /// Read a frame from the bug. See: [`switch_core_media_bug_read`](../../freeswitch_sys/fn.switch_core_media_bug_read.html).
    pub fn read_frame(&mut self, frame: &mut Frame) -> Result<usize> {
        let res = unsafe { switch_core_media_bug_read(self.0, &mut frame.0, true.into()) };
        if res != switch_status_t::SWITCH_STATUS_SUCCESS {
            Err(res.into())
        } else {
            Ok(frame.0.datalen as usize)
        }
    }

    //pub fn as_handle(&self) -> MediaBugHandle {
    //    MediaBugHandle::from_ptr(self.as_ptr())
    //}
}
