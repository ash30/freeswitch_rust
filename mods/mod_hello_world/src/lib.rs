use freeswitch_rs::core::Session;
use freeswitch_rs::log::{debug, info};
use freeswitch_rs::prelude::*;

#[switch_module_define(mod_hello_world)]
struct FSModule;

impl LoadableModule for FSModule {
    fn load(module: FSModuleInterface, _pool: FSModulePool) -> switch_status_t {
        info!("mod hello_world loading");
        module.add_api(hello_world);
        switch_status_t::SWITCH_STATUS_SUCCESS
    }
}
// api will use function name if no 'name' attribute provided to proc macro
#[switch_api_define]
fn hello_world(cmd: &str, _session: Option<&Session>, mut stream: StreamHandle) -> switch_status_t {
    debug!("mod hello_world cmd {}", &cmd);
    let _ = writeln!(stream, "+OK Success");
    switch_status_t::SWITCH_STATUS_SUCCESS
}
