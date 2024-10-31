use crate::utils::FSObject;
use crate::session::Session;
use std::ffi::CString;
use freeswitch_sys::switch_api_interface_t;
use freeswitch_sys::switch_loadable_module_create_interface;
use freeswitch_sys::switch_loadable_module_interface;
use freeswitch_sys::switch_memory_pool_t;
use freeswitch_sys::switch_stream_handle_t;
use freeswitch_sys::switch_status_t;
use freeswitch_sys::switch_module_interface_name_t;
use std::io::ErrorKind;

pub type StreamHandle<'a> = FSObject<'a,switch_stream_handle_t>;

impl<'a> std::io::Write for StreamHandle<'a> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        unsafe {
            if self.ptr.is_null() {
                return std::io::Result::Err(std::io::Error::new(ErrorKind::Other, "No FSStream writer"))
            }
            if let Some(w) = (*self.ptr).write_function {
                let cs = CString::new(buf).map_err(|_| std::io::Error::new(ErrorKind::InvalidData, ""))?;
                // The fs writer function success implies the full buffer is written 
                // we don't get any real info on error 
                let res = (w)(self.ptr, cs.as_ptr());
                if res == switch_status_t::SWITCH_STATUS_SUCCESS {
                    Ok(buf.len())
                }
                else {
                    std::io::Result::Err(std::io::Error::new(ErrorKind::Other, "FSStream writer Error"))
                }
            }
            else {
                std::io::Result::Err(std::io::Error::new(ErrorKind::Other, "No FSStream writer"))
            }
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
       Ok(())
    }
}

// ========

// We will need a macro to transform trait into extern C functions ....
// and call RUST function
pub trait LoadableModule {
    fn load(module: FSModuleInterface, pool: FSModulePool) -> switch_status_t;
    fn shutdown() -> switch_status_t { switch_status_t::SWITCH_STATUS_SUCCESS }
    fn runtime() -> switch_status_t { switch_status_t::SWITCH_STATUS_TERM }
}

pub type FSModuleInterface<'a> = FSObject<'a,*mut switch_loadable_module_interface>;
pub type FSModulePool<'a> = FSObject<'a,switch_memory_pool_t>;

impl<'a> FSModuleInterface<'a> {
    // SAFETY: DONT CALL 
    pub unsafe fn create(name:&str, pool:*mut switch_memory_pool_t,) -> *mut switch_loadable_module_interface {
        let mod_name =  CString::new(name.to_owned()).unwrap().into_raw();
        freeswitch_sys::switch_loadable_module_create_module_interface(pool, mod_name)
    }

    // Internally FS locks so safe to use &self 
    pub fn add_api<T:ApiInterface>(&self, _i:T) {
       let t = switch_module_interface_name_t::SWITCH_API_INTERFACE;
        // SAFETY: We assume the module ptr given to us is valid 
        // also we restrict access to the builder to ONLY the load function
        unsafe {
            let ptr = switch_loadable_module_create_interface(*(self.ptr), t) as *mut switch_api_interface_t;
            let interface = &mut *ptr;
            interface.interface_name = CString::new(T::NAME).unwrap().into_raw();
            interface.desc = CString::new(T::DESC).unwrap().into_raw();
            interface.function = Some(T::api_fn_raw);
        }
    }
}

pub trait ApiInterface {
    const NAME:&'static str;
    const DESC:&'static str;
    fn api_fn(cmd:&str, session:Option<Session>, stream:StreamHandle) -> freeswitch_sys::switch_status_t;
    unsafe extern "C" fn api_fn_raw(
        cmd: *const ::std::os::raw::c_char,
        session: *mut freeswitch_sys::switch_core_session_t,
        stream: *mut freeswitch_sys::switch_stream_handle_t,
    ) -> freeswitch_sys::switch_status_t;
}

