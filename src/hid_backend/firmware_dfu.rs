use crate::hid_backend::{config_channel, device_discovery::BlueismHid};
use crc32fast::Hasher;
use hidapi::HidDevice;
use std::thread;
use std::time::Duration;

const DFU_SYNC_INTERVAL: Duration = Duration::from_secs(1);

pub fn transfer_firmware_image<F>(
    device_path: &str,
    recipient: u8,
    image_data: &[u8],
    mut progress_callback: F,
) -> Result<(), String>
where
    F: FnMut(f32),
{
    if image_data.is_empty() {
        return Err("DFU image is empty".to_owned());
    }
    if image_data.len() > u32::MAX as usize {
        return Err("Firmware image is too large".to_owned());
    }

    let hid = BlueismHid::new()?;
    let device = hid.open(device_path)?;
    transfer_with_open_device(&device, recipient, image_data, &mut progress_callback)
}

fn transfer_with_open_device<F>(
    device: &HidDevice,
    recipient: u8,
    image_data: &[u8],
    progress_callback: &mut F,
) -> Result<(), String>
where
    F: FnMut(f32),
{
    let image_length = image_data.len() as u32;
    let image_checksum = file_crc(image_data);
    let dfu_config = config_channel::discover_dfu_config(device, recipient)?;

    let mut dfu_info;
    let mut offset;
    loop {
        dfu_info = dfu_sync_wait_until_inactive(device, recipient, &dfu_config)?;
        if is_dfu_operation_pending(&dfu_info) {
            return Err("Cannot start DFU. DFU in progress or memory is not clean.".to_owned());
        }

        offset = get_dfu_operation_offset(image_data, &dfu_info, image_checksum);
        config_channel::dfu_start_with_config(
            device,
            recipient,
            &dfu_config,
            image_length,
            image_checksum,
            offset,
        )?;

        dfu_info = config_channel::dfu_sync_with_config(device, recipient, &dfu_config)?;
        if dfu_info.is_started() {
            break;
        }
    }

    offset = send_chunks(
        device,
        recipient,
        image_data,
        image_checksum,
        image_length,
        offset,
        u32::from(dfu_info.sync_buffer_size),
        &dfu_config,
        progress_callback,
    )?;

    let dfu_info = dfu_sync_wait_until_inactive(device, recipient, &dfu_config)?;
    if dfu_info.is_busy() {
        return Err("Device holds DFU active".to_owned());
    }
    if dfu_info.offset != offset {
        return Err(format!(
            "Offset {offset} does not match device info offset {}",
            dfu_info.offset
        ));
    }

    progress_callback(1.0);
    config_channel::dfu_reboot_with_config(device, recipient, &dfu_config)?;
    Ok(())
}

fn send_chunks<F>(
    device: &HidDevice,
    recipient: u8,
    image_data: &[u8],
    image_checksum: u32,
    image_length: u32,
    mut offset: u32,
    sync_buffer_size: u32,
    dfu_config: &config_channel::DfuModuleConfig,
    progress_callback: &mut F,
) -> Result<u32, String>
where
    F: FnMut(f32),
{
    if sync_buffer_size == 0 {
        return Err("Device returned zero DFU sync buffer size".to_owned());
    }

    let mut next_checkpoint = offset.saturating_add(sync_buffer_size).min(image_length);

    while offset < image_length {
        progress_callback(offset as f32 / image_length as f32);

        let remaining_to_checkpoint = next_checkpoint - offset;
        let chunk_len = remaining_to_checkpoint.min(config_channel::EVENT_DATA_LEN_MAX as u32);
        let chunk_start = offset as usize;
        let chunk_end = chunk_start + chunk_len as usize;
        let chunk = image_data
            .get(chunk_start..chunk_end)
            .ok_or_else(|| "Invalid DFU image chunk range".to_owned())?;

        config_channel::dfu_send_data_with_config(device, recipient, dfu_config, chunk)?;
        offset += chunk_len;
        progress_callback(offset as f32 / image_length as f32);

        if offset >= next_checkpoint {
            dfu_checkpoint(
                device,
                recipient,
                dfu_config,
                image_checksum,
                image_length,
                offset,
            )?;
            next_checkpoint = offset.saturating_add(sync_buffer_size).min(image_length);
        }
    }

    Ok(offset)
}

fn dfu_checkpoint(
    device: &HidDevice,
    recipient: u8,
    dfu_config: &config_channel::DfuModuleConfig,
    image_checksum: u32,
    image_length: u32,
    offset: u32,
) -> Result<(), String> {
    let mut store_retry = 0;
    let mut sleep_time = Duration::from_millis(300);

    loop {
        let dfu_info = config_channel::dfu_sync_with_config(device, recipient, dfu_config)?;
        if dfu_info.image_length != image_length || dfu_info.image_checksum != image_checksum {
            return Err("Invalid sync information".to_owned());
        }
        if !dfu_info.is_busy() && dfu_info.offset != image_length {
            return Err("DFU interrupted by device".to_owned());
        }
        if dfu_info.is_storing() {
            store_retry += 1;
            if store_retry % 8 == 0 {
                sleep_time = (sleep_time + sleep_time).min(DFU_SYNC_INTERVAL);
            }
            thread::sleep(sleep_time);
            continue;
        }
        if dfu_info.is_busy() && dfu_info.offset != offset {
            return Err(format!(
                "Mismatching offset after synchronization {} != {offset}",
                dfu_info.offset
            ));
        }

        return Ok(());
    }
}

fn dfu_sync_wait_until_inactive(
    device: &HidDevice,
    recipient: u8,
    dfu_config: &config_channel::DfuModuleConfig,
) -> Result<config_channel::DfuInfo, String> {
    loop {
        let dfu_info = config_channel::dfu_sync_with_config(device, recipient, dfu_config)?;
        if dfu_info.is_busy() {
            thread::sleep(DFU_SYNC_INTERVAL);
        } else {
            return Ok(dfu_info);
        }
    }
}

fn get_dfu_operation_offset(
    image_data: &[u8],
    dfu_info: &config_channel::DfuInfo,
    image_checksum: u32,
) -> u32 {
    let image_length = image_data.len() as u32;

    if dfu_info.image_length == image_length
        && dfu_info.image_checksum == image_checksum
        && dfu_info.offset <= image_length
    {
        dfu_info.offset
    } else {
        0
    }
}

fn is_dfu_operation_pending(dfu_info: &config_channel::DfuInfo) -> bool {
    dfu_info.is_busy()
}

fn file_crc(image_data: &[u8]) -> u32 {
    let mut hasher = Hasher::new_with_initial(1);
    hasher.update(image_data);
    hasher.finalize()
}
