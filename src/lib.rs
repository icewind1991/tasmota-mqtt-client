#![doc = include_str!("../README.md")]

mod download;
mod error;
mod mqtt;

use crate::download::download_config;
pub use crate::download::DownloadedFile;
use crate::error::MqttError;
use crate::mqtt::MqttHelper;
pub use error::{Error, Result};
use rumqttc::MqttOptions;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use std::collections::BTreeSet;
use std::fmt::Debug;
use std::net::IpAddr;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::spawn;
use tokio::sync::broadcast::{channel, Sender};
use tokio::time::timeout;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::{Stream, StreamExt};
use tracing::debug;

/// A client for interacting with tasmota devices over MQTT
pub struct TasmotaClient {
    mqtt: MqttHelper,
    known_devices: Arc<Mutex<BTreeSet<String>>>,
    device_update: Sender<DeviceUpdate>,
    timeout: Duration,
}

/// A device has been added or removed.
///
/// See also [`TasmotaClient::devices`].
#[derive(Debug, Clone)]
pub enum DeviceUpdate {
    /// A new device has been discovered, or a previously offline device has come back
    Added(String),
    /// A previously discovered device has gone offline
    Removed(String),
}

impl TasmotaClient {
    /// Connect to an MQTT server to allow access to tasmota devices connected to the same server
    ///
    /// # Usage
    ///
    /// ```rust,no_run
    /// # use tasmota_mqtt_client::{Result, TasmotaClient};
    /// # #[tokio::main]
    /// # async fn main() -> Result<()> {
    /// let client = TasmotaClient::connect(
    ///     "mqtt.example.com",
    ///     1883,
    ///     Some(("mqtt_username", "mqtt_password")),
    /// ).await?;
    /// #   Ok(())
    /// # }
    /// ```
    ///
    pub async fn connect(host: &str, port: u16, credentials: Option<(&str, &str)>) -> Result<Self> {
        let mut mqtt_opts = MqttOptions::new("tasmota-client", host, port);
        if let Some((username, password)) = credentials {
            mqtt_opts.set_credentials(username, password);
        }
        Self::from_mqtt_options(mqtt_opts).await
    }

