//! Unofficial Rust bindings for [Freeswitch](https://signalwire.com/freeswitch)
//!
//! The goal is to provide a safe + ergonomic pit of success for mod authors and connect freeswitch to the wider rust ecosystem.
//!
//! To that end, it includes:
//!
//! - Bindings for basic freeswitch types via bindgen *_sys crate, type aliased here mostly for
//!   organisation
//! - 'New Type' wrappers over freeswitch types to help map ownership and standard marker traits
//! - Rust friendly mod creation via proc macros that mimic the traditional C SDK.
//!
//!

// mods
mod frame;
mod modules;
mod session;
mod utils;

// rexports
pub use frame::*;

// Public mods
pub mod channel;
pub mod event;
pub mod fslog;

pub mod types {
    pub use freeswitch_sys::switch_abc_type_t;
    pub use freeswitch_sys::switch_api_interface_t;
    pub use freeswitch_sys::switch_application_interface_t;
    pub use freeswitch_sys::switch_core_session_t;
    pub use freeswitch_sys::switch_event_types_t;
    pub use freeswitch_sys::switch_frame_t;
    pub use freeswitch_sys::switch_loadable_module_function_table;
    pub use freeswitch_sys::switch_loadable_module_interface_t;
    pub use freeswitch_sys::switch_memory_pool_t;
    pub use freeswitch_sys::switch_module_interface_name_t;
    pub use freeswitch_sys::switch_status_t;
    pub use freeswitch_sys::switch_stream_handle_t;
}

pub mod prelude {
    pub use crate::modules::*;
    pub use crate::types::switch_status_t;
    pub use crate::utils::FSNewType;
    pub use crate::utils::{FSError, Result};

    // macros
    pub use freeswitch_rs_macros::switch_api_define;
    pub use freeswitch_rs_macros::switch_module_define;

    // logging
    pub use crate::fslog::session_log;

    pub use crate::fslog::SWITCH_CHANNEL_ID_EVENT;
    pub use crate::fslog::SWITCH_CHANNEL_ID_LOG;
    pub use crate::fslog::SWITCH_CHANNEL_ID_LOG_CLEAN;
    pub use crate::fslog::SWITCH_CHANNEL_ID_SESSION;

    pub(crate) use crate::utils::*;
}

#[doc(hidden)]
pub use modules::*;

pub mod core {
    pub use crate::session::*;
}

#[doc(hidden)]
pub use log;
