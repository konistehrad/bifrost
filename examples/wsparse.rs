#![allow(unused_variables)]

use std::io::stdin;

use bifrost::{error::ApiResult, z2m::api::Message};

#[tokio::main]
#[rustfmt::skip]
async fn main() -> ApiResult<()> {
    pretty_env_logger::init();

    for line in stdin().lines() {
        let data = serde_json::from_str(&line?);

        let Ok(msg) = data else {
            log::error!("INVALID LINE: {:#?}", data);
            continue;
        };

        match msg {
            Message::BridgeInfo(ref obj) => {
                println!("{:#?}", obj.config_schema);
            },
            Message::BridgeLogging(ref obj) => {
                println!("{obj:#?}");
            },
            Message::BridgeExtensions(ref obj) => {
                println!("{obj:#?}");
            },
            Message::BridgeDevices(ref devices) => {
                for dev in devices {
                    println!("{dev:#?}");
                }
            },
            Message::BridgeGroups(ref obj) => {
                println!("{obj:#?}");
            },
            Message::BridgeDefinitions(ref obj) => {
                println!("{obj:#?}");
            },
            Message::BridgeState(ref obj) => {
                println!("{obj:#?}");
            },
            Message::BridgeEvent(ref obj) => {
                println!("{obj:#?}");
            },
        }
    }

    Ok(())
}
