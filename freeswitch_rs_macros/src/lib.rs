use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::parse::Parser;

#[proc_macro_attribute]
pub fn switch_module_define(attr: TokenStream, item: TokenStream) -> TokenStream {
    let mod_struct = syn::parse_macro_input!(item as syn::ItemStruct);
    let args = syn::punctuated::Punctuated::<syn::Path, syn::Token![,]>::parse_terminated
    .parse2(attr.into())
    .unwrap();

    let mod_name =  args.get(0).and_then(|p| p.get_ident()).unwrap_or(&mod_struct.ident);
    impl_switch_module_define(&mod_struct, &mod_name)
}

fn impl_switch_module_define(ast: &syn::ItemStruct, mod_name:&syn::Ident) -> TokenStream {
    let struct_name = &ast.ident;
    let mod_interface_ident = format_ident!("{}_module_interface", mod_name);
    let mod_name_string = mod_name.to_string().to_owned();

    let output = quote! {
        // Wrap Load function 
        use std::io::Write;

        #ast 

        impl #struct_name {
            unsafe extern "C" fn load_wrapper (
                module_interface: *mut *mut freeswitch_rs::switch_loadable_module_interface_t,
                pool: *mut freeswitch_rs::switch_memory_pool_t,
            ) -> freeswitch_rs::switch_status_t 
            {
                freeswitch_rs::log::set_logger(&freeswitch_rs::FS_LOG).expect("successful rust logger init");
                freeswitch_rs::log::set_max_level(freeswitch_rs::log::LevelFilter::Debug);

                let ptr = freeswitch_rs::FSModuleInterface::create(#mod_name_string, pool);
                if ptr.is_null() { panic!("Module Creation Failed") }
                *module_interface = *(&ptr);

                let module = freeswitch_rs::FSModuleInterface::from_raw(module_interface);
                let pool = freeswitch_rs::FSModulePool::from_raw(pool);
                #struct_name::load(module,pool)
            }
        }

        // Module Table
        #[no_mangle]
        #[allow(non_upper_case_globals)]
        pub static mut #mod_interface_ident: freeswitch_rs::switch_loadable_module_function_table= freeswitch_rs::switch_loadable_module_function_table{
            switch_api_version: 5,
            load: Some(#struct_name::load_wrapper),
            shutdown: None,
            runtime: None,
            flags: 0,
        };
    };
    eprintln!("TOKENS: {}", output);
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



