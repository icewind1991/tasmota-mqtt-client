mod download;
mod error;
mod mqtt;

use crate::download::download_config;
pub use crate::download::DownloadedFile;
use crate::mqtt::MqttHelper;
pub use error::{Error, Result};
use rumqttc::MqttOptions;

pub struct TasmotaClient {
    mqtt: MqttHelper,
}

impl TasmotaClient {
    pub fn connect(host: &str, port: u16, credentials: Option<(&str, &str)>) -> Result<Self> {
        let mut mqtt_opts = MqttOptions::new("tasmota-client", host, port);
        if let Some((username, password)) = credentials {
            mqtt_opts.set_credentials(username, password);
        }
        Ok(TasmotaClient {
            mqtt: MqttHelper::connect(mqtt_opts)?,
        })
    }

    pub async fn download_config(&self, client: &str, password: &str) -> Result<DownloadedFile> {
        download_config(&self.mqtt, client, password).await
    }
}
