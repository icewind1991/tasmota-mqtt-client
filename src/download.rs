use crate::error::DownloadError;
use crate::mqtt::MqttHelper;
use crate::Result;
use bytes::{Bytes, BytesMut};
use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};
use tracing::debug;

#[derive(Serialize)]
struct SendDownloadPayload<'a> {
    password: &'a str,
    #[serde(rename = "type")]
    ty: u8,
    binary: u8,
}

#[derive(Default, Debug)]
struct DownloadState {
    name: String,
    size: u32,
    ty: u8,
    id: u32,
    data: BytesMut,
    md5: [u8; 16],
}

#[derive(Debug)]
pub struct DownloadedFile {
    pub name: String,
    pub data: Bytes,
    pub md5: [u8; 16],
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
struct DownloadResponse<'a> {
    file_download: Option<&'a str>,
    file: Option<&'a str>,
    size: Option<u32>,
    id: Option<u32>,
    #[serde(rename = "Type")]
    ty: Option<u8>,
    md5: Option<&'a str>,
}

pub async fn download_config(
    mqtt: &MqttHelper,
    client: &str,
    password: &str,
) -> Result<DownloadedFile> {
    let mut rx = mqtt
        .subscribe(format!("stat/{client}/FILEDOWNLOAD"))
        .await?;
    let topic = format!("cmnd/{client}/FILEDOWNLOAD");

    mqtt.send(
        &topic,
        &SendDownloadPayload {
            password,
            ty: 2,
            binary: 1,
        },
    )
    .await?;

    let mut state = DownloadState::default();

    loop {
        let msg = rx.recv().await.unwrap();

        if let Ok(response) = serde_json::from_slice::<DownloadResponse>(msg.payload.as_ref()) {
            debug!(message = ?response, "processing download status message");
            if let Some(status) = response.file_download {
                match status {
                    "Started" => {
                        // don't request another chunk, another response is on the way already
                        continue;
                    }
                    "Aborted" => {
                        return Err(DownloadError::DownloadAborted.into());
                    }
                    "Error 1" => {
                        return Err(DownloadError::InvalidPassword.into());
                    }
                    "Error 2" => {
                        return Err(DownloadError::BadChunkSize.into());
                    }
                    "Error 3" => {
                        return Err(DownloadError::InvalidFileType.into());
                    }
                    "Done" => {
                        break;
                    }
                    _ => {}
                }
            }
            if let Some(name) = response.file {
                state.name = name.to_string();
            }
            if let Some(size) = response.size {
                state.size = size;
            }
            if let Some(id) = response.id {
                state.id = id;
            }
            if let Some(ty) = response.ty {
                state.ty = ty;
            }
            if let Some(md5) = response.md5 {
                hex::decode_to_slice(md5, &mut state.md5[..]).map_err(DownloadError::from)?;

                // don't request another chunk
                continue;
            }
        } else {
            debug!(size = msg.payload.len(), "processing download chunk");
            state.data.extend(msg.payload);
        }

        mqtt.send_str(&topic, "?").await?;
    }

    if state.data.len() != state.size as usize {
        return Err(DownloadError::MismatchedLength(state.size, state.data.len() as u32).into());
    }

    let mut hasher = Md5::new();
    hasher.update(state.data.as_ref());
    let hash = hasher.finalize();

    if hash != state.md5.into() {
        return Err(DownloadError::MismatchedHash(state.md5, hash.into()).into());
    }

    Ok(DownloadedFile {
        name: state.name,
        data: state.data.freeze(),
        md5: state.md5,
    })
}
