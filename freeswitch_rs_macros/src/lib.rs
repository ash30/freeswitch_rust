use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{parse::Parser, parse_macro_input};

#[proc_macro]
pub fn switch_module_define(_item: TokenStream) -> TokenStream {
    let data = syn::punctuated::Punctuated::<syn::Path, syn::Token![,]>::parse_terminated
    .parse2(_item.into())
    .unwrap();
    impl_switch_module_define(data)
}

fn impl_switch_module_define(args: syn::punctuated::Punctuated<syn::Path,syn::token::Comma>) -> TokenStream {
    let name = args.get(0).unwrap().get_ident().unwrap();
    let fn_name = format_ident!("{}_module_interface", name);
    let load_fn = args.get(1);
    let output = quote! {
        #[no_mangle]
        #[allow(non_upper_case_globals)]
        pub static mut #fn_name: freeswitch_rs::switch_loadable_module_function_table= freeswitch_rs::switch_loadable_module_function_table{
            switch_api_version: 5,
            load: Some(#load_fn),
            shutdown: None,
            runtime: None,
            flags: 0,
        };
    };
    //eprintln!("TOKENS: {}", output);
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



