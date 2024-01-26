use hex::FromHexError;
use rumqttc::{ClientError, ConnectionError};
use thiserror::Error;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Error with mqtt transport: {0:#}")]
    Mqtt(MqttError),
    #[error("Topic {0} doesn't follow expected format")]
    MalformedTopic(String),
    #[error("Malformed json payload received: {0:#}")]
    JsonPayload(serde_json::Error),
    #[error(transparent)]
    Download(#[from] DownloadError),
}

impl From<serde_json::Error> for Error {
    fn from(value: serde_json::Error) -> Self {
        Error::JsonPayload(value)
    }
}

#[derive(Debug, Error)]
pub enum MqttError {
    #[error("transparent")]
    Client(ClientError),
    #[error("transparent")]
    Connection(ConnectionError),
}

impl From<MqttError> for Error {
    fn from(value: MqttError) -> Self {
        Error::Mqtt(value)
    }
}

impl From<ClientError> for Error {
    fn from(value: ClientError) -> Self {
        MqttError::Client(value).into()
    }
}

impl From<ConnectionError> for Error {
    fn from(value: ConnectionError) -> Self {
        MqttError::Connection(value).into()
    }
}

#[derive(Debug, Error)]
pub enum DownloadError {
    #[error("Aborted")]
    DownloadAborted,
    #[error("Invalid password for device")]
    InvalidPassword,
    #[error("Bad chunk size")]
    BadChunkSize,
    #[error("Invalid file type")]
    InvalidFileType,
    #[error("Received error code: {0}")]
    Unknown(u32),
    #[error("Mismatched payload length, expected {0} got {1}")]
    MismatchedLength(u32, u32),
    #[error("Received an invalid md5 hash")]
    InvalidHash,
    #[error("Received data doesn't match the expected md5 hash, expected {0:x?} got {1:x?}")]
    MismatchedHash([u8; 16], [u8; 16]),
}

impl From<FromHexError> for DownloadError {
    fn from(_: FromHexError) -> Self {
        DownloadError::InvalidHash
    }
}
