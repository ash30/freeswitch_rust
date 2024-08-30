use proc_macro::TokenStream;
use quote::quote;

#[proc_macro_attribute]
pub fn switch_module_define(attr: TokenStream, item: TokenStream) -> TokenStream {
    //let args = syn::parse_macro_input!(attr as syn::Attribute);
    let ast = syn::parse_macro_input!(item as syn::ItemStruct);
    impl_switch_module_define(&ast)
}

fn impl_switch_module_define(ast: &syn::ItemStruct) -> TokenStream {
    let syn::ItemStruct {
        ident,
        ..
	} = ast;

    let output = quote! {
        #ast

        struct __MOD__;
        impl __MOD__ {
            const fslog:freeswitch_rs::FSLogger = freeswitch_rs::FSLogger;

            unsafe extern "C" fn mod_load_raw(
                module_interface: *mut *mut freeswitch_sys::switch_loadable_module_interface_t,
                pool: *mut freeswitch_sys::switch_memory_pool_t,
            ) -> freeswitch_sys::switch_status_t {

                // Setup logging for the rest of the user defined module functions
                freeswitch_rs::log::set_logger(&__MOD__::fslog);

                let m = freeswitch_rs::FSModuleInterface::from_raw(module_interface);
                let p = freeswitch_rs::FSModulePool::from_raw(pool);
                #ident::load(m,p);
                freeswitch_sys::switch_status_t_SWITCH_STATUS_SUCCESS
            }

            unsafe extern "C" fn mod_shutdown_raw() -> freeswitch_sys::switch_status_t {
                freeswitch_sys::switch_status_t_SWITCH_STATUS_SUCCESS
            }

            unsafe extern "C" fn mod_runtime_raw() -> freeswitch_sys::switch_status_t {
                freeswitch_sys::switch_status_t_SWITCH_STATUS_TERM
            }
        }

        #[no_mangle]
        #[allow(non_upper_case_globals)]
        pub static mut mod_test: freeswitch_sys::switch_loadable_module_function_table= freeswitch_sys::switch_loadable_module_function_table{
            switch_api_version: 5,
            load: Some(__MOD__::mod_load_raw),
            shutdown: Some(__MOD__::mod_shutdown_raw),
            runtime: Some(__MOD__::mod_runtime_raw),
            flags: 0,
        };
    };
    TokenStream::from(output)
}


#[proc_macro_attribute]
pub fn switch_api_define(attr: TokenStream, item: TokenStream) -> TokenStream {
    //let args = syn::parse_macro_input!(attr as syn::Attribute);
    let ast = syn::parse_macro_input!(item as syn::ItemFn);
    impl_switch_api_define(&ast)
}

fn impl_switch_api_define(ast: &syn::ItemFn) -> TokenStream {
    	let syn::ItemFn {
        sig,
        block,
        ..
	} = ast;

    let name = &sig.ident;
    let output = quote! {

        #[allow(non_camel_case_types)]
        struct #name;
        impl #name {
            #ast
        }

        impl freeswitch_rs::ApiInterface for #name {
            const NAME:&'static str = "test";
            const DESC:&'static str = "test";
            fn api_fn(cmd:&str, session:Option<Session>, stream:Stream) {
                #name::#name(cmd,session,stream)                
            }
            unsafe extern "C" fn api_fn_raw(
                cmd: *const ::std::os::raw::c_char,
                session: *mut freeswitch_sys::switch_core_session_t,
                stream: *mut freeswitch_sys::switch_stream_handle_t,
            ) -> freeswitch_sys::switch_status_t {

                let c = std::ffi::CStr::from_ptr(cmd);
                let session = None;
                let stream = freeswitch_rs::Stream {};

                #name::api_fn(c.to_str().unwrap(),session,stream);
                freeswitch_sys::switch_status_t_SWITCH_STATUS_SUCCESS
                
                // call original function please
            }
        }
    };

    //eprintln!("TOKENS1: {}", sig.ident);
    //eprintln!("TOKENS: {}", output);
    TokenStream::from(output)
}



