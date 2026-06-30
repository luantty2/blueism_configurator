use crate::application::data::{
    DeviceInformation, FirmwarePackageCheck, FirmwareSummaryInformation,
    LocalFirmwarePackageInformation,
};
use serde::Deserialize;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use zip::ZipArchive;

const MCUBOOT_IMAGE_MAGIC: u32 = 0x96f3_b83d;

#[derive(Deserialize)]
struct DfuManifest {
    #[serde(rename = "format-version")]
    format_version: u8,
    files: Vec<DfuManifestFile>,
}

#[derive(Clone, Deserialize)]
struct DfuManifestFile {
    board: String,
    file: String,
    #[serde(default)]
    image_index: Option<String>,
    #[serde(default)]
    slot: Option<String>,
    #[serde(default)]
    size: Option<u64>,
    #[serde(flatten)]
    extra: serde_json::Map<String, serde_json::Value>,
}

pub fn analyze_local_firmware_package(
    path: &Path,
    device_information: &DeviceInformation,
) -> Result<LocalFirmwarePackageInformation, Vec<FirmwarePackageCheck>> {
    let mut checks = Vec::new();
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("Unavailable")
        .to_owned();
    push_pass(&mut checks, "Selected file", file_name);

    if !path.exists() {
        return fail(checks, "File exists", "File does not exist");
    }
    push_pass(&mut checks, "File exists", "Yes");

    let Some(device_details) = device_information.details.as_ref() else {
        return fail(
            checks,
            "Device firmware info",
            "Cannot get firmware information from device",
        );
    };
    let Some(device_firmware) = device_details.firmware.as_ref() else {
        return fail(
            checks,
            "Device firmware info",
            "Cannot get firmware information from device",
        );
    };
    push_pass(
        &mut checks,
        "Device FW version",
        device_firmware.version.clone(),
    );
    push_pass(
        &mut checks,
        "Device flash area ID",
        device_firmware.flash_area_id.to_string(),
    );

    let Some(device_bootloader) = device_details.bootloader_variant.as_deref() else {
        return fail(
            checks,
            "Device bootloader",
            "Cannot determine device bootloader variant",
        );
    };
    push_pass(&mut checks, "Device bootloader", device_bootloader);

    let file = match File::open(path) {
        Ok(file) => file,
        Err(_) => return fail(checks, "Open package", "Wrong file or file path"),
    };
    push_pass(&mut checks, "Open package", "OK");

    let mut archive = match ZipArchive::new(file) {
        Ok(archive) => archive,
        Err(_) => return fail(checks, "Package format", "Invalid DFU package format"),
    };
    push_pass(&mut checks, "Package format", "ZIP");

    let manifest = match read_manifest(&mut archive) {
        Ok(manifest) => manifest,
        Err(error) => return fail(checks, "Manifest", error),
    };
    push_pass(
        &mut checks,
        "Manifest format",
        manifest.format_version.to_string(),
    );

    let selected_file = match select_manifest_file(
        &manifest,
        device_firmware,
        &device_information.device.board_name,
        device_bootloader,
    ) {
        Ok(selected_file) => selected_file,
        Err(error) => return fail(checks, "Manifest selection", error),
    };
    push_pass(&mut checks, "Board name", selected_file.board.clone());
    push_pass(
        &mut checks,
        "Target slot",
        match target_slot_id(device_firmware.flash_area_id) {
            Ok(slot_id) => slot_id.to_string(),
            Err(error) => return fail(checks, "Target slot", error),
        },
    );
    push_pass(&mut checks, "Selected image", selected_file.file.clone());

    let image_bootloader = match bootloader_from_manifest_file(&selected_file) {
        Ok(bootloader) => bootloader,
        Err(error) => return fail(checks, "Image bootloader", error),
    };
    push_pass(&mut checks, "Image bootloader", image_bootloader);

    let image_data = match read_zip_entry(&mut archive, &selected_file.file) {
        Ok(image_data) => image_data,
        Err(error) => return fail(checks, "Read image", error),
    };
    push_pass(&mut checks, "Read image", "OK");

    if image_data.is_empty() {
        return fail(checks, "Image size", "DFU binary file is empty");
    }
    if image_data.len() > u32::MAX as usize {
        return fail(checks, "Image size", "Firmware image is too large");
    }
    push_pass(
        &mut checks,
        "Image size",
        format!("{} bytes", image_data.len()),
    );

    if selected_file
        .size
        .is_some_and(|expected_size| expected_size != image_data.len() as u64)
    {
        return fail(
            checks,
            "Manifest image size",
            "DFU binary size does not match manifest",
        );
    }
    if let Some(expected_size) = selected_file.size {
        push_pass(
            &mut checks,
            "Manifest image size",
            expected_size.to_string(),
        );
    }

    if let Err(error) = validate_image_data(&image_data, device_bootloader) {
        return fail(checks, "Image validation", error);
    }
    push_pass(&mut checks, "Image validation", "OK");

    let version = match version_from_manifest(&selected_file, device_bootloader) {
        Ok(version) => version,
        Err(error) => return fail(checks, "FW version from file", error),
    };
    push_pass(&mut checks, "FW version from file", version.clone());

    Ok(LocalFirmwarePackageInformation {
        path: path.to_path_buf(),
        selected_image_name: selected_file.file,
        version,
        image_data,
        checks,
    })
}

