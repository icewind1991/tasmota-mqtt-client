use hex::FromHexError;
use rumqttc::{ClientError, ConnectionError};
use thiserror::Error;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    #[error("Error with mqtt transport: {0:#}")]
    Mqtt(MqttError),
    #[error("Malformed json payload received: {0:#}")]
    JsonPayload(serde_json::Error),
    #[error(transparent)]
    Download(#[from] DownloadError),
    #[error("Malformed reply received from device for {0}: {1}")]
    MalformedReply(&'static str, String),
    #[error("Timeout while waiting for reply from device")]
    Timeout,
}

impl From<serde_json::Error> for Error {
    fn from(value: serde_json::Error) -> Self {
        Error::JsonPayload(value)
    }
}

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum MqttError {
    #[error("transparent")]
    Client(ClientError),
    #[error("transparent")]
    Connection(ConnectionError),
    #[error("connection closed unexpectedly")]
    Eof,
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
#[non_exhaustive]
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
    Unknown(String),
    #[error("Mismatched payload length, expected {0} got {1}")]
    MismatchedLength(u32, u32),
    #[error("Received an invalid md5 hash")]
    InvalidHash,
    #[error("Received data doesn't match the expected md5 hash, expected {0:x?} got {1:x?}")]
    MismatchedHash([u8; 16], [u8; 16]),
    #[error("Device has disconnected during the download")]
    Gone,
}

impl From<FromHexError> for DownloadError {
    fn from(_: FromHexError) -> Self {
        DownloadError::InvalidHash
    }
}
