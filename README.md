# Unofficial FreeSWITCH Rust Bindings

**⚠️ Work in Progress** - This is an experimental, unofficial project to provide idiomatic Rust bindings for FreeSWITCH.

The goal is to provide a safe + ergonomic pit of success for mod authors, and connect freeswitch to the wider rust ecosystem.

## Example Mod

A simple module demonstrating the basic API pattern:

```rust
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
```

For a more advanced example with async processing and media bugs, see [`mods/mod_wsfork`](mods/mod_wsfork/src/lib.rs).

## Acknowledgments

- This project wouldn't be possible without the work of the [FreeSWITCH](https://freeswitch.org/) project, creating and maintaining an incredible open source telephony platform.
- The previous work in the area: [Freeswitchrs](https://github.com/friends-of-freeswitch/freeswitchrs) which helped guide the way.
- The open source mods written by Dave Horton / [drachtio](https://drachtio.org/) which really helped me understand freeswitch.

