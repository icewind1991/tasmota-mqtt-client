# tasmota-mqtt-client

Rust library for interacting with tasmota devices over MQTT

## Supported features

- Device discovery
- Query device name
- Query device ip
- Backup device config

## Example

```rust
use std::pin::pin;
use tasmota_mqtt_client::{DeviceUpdate, Result, TasmotaClient};
use tokio::join;
use tokio_stream::StreamExt;

#[tokio::main]
async fn main() -> Result<()> {
    let client = TasmotaClient::connect(
        "mqtt.example.com",
        1883,
        Some(("mqtt_username", "mqtt_password")),
    )
        .await?;

    let mut discovery = pin!(client.devices());
    while let Some(update) = discovery.next().await {
        match update {
            DeviceUpdate::Added(device) => {
                let (ip, name) = join!(client.device_ip(&device), client.device_name(&device));
                println!("discovered {}({device}) with ip {}", name?, ip?);
            }
            DeviceUpdate::Removed(device) => {
                println!("{device} has gone offline");
            }
        }
    }
    Ok(())
}
```
