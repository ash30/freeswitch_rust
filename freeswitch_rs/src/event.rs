use freeswitch_sys::*;
use std::{
    ffi::{CStr, CString},
    mem::MaybeUninit,
};

use crate::{call_with_meta_prefix, FSScopedHandleMut, Result};

pub struct Event;

impl Event {
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

    pub fn new_custom_event(
        subclass: String,
    ) -> Result<FSScopedHandleMut<'static, switch_event_t>> {
        Event::new_core_event(switch_event_types_t::SWITCH_EVENT_CUSTOM, Some(subclass))
    }

    pub fn new_core_event(
        event: switch_event_types_t,
        subclass: Option<String>,
    ) -> Result<FSScopedHandleMut<'static, switch_event_t>> {
        let mut e: MaybeUninit<*mut switch_event_t> = MaybeUninit::zeroed();

        let subclass = subclass.map(|s| CString::new(s).unwrap());
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
                switch_status_t::SWITCH_STATUS_SUCCESS => {
                    Ok(FSScopedHandleMut::from_raw(e.assume_init()))
                }
                other => Err(other.into()),
            }
        }
    }
}

impl FSScopedHandleMut<'static, switch_event_t> {
    // its only really safe to fire/mutate newly owned events
    // hence implementing these methods only on Mutable Scoped Handle

    // Null for user data, same as the normal macro
    pub fn fire(mut self) -> Result<()> {
        // SAFETY:
        // switch_event_fire_detailed cleans up memory
        // even if error, but just incase we will cleanup non null ptrs
        unsafe {
            let res = call_with_meta_prefix!(
                switch_event_fire_detailed,
                &mut self.ptr,
                std::ptr::null_mut()
            );

            match res {
                switch_status_t::SWITCH_STATUS_SUCCESS => Ok(()),
                other => {
                    if !self.ptr.is_null() {
                        switch_event_destroy(&mut self.ptr);
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
