use serde::Deserialize;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;
use url::Url;

pub const DEFAULT_MANIFEST_URL: &str = "https://blueism-firmware-repo.netlify.app/manifest.json";

#[derive(Clone, Debug, Deserialize)]
#[allow(dead_code)]
pub struct FirmwareRepositoryManifest {
    pub generated_at: Option<String>,
    #[serde(default)]
    pub devices: Vec<FirmwareRepositoryDevice>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct FirmwareRepositoryDevice {
    pub board_name: String,
    pub bootloader: String,
    #[serde(default)]
    pub firmwares: Vec<FirmwareRepositoryFirmware>,
}

#[derive(Clone, Debug, Deserialize)]
#[allow(dead_code)]
pub struct FirmwareRepositoryFirmware {
    pub version: String,
    pub channel: String,
    pub package: FirmwareRepositoryPackage,
    pub image: FirmwareRepositoryImage,
}

#[derive(Clone, Debug, Deserialize)]
pub struct FirmwareRepositoryPackage {
    pub file: String,
    pub url: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Clone, Debug, Deserialize)]
#[allow(dead_code)]
pub struct FirmwareRepositoryImage {
    pub file: String,
    pub size: u64,
    pub manifest_size: Option<u64>,
    pub target_slot: Option<u8>,
}

pub fn fetch_manifest(url: &str, timeout: Duration) -> Result<FirmwareRepositoryManifest, String> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(timeout)
        .timeout_read(timeout)
        .timeout_write(timeout)
        .build();

    let response = agent
        .get(url)
        .call()
        .map_err(|error| format!("Network request failed: {error}"))?;

    response
        .into_json::<FirmwareRepositoryManifest>()
        .map_err(|error| format!("Cannot parse firmware manifest: {error}"))
}

pub fn download_package_with_progress(
    manifest_url: &str,
    firmware: &FirmwareRepositoryFirmware,
    output_dir: &Path,
    timeout: Duration,
    mut progress_callback: impl FnMut(f32),
) -> Result<PathBuf, String> {
    let package_url = resolve_package_url(manifest_url, &firmware.package.url)?;
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(timeout)
        .timeout_read(timeout)
        .timeout_write(timeout)
        .build();
    let response = agent
        .get(package_url.as_str())
        .call()
        .map_err(|error| format!("Firmware download failed: {error}"))?;
    let mut reader = response.into_reader();
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|error| format!("Cannot read firmware package: {error}"))?;
        if read == 0 {
            break;
        }
        bytes.extend_from_slice(&buffer[..read]);
        if firmware.package.size > 0 {
            progress_callback((bytes.len() as f32 / firmware.package.size as f32).clamp(0.0, 1.0));
        }
    }
    progress_callback(1.0);

    if bytes.len() as u64 != firmware.package.size {
        return Err(format!(
            "Downloaded package size mismatch: expected {}, got {}",
            firmware.package.size,
            bytes.len()
        ));
    }

    let actual_sha256 = hex_digest(&sha256_digest(&bytes));
    if actual_sha256 != firmware.package.sha256 {
        return Err("Downloaded package SHA-256 mismatch".to_owned());
    }

    fs::create_dir_all(output_dir).map_err(|error| format!("Cannot create cache dir: {error}"))?;
    let output_path = output_dir.join(&firmware.package.file);
    fs::write(&output_path, bytes)
        .map_err(|error| format!("Cannot save firmware package: {error}"))?;
    Ok(output_path)
}

pub fn cached_package_path(
    firmware: &FirmwareRepositoryFirmware,
    cache_dir: &Path,
) -> Result<Option<PathBuf>, String> {
    let path = cache_dir.join(&firmware.package.file);
    if !path.exists() {
        return Ok(None);
    }

    let bytes = fs::read(&path).map_err(|error| format!("Cannot read cached firmware: {error}"))?;
    if bytes.len() as u64 != firmware.package.size {
        return Ok(None);
    }

    let actual_sha256 = hex_digest(&sha256_digest(&bytes));
    if actual_sha256 == firmware.package.sha256 {
        Ok(Some(path))
    } else {
        Ok(None)
    }
}

pub fn firmware_cache_dir() -> PathBuf {
    std::env::temp_dir()
        .join("blueism-configurator")
        .join("firmware")
}

pub fn clear_firmware_cache() -> Result<(), String> {
    let cache_dir = firmware_cache_dir();
    if !cache_dir.exists() {
        return Ok(());
    }

    fs::remove_dir_all(&cache_dir).map_err(|error| format!("Cannot clear cache: {error}"))
}

fn resolve_package_url(manifest_url: &str, package_url: &str) -> Result<Url, String> {
    if let Ok(url) = Url::parse(package_url) {
        return Ok(url);
    }

    Url::parse(manifest_url)
        .and_then(|base| base.join(package_url))
        .map_err(|error| format!("Invalid firmware package URL: {error}"))
}

fn sha256_digest(bytes: &[u8]) -> ring::digest::Digest {
    ring::digest::digest(&ring::digest::SHA256, bytes)
}

fn hex_digest(digest: &ring::digest::Digest) -> String {
    digest
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
