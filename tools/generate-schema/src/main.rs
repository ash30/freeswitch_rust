use schemars::schema_for;
use serde_json::json;
use std::collections::HashMap;

use wsfork_events::WSForkEvent;

fn main() {
    let mut schemas = HashMap::new();

    schemas.insert("WSFork", schema_for!(WSForkEvent));

    let output = json!({
        "events":schemas
    });

    println!("{}", serde_json::to_string_pretty(&output).unwrap());
}
