use std::env;
use std::path::PathBuf;

fn main() {
    // Tell cargo to look for shared libraries in the specified directory
    //println!("cargo:rustc-link-search=/path/to/lib");

    // Tell cargo to tell rustc to link the system bzip2
    // shared library.
    println!("cargo:rustc-link-lib=freeswitch");


    // The bindgen::Builder is the main entry point
    // to bindgen, and lets you build up options for
    // the resulting bindings.
    let bindings = bindgen::Builder::default()
        // The input header we would like to generate
        // bindings for.
        .header("./include/wrapper.h")
        // Tell cargo to invalidate the built crate whenever any of the
        // included header files changed.
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
	    .allowlist_item("^switch_core_session.*")
	    .allowlist_item("^switch_core_perform_session.*")
	    .allowlist_item("^switch_log.*")
        .allowlist_item("^switch_core_media_bug.*")
        .allowlist_item("switch_loadable_module_function_table_t")
        .allowlist_item("switch_loadable_module_create_interface")
        .allowlist_item("switch_.*?_interface_t")
        .newtype_enum("switch_abc_type_t")
        .allowlist_item("switch_module_interface_name_t")
        .newtype_enum("switch_module_interface_name_t")
        // Finish the builder and generate the bindings.
        .generate()
        // Unwrap the Result and panic on failure.
        .expect("Unable to generate bindings");

    // Write the bindings to the $OUT_DIR/bindings.rs file.
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");
}
