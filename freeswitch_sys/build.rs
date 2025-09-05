use std::env;
use std::path::PathBuf;

fn main() {
    let lib = pkg_config::Config::new()
        .statik(false)
        .probe("freeswitch")
        .unwrap();

    let mut bindings = bindgen::Builder::default()
        .clang_args(
            lib.include_paths
                .iter()
                .map(|path| format!("-I{}", path.display())),
        )
        .header("./include/wrapper.h")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()));

    bindings = bindings
        // General
        .allowlist_item("switch_status_t")
        .newtype_enum("switch_status_t")
        // Session
        .allowlist_item("^switch_core_session.*")
        .allowlist_item("^switch_core_perform_session.*")
        // Logging
        .allowlist_item("^switch_log.*")
        .allowlist_item("switch_log_printf")
        .allowlist_item("switch_log_level_t")
        .newtype_enum("switch_log_level_t")
        // bugs
        .allowlist_item("^switch_core_media_bug.*")
        // Mod Loading
        .allowlist_item("switch_loadable_module_function_table_t")
        .allowlist_item("switch_loadable_module_create_interface")
        .allowlist_item("switch_loadable_module_create_module_interface")
        .allowlist_item("switch_.*?_interface_t")
        .newtype_enum("switch_abc_type_t")
        .allowlist_item("switch_module_interface_name_t")
        .newtype_enum("switch_module_interface_name_t")
        // Channels
        .allowlist_item("^switch_channel.*")
        .allowlist_item("switch_text_channel_t")
        .newtype_enum("switch_text_channel_t");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());

    bindings
        .generate()
        .expect("Unable to generate bindings")
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");
}