fn read_manifest(archive: &mut ZipArchive<File>) -> Result<DfuManifest, String> {
    let manifest_data = read_zip_entry(archive, "manifest.json")
        .map_err(|_| "No manifest.json found in DFU package".to_owned())?;
    serde_json::from_slice(&manifest_data).map_err(|_| "Cannot parse manifest.json".to_owned())
}

fn read_zip_entry(archive: &mut ZipArchive<File>, name: &str) -> Result<Vec<u8>, String> {
    let mut entry = archive
        .by_name(name)
        .map_err(|_| format!("DFU package is missing {name}"))?;
    let mut data = Vec::new();
    entry
        .read_to_end(&mut data)
        .map_err(|_| format!("Cannot read {name} from DFU package"))?;
    Ok(data)
}

fn select_manifest_file(
    manifest: &DfuManifest,
    device_firmware: &FirmwareSummaryInformation,
    device_board_name: &str,
    device_bootloader: &str,
) -> Result<DfuManifestFile, String> {
    let dfu_slot_id = target_slot_id(device_firmware.flash_area_id)?;

    let selected = match manifest.format_version {
        0 => select_format_v0_file(manifest, dfu_slot_id, device_bootloader),
        1 => select_format_v1_file(manifest, dfu_slot_id),
        other => return Err(format!("Unsupported manifest format-version {other}")),
    }?;

    let package_board_name = if manifest.format_version == 0 {
        selected.board.split('_').next().unwrap_or("").to_owned()
    } else {
        selected.board.clone()
    };
    if package_board_name != device_board_name {
        return Err(format!(
            "Update file is for other board: {package_board_name}"
        ));
    }

    let image_bootloader = bootloader_from_manifest_file(&selected)?;
    if image_bootloader != device_bootloader {
        return Err(format!(
            "Update file is for other bootloader: {image_bootloader}"
        ));
    }

    Ok(selected)
}

fn target_slot_id(flash_area_id: u8) -> Result<u8, String> {
    match flash_area_id {
        0 => Ok(1),
        1 => Ok(0),
        _ => Err("Invalid area ID in firmware info".to_owned()),
    }
}

fn push_pass(
    checks: &mut Vec<FirmwarePackageCheck>,
    name: impl Into<String>,
    value: impl Into<String>,
) {
    checks.push(FirmwarePackageCheck {
        name: name.into(),
        value: value.into(),
        passed: true,
    });
}

fn fail<T>(
    mut checks: Vec<FirmwarePackageCheck>,
    name: impl Into<String>,
    value: impl Into<String>,
) -> Result<T, Vec<FirmwarePackageCheck>> {
    checks.push(FirmwarePackageCheck {
        name: name.into(),
        value: value.into(),
        passed: false,
    });
    Err(checks)
}

fn select_format_v0_file(
    manifest: &DfuManifest,
    dfu_slot_id: u8,
    bootloader: &str,
) -> Result<DfuManifestFile, String> {
    let expected_name = dfu_image_name(dfu_slot_id, bootloader)?;
    let mut matches = manifest
        .files
        .iter()
        .filter(|file| file.file == expected_name);

    let selected = matches
        .next()
        .ok_or_else(|| "No suitable file entry found".to_owned())?;
    if matches.next().is_some() {
        return Err("Error: Multiple matching DFU images found in the archive".to_owned());
    }

    Ok(selected.clone())
}

