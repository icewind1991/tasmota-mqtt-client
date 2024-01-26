use clap::Parser;
use hex_fmt::HexFmt;
pub use tasmota_mqtt_client::{Result, TasmotaClient};

#[derive(Debug, Parser)]
struct Args {
    hostname: String,
    port: u16,
    username: String,
    password: String,
    device: String,
    device_password: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let client = TasmotaClient::connect(
        &args.hostname,
        args.port,
        Some((&args.username, &args.password)),
    )?;
    let file = client
        .download_config(&args.device, &args.device_password)
        .await?;

    println!("downloaded {} with hash {}", file.name, HexFmt(file.md5));
    if let Err(e) = std::fs::write(&file.name, file.data) {
        eprintln!("Error while saving {}: {:#}", file.name, e);
    }
    Ok(())
}