    /// Connect to an MQTT server using an existing [`MqttOptions`].
    pub async fn from_mqtt_options(options: MqttOptions) -> Result<Self> {
        let mqtt = MqttHelper::connect(options);

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

                debug!(
                    message = payload,
                    device = device,
                    "processing discovery message"
                );
                match payload {
                    "Online" => {
                        if edit_devices.lock().unwrap().insert(device.into()) {
                            let _ = tx.send(DeviceUpdate::Added(device.into()));
                        }
                    }
                    "Offline" => {
                        if edit_devices.lock().unwrap().remove(device) {
                            let _ = tx.send(DeviceUpdate::Removed(device.into()));
                        }
                    }
                    _ => {}
                }
            }
        });

        Ok(TasmotaClient {
            mqtt,
            known_devices,
            device_update,
            timeout: Duration::from_secs(1),
        })
    }

    /// Set the timeout used for one-show commands
    ///
    /// The default timeout is 1 second
    pub fn set_timeout(&mut self, timeout: Duration) {
        self.timeout = timeout;
    }

    /// Download the config backup from a device
    ///
    /// The password is the mqtt password used by the device, which might be different from the mqtt password used by this client
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use std::pin::pin;
    /// # use tasmota_mqtt_client::{DeviceUpdate, Result, TasmotaClient};
    /// # use tokio::join;
    /// # use tokio_stream::StreamExt;
    /// #
    /// # #[tokio::main]
    /// # async fn main() -> Result<()> {
    ///     # let client = TasmotaClient::connect(
    ///     #     "mqtt.example.com",
    ///     #     1883,
    ///     #     Some(("mqtt_username", "mqtt_password")),
    ///     # ).await?;
    /// // let client: TasmotaClient = ...
    /// let download = client.download_config("tasmota_device", "tasmota_device_mqtt_password").await?;
    /// println!("downloaded config file {} of {} bytes", download.name, download.data.len());
    ///     # Ok(())
    /// # }
    /// ```
    #[tracing::instrument(skip(self))]
    pub async fn download_config(&self, client: &str, password: &str) -> Result<DownloadedFile> {
        download_config(&self.mqtt, client, password, self.device_update.subscribe()).await
    }

    /// Get the list of known devices at this point in time
    ///
    /// Due to the asynchronous nature of discovery, calling this directly after creating the client
    /// will be unlikely to return all live devices.
    ///
    /// Use [`Self::devices`] if you need to know all live devices.
    pub fn current_devices(&self) -> Vec<String> {
        self.known_devices.lock().unwrap().iter().cloned().collect()
    }

    /// Subscribe to device discovery, receiving a [`DeviceUpdate`] whenever a device comes online or goes offline
    ///
    /// This will include an update for any device that is known at the time of calling
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use std::pin::pin;
    /// # use tasmota_mqtt_client::{DeviceUpdate, Result, TasmotaClient};
    /// # use tokio::join;
    /// # use tokio_stream::StreamExt;
    /// #
    /// # #[tokio::main]
    /// # async fn main() -> Result<()> {
    ///     # let client = TasmotaClient::connect(
    ///     #     "mqtt.example.com",
    ///     #     1883,
    ///     #     Some(("mqtt_username", "mqtt_password")),
    ///     # ).await?;
    /// // let client: TasmotaClient = ...
    /// let mut discovery = pin!(client.devices());
    /// while let Some(update) = discovery.next().await {
    ///     match update {
    ///         DeviceUpdate::Added(device) => {
    ///             let (ip, name) = join!(client.device_ip(&device), client.device_name(&device));
    ///             println!("discovered {}({device}) with ip {}", name?, ip?);
    ///         }
    ///         DeviceUpdate::Removed(device) => {
    ///             println!("{device} has gone offline");
    ///         }
    ///     }
    /// }
    ///     # Ok(())
    /// # }
    /// ```
    pub fn devices(&self) -> impl Stream<Item = DeviceUpdate> {
        let current = self.current_devices();
        let rx = self.device_update.subscribe();

        tokio_stream::iter(current.into_iter().map(DeviceUpdate::Added))
            .chain(BroadcastStream::new(rx).filter_map(Result::ok))
    }

    /// Send a command that expect a single reply message
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use std::pin::pin;
    /// # use tasmota_mqtt_client::{DeviceUpdate, Result, TasmotaClient};
    /// # use tokio::join;
    /// # use tokio_stream::StreamExt;
    /// # use serde::Deserialize;
    /// #
    /// # #[tokio::main]
    /// # async fn main() -> Result<()> {
    ///     # let client = TasmotaClient::connect(
    ///     #     "mqtt.example.com",
    ///     #     1883,
    ///     #     Some(("mqtt_username", "mqtt_password")),
    ///     # ).await?;
    /// // let client: TasmotaClient = ...
    /// #[derive(Deserialize)]
    /// struct PowerResponse {
    ///     #[serde(rename = "POWER")]
    ///     power: String,
    /// }
    /// let response: PowerResponse = client.command("tasmota_device", "Power", "Off").await?;
    /// println!("power: {}", response.power);
    ///     # Ok(())
    /// # }
    /// ```
    #[tracing::instrument(skip(self))]
    pub async fn command<T: DeserializeOwned>(
        &self,
        device: &str,
        command: &str,
        payload: &str,
    ) -> Result<T> {
        let mut rx = self.mqtt.subscribe(format!("stat/{device}/RESULT")).await?;
        self.mqtt
            .send_str(&format!("cmnd/{device}/{command}"), payload)
            .await?;

        let reply = async {
            while let Some(msg) = rx.recv().await {
                if let Ok(response) = serde_json::from_slice(msg.payload.as_ref()) {
                    return Ok(response);
                }
            }

            Err(MqttError::Eof.into())
        };

        timeout(self.timeout, reply)
            .await
            .map_err(|_| Error::Timeout)?
    }

    /// Get the ip address for the device
    #[tracing::instrument(skip(self))]
    pub async fn device_ip(&self, device: &str) -> Result<IpAddr> {
        #[derive(Deserialize, Debug)]
        struct IpAddressResponse {
            #[serde(rename = "IPAddress1")]
            ip_address_1: String,
        }
        let response: IpAddressResponse = self.command(device, "IPADDRESS", "").await?;
        let raw = response.ip_address_1;

        let Some(Ok(ip)) = raw
            .split(' ')
            .map(|part| part.trim_start_matches('(').trim_end_matches(')'))
            .rev()
            .map(IpAddr::from_str)
            .next()
        else {
            return Err(Error::MalformedReply("device ip", raw));
        };

        Ok(ip)
    }

    /// Get the name for the device
    #[tracing::instrument(skip(self))]
    pub async fn device_name(&self, device: &str) -> Result<String> {
        #[derive(Deserialize, Debug)]
        struct NameResponse {
            #[serde(rename = "DeviceName")]
            device_name: String,
        }
        let response: NameResponse = self.command(device, "DeviceName", "").await?;
        Ok(response.device_name)
    }
}
