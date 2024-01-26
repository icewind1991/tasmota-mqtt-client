use clap::Parser;
use std::pin::pin;
use tasmota_mqtt_client::DeviceUpdate;
pub use tasmota_mqtt_client::{Result, TasmotaClient};
use tokio_stream::StreamExt;

#[derive(Debug, Parser)]
struct Args {
    hostname: String,
    port: u16,
    username: String,
    password: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let client = TasmotaClient::connect(
        &args.hostname,
        args.port,
        Some((&args.username, &args.password)),
    )
    .await?;

    let mut discovery = pin!(client.devices());
    while let Some(update) = discovery.next().await {
        match update {
            DeviceUpdate::Added(device) => {
                println!("discovered {device}");
            }
            DeviceUpdate::Removed(device) => {
                println!("{device} has gone offline");
            }
        }
    }
    Ok(())
}
