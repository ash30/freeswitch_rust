use crate::utils::FSNewType;
use freeswitch_sys::*;
use std::{
    ffi::{CStr, CString},
    mem::MaybeUninit,
};

use crate::prelude::*;

use crate::channel::Channel;

/// Wrapper around FreeSWITCH event.
#[repr(transparent)]
pub struct Event(*mut switch_event_t);

impl Event {
    /// Reserve a subclass name for private use with a custom event. See: [`switch_event_reserve_subclass_detailed`](../../freeswitch_sys/fn.switch_event_reserve_subclass_detailed.html).
    pub fn reserve_subclass(name: &CStr) -> Result<()> {
        // SAFETY: file and name are copied
        unsafe {
            let file = CString::new(std::file!()).unwrap();
            match switch_event_reserve_subclass_detailed(file.as_ptr(), name.as_ptr()) {
                switch_status_t::SWITCH_STATUS_SUCCESS => Ok(()),
                other => Err(other.into()),
            }
        }
    }

    /// Free a subclass name reserved for private use with a custom event. See: [`switch_event_free_subclass_detailed`](../../freeswitch_sys/fn.switch_event_free_subclass_detailed.html).
    pub fn free_subclass(name: &CStr) -> Result<()> {
        // SAFETY: file and name are copied
        unsafe {
            let file = CString::new(std::file!()).unwrap();
            match switch_event_free_subclass_detailed(file.as_ptr(), name.as_ptr()) {
                switch_status_t::SWITCH_STATUS_SUCCESS => Ok(()),
                other => Err(other.into()),
            }
        }
    }

    /// Create a new custom event with the given subclass name.
    pub fn new_custom_event(subclass: &CStr) -> Result<Self> {
        Event::new_core_event(switch_event_types_t::SWITCH_EVENT_CUSTOM, Some(subclass))
    }

    /// Create an event. See: [`switch_event_create_subclass_detailed`](../../freeswitch_sys/fn.switch_event_create_subclass_detailed.html).
    #[track_caller]
    pub fn new_core_event(event: switch_event_types_t, subclass: Option<&CStr>) -> Result<Self> {
        let mut e: MaybeUninit<*mut switch_event_t> = MaybeUninit::zeroed();

        let subclass_ptr = match subclass.as_ref() {
            None => std::ptr::null(),
            Some(s) => s.as_ptr(),
        };

        // SAFETY: Initialise the event via fs function,
        // where subclass is copied
        unsafe {
            let res = call_with_meta_prefix!(
                switch_event_create_subclass_detailed,
                e.as_mut_ptr(),
                event,
                subclass_ptr
            );
            match res {
                switch_status_t::SWITCH_STATUS_SUCCESS => Ok(Self(e.assume_init())),
                other => Err(other.into()),
            }
        }
    }
}

impl Event {
    /// Add information about a given channel to an event object. See: [`switch_channel_event_set_data`](../../freeswitch_sys/fn.switch_channel_event_set_data.html).
    pub fn set_channel_data(&mut self, channel: &Channel) {
        // SAFETY:
        // we assume channel holds a valid ptr, which is validated by
        // structs returning the reference
        // set_data methods will take profile lock for us so its safe to
        // call with shared reference
        unsafe {
            switch_channel_event_set_data(channel.as_ptr(), self.0);
        }
    }

    /// Set the body of an event. See: [`switch_event_set_body`](../../freeswitch_sys/fn.switch_event_set_body.html).
    pub fn set_body<T>(&mut self, body: T) -> Result<()>
    where
        T: Into<Vec<u8>>,
    {
        let cstr = CString::new(body).map_err(|_e| switch_status_t::SWITCH_STATUS_GENERR)?;
        // body is copied within fs, just make sure we free this side
        unsafe {
            let res = switch_event_set_body(self.0, cstr.as_ptr());
            match res {
                switch_status_t::SWITCH_STATUS_SUCCESS => Ok(()),
                other => Err(other.into()),
            }
        }
    }

    /// Fire an event. See: [`switch_event_fire_detailed`](../../freeswitch_sys/fn.switch_event_fire_detailed.html).
    #[track_caller]
    pub fn fire(mut self) -> Result<()> {
        // SAFETY:
        // switch_event_fire_detailed cleans up memory
        // even if error, but just incase we will cleanup non null ptrs
        unsafe {
            let res = call_with_meta_prefix!(
                switch_event_fire_detailed,
                &mut self.0,
                // Null for user data, same as the fs macro
                std::ptr::null_mut()
            );

            match res {
                switch_status_t::SWITCH_STATUS_SUCCESS => Ok(()),
                other => {
                    if !self.0.is_null() {
                        switch_event_destroy(&mut self.0);
                    }
                    Err(other.into())
                }
            }
        }
    }
}

// NOTES:
// Generally FS will null the ptr once the event is fired or Error'd
// drop is only really necessary if user create events and DON'T use them ...
// is it necessary?
