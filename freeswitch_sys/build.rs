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
        .allowlist_file("*.switch_types.h")
        .newtype_enum("^switch_status_t")
        .allowlist_item("switch_media_bug_flag.*")
        .bitfield_enum("switch_media_bug_flag.*")
        // Session
        .allowlist_item("^switch_core_session.*")
        .allowlist_item("^switch_core_perform_session.*")
        // bugs
        .allowlist_item("^switch_core_media_bug.*")
        // Logging
        .allowlist_item("^switch_log.*")
        .newtype_enum("^switch_log_level_t")
        // Mod Loading
        .allowlist_item("^switch_loadable_module_function_table_t")
        .allowlist_item("^switch_loadable_module_create_interface")
        .allowlist_item("^switch_loadable_module_create_module_interface")
        .allowlist_item("^switch_.*?_interface_t")
        .newtype_enum("^switch_abc_type_t")
        .newtype_enum("^switch_module_interface_name_t")
        // Events
        .allowlist_item("^switch_event.*")
        .newtype_enum("^switch_event_types_t")
        // Channels
        .allowlist_item("^switch_state_handler.*")
        .allowlist_item("^switch_channel.*")
        .newtype_enum("switch_channel_state_t")
        .newtype_enum("^switch_text_channel_t");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());

    bindings
        .generate()
        .expect("Unable to generate bindings")
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");
}