fn select_format_v1_file(
    manifest: &DfuManifest,
    dfu_slot_id: u8,
) -> Result<DfuManifestFile, String> {
    if manifest.files.len() == 1 {
        return Ok(manifest.files[0].clone());
    }

    let slot_id = dfu_slot_id.to_string();
    let mut matches = manifest.files.iter().filter(|file| {
        file.image_index.as_deref() == Some("0") && file.slot.as_deref() == Some(slot_id.as_str())
    });

    let selected = matches
        .next()
        .ok_or_else(|| "No suitable file entry found".to_owned())?;
    if matches.next().is_some() {
        return Err("Error: Multiple matching DFU images found in the archive".to_owned());
    }

    Ok(selected.clone())
}

fn bootloader_from_manifest_file(file: &DfuManifestFile) -> Result<String, String> {
    let version_keys = file
        .extra
        .keys()
        .filter_map(|key| key.strip_prefix("version_"))
        .collect::<Vec<_>>();

    if version_keys.len() != 1 {
        return Err(
            "Invalid DFU zip manifest: improper version definition count for image".to_owned(),
        );
    }

    Ok(version_keys[0].to_owned())
}

fn version_from_manifest(file: &DfuManifestFile, bootloader: &str) -> Result<String, String> {
    let key = format!("version_{bootloader}");
    let version = file
        .extra
        .get(&key)
        .and_then(|value| value.as_str())
        .ok_or_else(|| "Cannot read image version from file".to_owned())?;

    Ok(version.replace('+', "."))
}

fn validate_image_data(image_data: &[u8], bootloader: &str) -> Result<(), String> {
    match bootloader {
        "MCUBOOT" | "MCUBOOT+XIP" => validate_mcuboot_image(image_data),
        "B0" => validate_b0_image(image_data),
        other => Err(format!("Device uses an unsupported bootloader {other}")),
    }
}

fn validate_mcuboot_image(image_data: &[u8]) -> Result<(), String> {
    let magic = read_u32_le(image_data, 0).ok_or_else(|| "DFU image is too short".to_owned())?;
    if magic != MCUBOOT_IMAGE_MAGIC {
        return Err("DFU image is invalid".to_owned());
    }

    let header_size =
        read_u16_le(image_data, 8).ok_or_else(|| "DFU image is too short".to_owned())? as usize;
    let image_size =
        read_u32_le(image_data, 12).ok_or_else(|| "DFU image is too short".to_owned())? as usize;
    if header_size == 0 || header_size + image_size > image_data.len() {
        return Err("DFU image is invalid".to_owned());
    }

    Ok(())
}

fn validate_b0_image(image_data: &[u8]) -> Result<(), String> {
    if b0_fwinfo_offset(image_data).is_some() {
        Ok(())
    } else {
        Err("Invalid image format".to_owned())
    }
}

fn b0_fwinfo_offset(image_data: &[u8]) -> Option<usize> {
    const MAGIC_COMMON: u32 = 0x281e_e6de;
    const MAGIC_FWINFO: u32 = 0x8fce_bb4c;
    const COMPATIBILITY: [u32; 2] = [0x0000_3402, 0x0000_3502];
    const HEADER_OFFSETS: [usize; 5] = [0x0000, 0x0200, 0x0400, 0x0800, 0x1000];

    HEADER_OFFSETS.into_iter().find(|offset| {
        read_u32_le(image_data, *offset) == Some(MAGIC_COMMON)
            && read_u32_le(image_data, *offset + 4) == Some(MAGIC_FWINFO)
            && read_u32_le(image_data, *offset + 8)
                .is_some_and(|compatibility| COMPATIBILITY.contains(&compatibility))
    })
}

fn dfu_image_name(dfu_slot_id: u8, bootloader: &str) -> Result<String, String> {
    match bootloader {
        "MCUBOOT" => Ok("app_update.bin".to_owned()),
        "MCUBOOT+XIP" => match dfu_slot_id {
            0 => Ok("app_update.bin".to_owned()),
            1 => Ok("mcuboot_secondary_app_update.bin".to_owned()),
            _ => Err("Invalid DFU slot ID".to_owned()),
        },
        "B0" => Ok(format!("signed_by_b0_s{dfu_slot_id}_image.bin")),
        other => Err(format!("Device uses an unsupported bootloader {other}")),
    }
}

fn read_u16_le(data: &[u8], offset: usize) -> Option<u16> {
    let bytes = data.get(offset..offset + 2)?;
    Some(u16::from_le_bytes(bytes.try_into().ok()?))
}

fn read_u32_le(data: &[u8], offset: usize) -> Option<u32> {
    let bytes = data.get(offset..offset + 4)?;
    Some(u32::from_le_bytes(bytes.try_into().ok()?))
}
