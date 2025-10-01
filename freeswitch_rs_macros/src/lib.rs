use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::meta::ParseNestedMeta;
use syn::parse::Parser;
use syn::{parse_macro_input, LitStr};

#[proc_macro_attribute]
pub fn switch_module_define(attr: TokenStream, item: TokenStream) -> TokenStream {
    let mod_struct = syn::parse_macro_input!(item as syn::ItemStruct);
    let args = syn::punctuated::Punctuated::<syn::Path, syn::Token![,]>::parse_terminated
        .parse2(attr.into())
        .unwrap();

    let mod_name = args
        .get(0)
        .and_then(|p| p.get_ident())
        .unwrap_or(&mod_struct.ident);
    impl_switch_module_define(&mod_struct, &mod_name)
}

fn impl_switch_module_define(ast: &syn::ItemStruct, mod_name: &syn::Ident) -> TokenStream {
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
                let _ = freeswitch_rs::log::set_logger(&freeswitch_rs::FS_LOG);
                freeswitch_rs::log::set_max_level(freeswitch_rs::log::LevelFilter::Debug);

                let ptr = freeswitch_rs::FSModuleInterface::create(#mod_name_string, pool);
                if ptr.is_null() { panic!("Module Creation Failed") }
                *module_interface = *(&ptr);

                let pool = freeswitch_rs::FSModulePool(pool);
                let module = freeswitch_rs::FSModuleInterface(module_interface);
                #struct_name::load(module,pool)
            }

            unsafe extern "C" fn shutdown_wrapper() -> freeswitch_rs::switch_status_t
            {
                #struct_name::shutdown()
            }
        }

        // Module Table
        #[no_mangle]
        #[allow(non_upper_case_globals)]
        pub static mut #mod_interface_ident: freeswitch_rs::switch_loadable_module_function_table= freeswitch_rs::switch_loadable_module_function_table{
            switch_api_version: 5,
            load: Some(#struct_name::load_wrapper),
            shutdown: Some(#struct_name::shutdown_wrapper),
            runtime: None,
            flags: 0,
        };
    };
    //eprintln!("TOKENS: {}", output);
    TokenStream::from(output)
}

#[derive(Default)]
struct ApiAttributes {
    name: Option<LitStr>,
    desc: Option<LitStr>,
}

impl ApiAttributes {
    fn parse(&mut self, meta: ParseNestedMeta) -> syn::parse::Result<()> {
        if meta.path.is_ident("name") {
            self.name = Some(meta.value()?.parse()?);
            Ok(())
        } else if meta.path.is_ident("desc") {
            self.desc = Some(meta.value()?.parse()?);
            Ok(())
        } else {
            Err(meta.error("unsupported property"))
        }
    }
}

#[proc_macro_attribute]
pub fn switch_api_define(attr: TokenStream, item: TokenStream) -> TokenStream {
    let ast = syn::parse_macro_input!(item as syn::ItemFn);
    let mut attrs = ApiAttributes::default();
    let parser = syn::meta::parser(|meta| attrs.parse(meta));
    parse_macro_input!(attr with parser);
    impl_switch_api_define(&ast, attrs)
}

fn impl_switch_api_define(ast: &syn::ItemFn, attrs: ApiAttributes) -> TokenStream {
    let syn::ItemFn { sig, block, .. } = ast;

    let name = &sig.ident;
    let fs_name = attrs.name.map(|ls| ls.value()).unwrap_or(name.to_string());
    let fs_desc = attrs.desc.map(|ls| ls.value()).unwrap_or("".to_string());

    let output = quote! {
        #[allow(non_camel_case_types)]
        struct #name;
        impl #name {
            #ast
        }

        impl freeswitch_rs::ApiInterface for #name {
            const NAME:&'static str = #fs_name;
            const DESC:&'static str = #fs_desc;
            fn api_fn(cmd:&str, session:Option<&freeswitch_rs::Session>, stream:freeswitch_rs::StreamHandle) -> freeswitch_rs::switch_status_t {
                #name::#name(cmd,session,stream)
            }
            unsafe extern "C" fn api_fn_raw(
                cmd: *const ::std::os::raw::c_char,
                session: *mut freeswitch_rs::switch_core_session_t,
                stream: *mut freeswitch_rs::switch_stream_handle_t,
            ) -> freeswitch_rs::switch_status_t {
                let cstr = std::ffi::CStr::from_ptr(cmd);
                let session = None;
                let stream = freeswitch_rs::StreamHandle(stream);
                #name::api_fn(cstr.to_str().unwrap(),session,stream)
            }
        }
    };

    //eprintln!("TOKENS1: {}", sig.ident);
    //eprintln!("TOKENS: {}", output);
    TokenStream::from(output)
}

#[proc_macro_attribute]
pub fn switch_state_handler(_: TokenStream, item: TokenStream) -> TokenStream {
    let ast = syn::parse_macro_input!(item as syn::ItemFn);
    impl_switch_state_handler(&ast)
}

fn impl_switch_state_handler(ast: &syn::ItemFn) -> TokenStream {
    let syn::ItemFn { sig, .. } = ast;
    let name = &sig.ident;
    let output = quote! {
        mod #name {
            use freeswitch_rs::Session;
            use freeswitch_sys::switch_status_t;
            use crate::*;
            use super::*;
            #ast
        }

        unsafe extern "C" fn #name(session: *mut freeswitch_sys::switch_core_session) -> freeswitch_sys::switch_status_t{
            let s= freeswitch_rs::Session(session);
            #name::#name(&s)
        }
    };
    TokenStream::from(output)
}
