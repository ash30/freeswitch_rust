use std::ffi::CStr;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct WSForkEvent {
    pub session: String,
    #[serde(flatten)]
    pub body: Body,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type")]
pub enum Body {
    Connected {},
    Closed {},
    Error { desc: String },
    Overrun {},
}

pub const MOD_WSFORK_EVENT_CONNECT: &CStr = c"CONNECT";
pub const MOD_WSFORK_EVENT_DISCONNECT: &CStr = c"DISCONNECT";
pub const MOD_WSFORK_EVENT_SAMPLES_OVERRUN: &CStr = c"OVERRUN";
pub const MOD_WSFORK_EVENT_ERROR: &CStr = c"ERROR";

impl Body {
    pub fn tag(&self) -> &'static CStr {
        match self {
            Body::Connected { .. } => MOD_WSFORK_EVENT_CONNECT,
            Body::Closed { .. } => MOD_WSFORK_EVENT_DISCONNECT,
            Body::Error { .. } => MOD_WSFORK_EVENT_ERROR,
            Body::Overrun { .. } => MOD_WSFORK_EVENT_SAMPLES_OVERRUN,
        }
    }
}
