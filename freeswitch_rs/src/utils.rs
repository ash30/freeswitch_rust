use freeswitch_sys::switch_status_t;
use std::{error::Error, fmt::Display};

pub(crate) trait FSHandle<T: FSNewType> {}
pub(crate) trait FSNewType {
    type Inner;
    fn from_ptr(ptr: Self::Inner) -> Self;
    fn as_ptr(&self) -> Self::Inner;
}

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
        let loc = std::panic::Location::caller();
        let file = CString::new(loc.file()).unwrap();
        let line = loc.line() as i32;
        let func = std::ptr::null();
        $func($($arg),*, file.as_ptr(), func, line)
     }}
}

macro_rules! call_with_meta_prefix {
     ($func:ident, $($arg:expr),*) => {{
        let loc = std::panic::Location::caller();
        let file = CString::new(loc.file()).unwrap();
        let line = loc.line() as i32;
        let func = std::ptr::null();
        $func(file.as_ptr(), func, line,$($arg),*)
     }}
}

macro_rules! fs_new_type{
    ($wrapper:ident, $inner:ty) => {
        fs_new_type!($wrapper, $inner, derive());
    };
    ($wrapper:ident, $inner:ty, derive($($derive:path),*)) => {

        #[derive(Debug $(, $derive)*)]
        pub struct $wrapper($inner);

        impl crate::utils::FSNewType for $wrapper {
            type Inner = $inner;
            fn from_ptr(ptr:$inner) -> Self {
                Self(ptr)
            }
            fn as_ptr(&self) -> $inner {
                self.0
            }
        }
    };
}

macro_rules! fs_session_owned_type {
    ($wrapper:ident, $inner:ty) => {
        fs_session_owned_type!($wrapper, $inner, derive());
    };
    ($wrapper:ident, $inner:ty, derive($($derive:path),*)) => {

        #[derive(Debug $(, $derive)*)]
        pub struct $wrapper<'a>($inner, std::marker::PhantomData<&'a Session>);

        impl<'a> crate::utils::FSNewType for $wrapper<'a> {
            type Inner = $inner;
            fn from_ptr(ptr:$inner) -> Self {
                Self(ptr, std::marker::PhantomData)
            }
            fn as_ptr(&self) -> $inner {
                self.0
            }
        }
    };
}

pub(crate) use call_with_meta_prefix;
pub(crate) use call_with_meta_suffix;
pub(crate) use fs_new_type;
pub(crate) use fs_session_owned_type;
