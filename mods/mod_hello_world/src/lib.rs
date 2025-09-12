use freeswitch_rs::log::{debug, info};
use freeswitch_rs::SWITCH_CHANNEL_ID_LOG;
use freeswitch_rs::*;

#[switch_module_define(mod_hello_world)]
struct FSModule;

impl LoadableModule for FSModule {
    fn load(module: FSModuleInterface, pool: FSModulePool) -> switch_status_t {
        info!(channel = SWITCH_CHANNEL_ID_LOG; "mod hello_world loading");
        module.add_api(api_main);
        switch_status_t::SWITCH_STATUS_SUCCESS
    }
}

#[switch_api_define("hello_world")]
fn api_main(cmd: &str, _session: Option<&Session>, mut stream: StreamHandle) -> switch_status_t {
    debug!(channel = SWITCH_CHANNEL_ID_SESSION; "mod hello_world cmd {}", &cmd);
    let _ = writeln!(stream, "+OK Success");
    switch_status_t::SWITCH_STATUS_SUCCESS
}
