mod download;
mod error;
mod mqtt;

use crate::download::download_config;
pub use crate::download::DownloadedFile;
use crate::mqtt::MqttHelper;
pub use error::{Error, Result};
use rumqttc::MqttOptions;
use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};
use tokio::spawn;
use tokio::sync::broadcast::{channel, Sender};
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::{Stream, StreamExt};

pub struct TasmotaClient {
    mqtt: MqttHelper,
    known_devices: Arc<Mutex<BTreeSet<String>>>,
    device_update: Sender<DeviceUpdate>,
}

#[derive(Debug, Clone)]
pub enum DeviceUpdate {
    Added(String),
    Removed(String),
}

impl TasmotaClient {
    pub async fn connect(host: &str, port: u16, credentials: Option<(&str, &str)>) -> Result<Self> {
        let mut mqtt_opts = MqttOptions::new("tasmota-client", host, port);
        if let Some((username, password)) = credentials {
            mqtt_opts.set_credentials(username, password);
        }
        let mqtt = MqttHelper::connect(mqtt_opts)?;

        let mut lwt = mqtt.subscribe("tele/+/LWT".into()).await?;

        let known_devices = Arc::new(Mutex::new(BTreeSet::new()));

        let edit_devices = known_devices.clone();

        let (tx, _) = channel(10);
        let device_update = tx.clone();

        spawn(async move {
            while let Some(msg) = lwt.recv().await {
                let payload = std::str::from_utf8(msg.payload.as_ref()).unwrap_or_default();
                let Some(device) = msg.topic.split('/').nth(1) else {
                    continue;
                };

                match payload {
                    "Online" => {
                        edit_devices.lock().unwrap().insert(device.into());
                        let _ = tx.send(DeviceUpdate::Added(device.into()));
                    }
                    "Offline" => {
                        edit_devices.lock().unwrap().remove(device.into());
                        let _ = tx.send(DeviceUpdate::Removed(device.into()));
                    }
                    _ => {}
                }
            }
        });

        Ok(TasmotaClient {
            mqtt,
            known_devices,
            device_update,
        })
    }

    /// Download the config backup from a device
    ///
    /// The password is the mqtt password used by the device, which might be different from the mqtt password used by this client
    pub async fn download_config(&self, client: &str, password: &str) -> Result<DownloadedFile> {
        download_config(&self.mqtt, client, password).await
    }

    /// Get the list of known devices at this point in time
    ///
    /// Due to the asynchronous nature of discovery, calling this directly after creating the client
    /// will be unlikely to return all live devices
    pub fn current_devices(&self) -> Vec<String> {
        self.known_devices.lock().unwrap().iter().cloned().collect()
    }

    /// Subscribe to device discovery, receiving a [`DeviceUpdate`] whenever a device comes online or goes offline
    pub fn devices(&self) -> impl Stream<Item = DeviceUpdate> {
        let current = self.current_devices();
        let rx = self.device_update.subscribe();

        tokio_stream::iter(
            current
                .into_iter()
                .map(|device| DeviceUpdate::Added(device)),
        )
        .chain(BroadcastStream::new(rx).filter_map(Result::ok))
    }
}
