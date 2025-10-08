use freeswitch_sys::{switch_log_level_t, switch_log_printf, switch_text_channel_t};
use log::Log;
use std::{ffi::CString, ptr::null};

#[repr(transparent)]
pub struct FSTextChannel(switch_text_channel_t);

macro_rules! logging_macro {
    ($name:ident, $t:expr, $fn:expr) => {
        #[macro_export]
        macro_rules! $name {
            ($e:expr) => {
                $crate::fslog::FSLoggerWithData(($fn)($e), $t)
            };
            () => {
                $crate::fslog::FSLoggerWithData(($fn)($e), $t)
            };
        }
        pub use $name;
    };
}

logging_macro!(channel_log, SWITCH_CHANNEL_ID_LOG, || null().cast());
logging_macro!(channel_log_clean, SWITCH_CHANNEL_ID_LOG_CLEAN, || null()
    .cast());
logging_macro!(session_log, SWITCH_CHANNEL_ID_SESSION, |s: &Session| s
    .as_ptr()
    .cast());
logging_macro!(
    session_log_clean,
    SWITCH_CHANNEL_ID_SESSION,
    |s: &Session| s.get_uuid().as_ptr()
);

pub const SWITCH_CHANNEL_ID_LOG: FSTextChannel =
    FSTextChannel(switch_text_channel_t::SWITCH_CHANNEL_ID_LOG);
pub const SWITCH_CHANNEL_ID_LOG_CLEAN: FSTextChannel =
    FSTextChannel(switch_text_channel_t::SWITCH_CHANNEL_ID_LOG_CLEAN);
pub const SWITCH_CHANNEL_ID_EVENT: FSTextChannel =
    FSTextChannel(switch_text_channel_t::SWITCH_CHANNEL_ID_EVENT);
pub const SWITCH_CHANNEL_ID_SESSION: FSTextChannel =
    FSTextChannel(switch_text_channel_t::SWITCH_CHANNEL_ID_SESSION);

// ==========
pub struct FSLoggerWithData(pub *const ::std::os::raw::c_char, pub FSTextChannel);
//
unsafe impl Send for FSLoggerWithData {}
unsafe impl Sync for FSLoggerWithData {}

impl Log for FSLoggerWithData {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        FSLogger.enabled(metadata)
    }
    fn log(&self, record: &log::Record) {
        FSLogger.log_with_userdata(record, SWITCH_CHANNEL_ID_SESSION, self.0.cast());
    }
    fn flush(&self) {
        FSLogger.flush();
    }
}

// We provide an adapter over switch logging so mod authors can use the standard
// log crate facarde. Still working out how to pass user data back ....
pub struct FSLogger;

impl FSLogger {
    fn log_with_userdata(
        &self,
        record: &log::Record,
        text_channel: FSTextChannel,
        userdata: *const ::std::os::raw::c_char,
    ) {
        if self.enabled(record.metadata()) {
            let file = record
                .file()
                .and_then(|s| CString::new(s).ok())
                .map(|s| s.into_raw());
            let line = record.line().unwrap_or(0);
            let func = std::ptr::null();
            let level = match record.metadata().level() {
                log::Level::Warn => switch_log_level_t::SWITCH_LOG_WARNING,
                log::Level::Info => switch_log_level_t::SWITCH_LOG_INFO,
                log::Level::Error => switch_log_level_t::SWITCH_LOG_ERROR,
                log::Level::Debug => switch_log_level_t::SWITCH_LOG_DEBUG,
                log::Level::Trace => switch_log_level_t::SWITCH_LOG_DEBUG10,
            };
            let fmt = format!("{}\n", record.args());
            let fmt_c = CString::new(fmt).unwrap();

            unsafe {
                switch_log_printf(
                    text_channel.0,
                    file.unwrap_or(std::ptr::null_mut()),
                    func,
                    line.try_into().unwrap_or(0),
                    userdata,
                    level,
                    fmt_c.into_raw(),
                )
            }
        }
    }
}

impl Log for FSLogger {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        self.log_with_userdata(record, SWITCH_CHANNEL_ID_LOG, null());
    }

    fn flush(&self) {}
}
