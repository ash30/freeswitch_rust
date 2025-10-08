use freeswitch_sys::*;
use std::ffi::c_void;
use std::ffi::CStr;
use std::ptr;

use crate::prelude::*;

pub use freeswitch_rs_macros::switch_state_handler;
type StateHandlerTable = switch_state_handler_table_t;

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
            let _ = t.into_value();
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

    pub fn remove_private<T: IntoChannelValue>(&self, key: &CStr) -> Result<()> {
        unsafe {
            match switch_channel_set_private(self.as_ptr(), key.as_ptr(), ptr::null()) {
                switch_status_t::SWITCH_STATUS_SUCCESS => Ok(()),
                other => Err(other.into()),
            }
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
