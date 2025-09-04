use freeswitch_sys::{switch_log_level_t, switch_log_printf, switch_text_channel_t};
use log::{kv::Value, Log};
use std::{ffi::CString, ptr::null};

pub struct FSTextChannel(switch_text_channel_t);

impl log::kv::ToValue for FSTextChannel {
    fn to_value(&self) -> log::kv::Value {
        Value::from(self.0 .0)
    }
}

pub const SWITCH_CHANNEL_ID_LOG: FSTextChannel =
    FSTextChannel(switch_text_channel_t::SWITCH_CHANNEL_ID_LOG);
pub const SWITCH_CHANNEL_ID_LOG_CLEAN: FSTextChannel =
    FSTextChannel(switch_text_channel_t::SWITCH_CHANNEL_ID_LOG_CLEAN);
pub const SWITCH_CHANNEL_ID_EVENT: FSTextChannel =
    FSTextChannel(switch_text_channel_t::SWITCH_CHANNEL_ID_EVENT);
pub const SWITCH_CHANNEL_ID_SESSION: FSTextChannel =
    FSTextChannel(switch_text_channel_t::SWITCH_CHANNEL_ID_SESSION);

// ==========
// We provide an adapter over switch logging so mod authors can use the standard
// log crate facarde. Still working out how to pass user data back ....
pub struct FSLogger;

pub static FS_LOG: FSLogger = FSLogger {};

impl Log for FSLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        true
    }
    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            // lets prefix the file name so we can find things in fs_cli
            let prefixed_file_name = format!(
                "{}:{}",
                env!("CARGO_PKG_NAME"),
                record.file().unwrap_or("unknown")
            );
            let file = CString::new(prefixed_file_name).unwrap();
            let line = record.line().unwrap_or(0);
            let func = CString::new("").unwrap();
            let level = match record.metadata().level() {
                log::Level::Warn => switch_log_level_t::SWITCH_LOG_WARNING,
                log::Level::Info => switch_log_level_t::SWITCH_LOG_INFO,
                log::Level::Error => switch_log_level_t::SWITCH_LOG_ERROR,
                log::Level::Debug => switch_log_level_t::SWITCH_LOG_DEBUG,
                log::Level::Trace => switch_log_level_t::SWITCH_LOG_DEBUG10,
            };
            let channel = record
                .key_values()
                .get("channel".into())
                .and_then(|a| a.to_u64())
                .unwrap_or(0);
            let fmt = format!("{}", record.args());
            let fmt_c = CString::new(fmt).unwrap();

            unsafe {
                switch_log_printf(
                    switch_text_channel_t(channel as u32),
                    file.into_raw(),
                    func.into_raw(),
                    line.try_into().unwrap_or(0),
                    null(),
                    level,
                    fmt_c.into_raw(),
                )
            }
        }
    }
    fn flush(&self) {}
}
