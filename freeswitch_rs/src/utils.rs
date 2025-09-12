use freeswitch_sys::switch_status_t;
use std::{error::Error, fmt::Display};

// Errors
#[derive(Debug)]
pub struct FSError(switch_status_t);
impl From<switch_status_t> for FSError {
    fn from(value: switch_status_t) -> Self {
        assert!(value != switch_status_t::SWITCH_STATUS_SUCCESS);
        Self(value)
    }
}
impl From<FSError> for switch_status_t {
    fn from(value: FSError) -> Self {
        value.0
    }
}

impl Display for FSError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.0)
    }
}

impl Error for FSError {}

pub type Result<T> = std::result::Result<T, FSError>;

// ---------
macro_rules! call_with_meta_suffix {
     ($func:ident, $($arg:expr),*) => {{
        let file = CString::new(std::file!()).unwrap();
        let line = std::line!().try_into().unwrap_or(0);
        // TODO: fixme
        let func = CString::new("").unwrap();
        $func($($arg),*, file.as_ptr(), func.as_ptr(), line)
     }}
}

macro_rules! call_with_meta_prefix {
     ($func:ident, $($arg:expr),*) => {{
        let file = CString::new(std::file!()).unwrap();
        let line = std::line!().try_into().unwrap_or(0);
        let func = CString::new("").unwrap();
        $func(file.as_ptr(), func.as_ptr(), line,$($arg),*)
     }}
}

pub(crate) use call_with_meta_prefix;
pub(crate) use call_with_meta_suffix;
