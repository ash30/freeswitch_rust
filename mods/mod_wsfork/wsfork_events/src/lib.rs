use std::ffi::CStr;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct WSForkEvent {
    pub session: String,
    #[serde(flatten)]
    pub body: Body,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug)]
#[serde(tag = "type")]
pub enum Body {
    Connected {},
    Closed {
        code: Option<u16>,
        reason: Option<String>,
    },
    Message {
        content: String,
    },
    Error {
        desc: String,
    },
    Overrun {},
}

pub const MOD_WSFORK_EVENT: &CStr = c"MOD_WSFORK_EVENT";
