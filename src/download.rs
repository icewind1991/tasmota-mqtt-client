use crate::error::DownloadError;
use crate::mqtt::MqttHelper;
use crate::Result;
use bytes::{Bytes, BytesMut};
use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};
use serde_json::Value;

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

        if let Ok(body) = serde_json::from_slice::<Value>(msg.payload.as_ref()) {
            if let Some(status) = body.get("FileDownload") {
                match status.as_str() {
                    Some("Started") => {
                        continue;
                    }
                    Some("Aborted") => {
                        return Err(DownloadError::DownloadAborted.into());
                    }
                    Some("Error 1") => {
                        return Err(DownloadError::InvalidPassword.into());
                    }
                    Some("Error 2") => {
                        return Err(DownloadError::BadChunkSize.into());
                    }
                    Some("Error 3") => {
                        return Err(DownloadError::InvalidFileType.into());
                    }
                    Some("Done") => {
                        break;
                    }
                    _ => {}
                }
            }
            if let Some(name) = body.get("File").and_then(|v| v.as_str()) {
                state.name = name.to_string();
            }
            if let Some(size) = body.get("Size").and_then(|v| v.as_u64()) {
                state.size = size as u32;
            }
            if let Some(id) = body.get("Size").and_then(|v| v.as_u64()) {
                state.id = id as u32;
            }
            if let Some(ty) = body.get("Type").and_then(|v| v.as_u64()) {
                state.ty = ty as u8;
            }
            if let Some(md5) = body.get("Md5").and_then(|v| v.as_str()) {
                hex::decode_to_slice(md5, &mut state.md5[..]).map_err(DownloadError::from)?;
            }
        } else {
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
    })
}
