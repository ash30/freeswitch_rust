use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{parse::Parser, parse_macro_input};


// switch_module_define(mod_name, load)
#[proc_macro]
pub fn switch_module_define(_item: TokenStream) -> TokenStream {
    let data = syn::punctuated::Punctuated::<syn::Type, syn::Token![,]>::parse_terminated
    .parse2(_item.into())
    .unwrap();
    impl_switch_module_define(data)
}

fn impl_switch_module_define(args: syn::punctuated::Punctuated<syn::Type,syn::token::Comma>) -> TokenStream {
    let mod_name:&syn::Ident = args.get(0)
        .and_then(|t| if let syn::Type::Path(p) = t {Some(p)} else {None} )
        .and_then(|p| p.path.get_ident())
        .expect("mod name should be valid");
    let mod_name_string = mod_name.to_string().to_owned();
    
    let load_fn = args.get(1);
    let mod_load_ident = format_ident!("{}_load", mod_name);
    let mod_interface_ident = format_ident!("{}_module_interface", mod_name);
    let output = quote! {
        
        // Wrap Load function 
        use std::io::Write;

        unsafe extern "C" fn #mod_load_ident (
            module_interface: *mut *mut freeswitch_rs::switch_loadable_module_interface_t,
            pool: *mut freeswitch_rs::switch_memory_pool_t,
        ) -> freeswitch_rs::switch_status_t {

            // init rust logger 
            freeswitch_rs::log::set_logger(&freeswitch_rs::FS_LOG).expect("successful rust logger init");

            let ptr = freeswitch_rs::FSModuleInterface::create(#mod_name_string, pool);
            if ptr.is_null() { panic!("Module Creation Failed") }
            *module_interface = *(&ptr);

            let module = freeswitch_rs::FSModuleInterface::from_raw(module_interface);
            let pool = freeswitch_rs::FSModulePool::from_raw(pool);
            #load_fn(module,pool)
        }

        // Module Table
        #[no_mangle]
        #[allow(non_upper_case_globals)]
        pub static mut #mod_interface_ident: freeswitch_rs::switch_loadable_module_function_table= freeswitch_rs::switch_loadable_module_function_table{
            switch_api_version: 5,
            load: Some(#mod_load_ident),
            shutdown: None,
            runtime: None,
            flags: 0,
        };
    };
    eprintln!("TOKENS: {}", output);
    TokenStream::from(output)
}

#[proc_macro_attribute]
pub fn switch_module_load_function(attr: TokenStream, item: TokenStream) -> TokenStream {
    //let args = syn::parse_macro_input!(attr as syn::Attribute);
    let ast = syn::parse_macro_input!(item as syn::ItemFn);
    impl_switch_module_load_function(&ast)
}

fn impl_switch_module_load_function(ast: &syn::ItemFn) -> TokenStream {
    let syn::ItemFn {
        sig: syn::Signature { ident, .. },
        block,
        ..
    } = ast;

    let output = quote! {
        // note: should we do this?
        use std::io::Write;

        unsafe extern "C" fn #ident (
            module_interface: *mut *mut freeswitch_rs::switch_loadable_module_interface_t,
            pool: *mut freeswitch_rs::switch_memory_pool_t,
        ) -> freeswitch_rs::switch_status_t {

            // NEED module name ...
            *module_interface = switch_loadable_module_create_module_interface(pool, "");
            let module = freeswitch_rs::FSModuleInterface::from_raw(module_interface);
            let pool = freeswitch_rs::FSModulePool::from_raw(pool);
            #block
        }
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
            fn api_fn(cmd:&str, session:Option<freeswitch_rs::Session>, stream:freeswitch_rs::StreamHandle) -> freeswitch_rs::switch_status_t {
                #name::#name(cmd,session,stream)                
            }
            unsafe extern "C" fn api_fn_raw(
                cmd: *const ::std::os::raw::c_char,
                session: *mut freeswitch_rs::switch_core_session_t,
                stream: *mut freeswitch_rs::switch_stream_handle_t,
            ) -> freeswitch_rs::switch_status_t {
                let c = std::ffi::CStr::from_ptr(cmd);
                let session = None;
                let stream = freeswitch_rs::StreamHandle::from_raw(stream);
                #name::api_fn(c.to_str().unwrap(),session,stream)
            }
        }
    };

    //eprintln!("TOKENS1: {}", sig.ident);
    //eprintln!("TOKENS: {}", output);
    TokenStream::from(output)
}



