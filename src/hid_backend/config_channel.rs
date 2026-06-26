#![allow(dead_code)]

use hidapi::HidDevice;
use std::collections::BTreeMap;
use std::{thread, time::Duration};

const REPORT_ID: u8 = 7;
const REPORT_SIZE: usize = 30;
const EVENT_DATA_LEN_MAX: usize = REPORT_SIZE - 5;
pub const LOCAL_RECIPIENT: u8 = 0x00;
const INVALID_PEER_ID: u8 = 0xff;
const MOD_FIELD_POS: u8 = 4;
const OPT_MODULE_DESCR: u8 = 0x0;
const OPT_FIELD_MAX_OPT_CNT: usize = 0xf;
const END_OF_TRANSFER_CHAR: char = '\n';
const POLL_RETRY_COUNT: usize = 200;
const POLL_INTERVAL: Duration = Duration::from_millis(20);

#[derive(Clone, Debug, Default)]
pub struct ConfigTargetInfo {
    pub board_name: String,
    pub hwid: String,
    pub max_module_id: u8,
}

#[derive(Clone, Debug, Default)]
pub struct DetailedDeviceInfo {
    pub modules: Vec<String>,
    pub firmware: Option<FwInfo>,
    pub identity: Option<DevInfo>,
    pub bootloader_variant: Option<String>,
}

#[derive(Clone, Debug)]
pub struct FwInfo {
    pub flash_area_id: u8,
    pub image_len: u32,
    pub version: String,
}

#[derive(Clone, Debug)]
pub struct DevInfo {
    pub vendor_id: u16,
    pub product_id: u16,
    pub generation: String,
}

struct ModuleConfig {
    id: u8,
    options: BTreeMap<String, u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
enum ConfigStatus {
    Pending = 0,
    GetMaxModId = 1,
    GetHwid = 2,
    GetBoardName = 3,
    IndexPeers = 4,
    GetPeer = 5,
    Fetch = 7,
    Success = 8,
    Timeout = 9,
    Reject = 10,
    WriteFail = 11,
    Disconnected = 12,
}

impl ConfigStatus {
    fn from_byte(byte: u8) -> Result<Self, String> {
        match byte {
            0 => Ok(Self::Pending),
            1 => Ok(Self::GetMaxModId),
            2 => Ok(Self::GetHwid),
            3 => Ok(Self::GetBoardName),
            4 => Ok(Self::IndexPeers),
            5 => Ok(Self::GetPeer),
            7 => Ok(Self::Fetch),
            8 => Ok(Self::Success),
            9 => Ok(Self::Timeout),
            10 => Ok(Self::Reject),
            11 => Ok(Self::WriteFail),
            12 => Ok(Self::Disconnected),
            _ => Err(format!("Unknown config status {byte}")),
        }
    }
}

struct Response {
    recipient: u8,
    event_id: u8,
    status: ConfigStatus,
    data: Vec<u8>,
}

pub fn read_basic_info(device: &HidDevice) -> Result<ConfigTargetInfo, String> {
    read_basic_info_for_recipient(device, LOCAL_RECIPIENT)
}

pub fn read_basic_info_for_recipient(
    device: &HidDevice,
    recipient: u8,
) -> Result<ConfigTargetInfo, String> {
    let board_name = read_board_name(device, recipient)?;
    let hwid = read_hwid(device, recipient)?;
    let max_module_id = read_max_module_id(device, recipient)?;

    Ok(ConfigTargetInfo {
        board_name,
        hwid,
        max_module_id,
    })
}

pub fn read_connected_peers(device: &HidDevice) -> Result<Vec<u8>, String> {
    exchange(device, LOCAL_RECIPIENT, 0, ConfigStatus::IndexPeers, &[])?;

    let mut peers = Vec::new();
    loop {
        let data = exchange(device, LOCAL_RECIPIENT, 0, ConfigStatus::GetPeer, &[])?;
        let Some((&peer_id, _hwid)) = data.split_last() else {
            return Err("Device returned empty peer data".to_string());
        };

        if peer_id == INVALID_PEER_ID {
            break;
        }

        peers.push(peer_id);
        if peers.len() > INVALID_PEER_ID as usize {
            return Err("Device returned too many peers".to_string());
        }
    }

    Ok(peers)
}

pub fn read_detailed_info(
    device: &HidDevice,
    recipient: u8,
) -> Result<DetailedDeviceInfo, String> {
    let modules = discover_device_config(device, recipient)?;
    let module_names = modules.keys().cloned().collect::<Vec<_>>();
    let Some((dfu_module_name, dfu_module)) = modules
        .iter()
        .find(|(name, _module)| name.as_str() == "dfu" || name.starts_with("dfu/"))
    else {
        return Ok(DetailedDeviceInfo {
            modules: module_names,
            ..Default::default()
        });
    };

    let firmware = dfu_module
        .options
        .get("fwinfo")
        .and_then(|option_id| fetch_option(device, recipient, dfu_module.id, *option_id).ok())
        .and_then(|data| parse_fwinfo(&data).ok());
    let identity = dfu_module
        .options
        .get("devinfo")
        .and_then(|option_id| fetch_option(device, recipient, dfu_module.id, *option_id).ok())
        .and_then(|data| parse_devinfo(&data).ok());

    let bootloader_variant = dfu_module_name
        .strip_prefix("dfu/")
        .map(ToOwned::to_owned);

    Ok(DetailedDeviceInfo {
        modules: module_names,
        firmware,
        identity,
        bootloader_variant,
    })
}

fn discover_device_config(
    device: &HidDevice,
    recipient: u8,
) -> Result<BTreeMap<String, ModuleConfig>, String> {
    let max_module_id = read_max_module_id(device, recipient)?;
    let mut modules = BTreeMap::new();

    for module_id in 0..=max_module_id {
        let (module_name, module_config) = discover_module_config(device, recipient, module_id)?;
        modules.insert(module_name, module_config);
    }

    Ok(modules)
}

fn discover_module_config(
    device: &HidDevice,
    recipient: u8,
    module_id: u8,
) -> Result<(String, ModuleConfig), String> {
    let mut fetched_options = Vec::new();
    let mut end_of_transfer_idx = None;

    for fetch_idx in 0..=(OPT_FIELD_MAX_OPT_CNT + 1) {
        let option = fetch_module_description_option(device, recipient, module_id)?;

        if fetched_options.iter().any(|fetched| fetched == &option) {
            break;
        }

        if option.starts_with(END_OF_TRANSFER_CHAR) {
            end_of_transfer_idx = Some(fetched_options.len());
        }

        fetched_options.push(option);

        if fetch_idx > OPT_FIELD_MAX_OPT_CNT {
            return Err("Improper module description".to_string());
        }
    }

    let Some(end_of_transfer_idx) = end_of_transfer_idx else {
        return Err("Improper module description".to_string());
    };

    let mut ordered_options = fetched_options[end_of_transfer_idx..].to_vec();
    ordered_options.extend_from_slice(&fetched_options[..end_of_transfer_idx]);
    if ordered_options.len() < 2 {
        return Err("Improper module description".to_string());
    }

    ordered_options.remove(0);
    let mut module_name = ordered_options.remove(0);
    let mut options = BTreeMap::new();
    for (option_idx, option_name) in ordered_options.into_iter().enumerate() {
        options.insert(option_name, option_idx as u8 + 1);
    }

    if let Some(option_id) = options.get("module_variant") {
        let variant = fetch_option(device, recipient, module_id, *option_id)?;
        module_name = format!("{module_name}/{}", decode_string(&variant));
    }

    Ok((module_name, ModuleConfig { id: module_id, options }))
}

fn fetch_module_description_option(
    device: &HidDevice,
    recipient: u8,
    module_id: u8,
) -> Result<String, String> {
    let event_id = (module_id << MOD_FIELD_POS) | OPT_MODULE_DESCR;
    let data = exchange(device, recipient, event_id, ConfigStatus::Fetch, &[])?;
    Ok(decode_string(&data))
}

fn fetch_option(
    device: &HidDevice,
    recipient: u8,
    module_id: u8,
    option_id: u8,
) -> Result<Vec<u8>, String> {
    let event_id = (module_id << MOD_FIELD_POS) | option_id;
    exchange(device, recipient, event_id, ConfigStatus::Fetch, &[])
}

fn parse_fwinfo(data: &[u8]) -> Result<FwInfo, String> {
    if data.len() != 13 {
        return Err(format!("Invalid fwinfo length {}", data.len()));
    }

    let flash_area_id = data[0];
    let image_len = u32::from_le_bytes(data[1..5].try_into().unwrap());
    let ver_major = data[5];
    let ver_minor = data[6];
    let ver_rev = u16::from_le_bytes(data[7..9].try_into().unwrap());
    let ver_build_nr = u32::from_le_bytes(data[9..13].try_into().unwrap());

    Ok(FwInfo {
        flash_area_id,
        image_len,
        version: format!("{ver_major}.{ver_minor}.{ver_rev}.{ver_build_nr}"),
    })
}

fn parse_devinfo(data: &[u8]) -> Result<DevInfo, String> {
    if data.len() < 5 {
        return Err(format!("Invalid devinfo length {}", data.len()));
    }

    Ok(DevInfo {
        vendor_id: u16::from_le_bytes(data[0..2].try_into().unwrap()),
        product_id: u16::from_le_bytes(data[2..4].try_into().unwrap()),
        generation: decode_string(&data[4..]),
    })
}

fn read_board_name(device: &HidDevice, recipient: u8) -> Result<String, String> {
    let data = exchange(device, recipient, 0, ConfigStatus::GetBoardName, &[])?;
    Ok(decode_string(&data))
}

fn read_hwid(device: &HidDevice, recipient: u8) -> Result<String, String> {
    let data = exchange(device, recipient, 0, ConfigStatus::GetHwid, &[])?;
    Ok(data.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn read_max_module_id(device: &HidDevice, recipient: u8) -> Result<u8, String> {
    let data = exchange(device, recipient, 0, ConfigStatus::GetMaxModId, &[])?;
    data.first()
        .copied()
        .ok_or_else(|| "Device returned empty max module id".to_string())
}

fn exchange(
    device: &HidDevice,
    recipient: u8,
    event_id: u8,
    status: ConfigStatus,
    event_data: &[u8],
) -> Result<Vec<u8>, String> {
    if event_data.len() > EVENT_DATA_LEN_MAX {
        return Err("Config event payload is too long".to_string());
    }

    let mut report = [0u8; REPORT_SIZE];
    report[0] = REPORT_ID;
    report[1] = recipient;
    report[2] = event_id;
    report[3] = status as u8;
    report[4] = event_data.len() as u8;
    report[5..5 + event_data.len()].copy_from_slice(event_data);

    device
        .send_feature_report(&report)
        .map_err(|err| format!("send_feature_report failed: {err}"))?;

    for _ in 0..POLL_RETRY_COUNT {
        thread::sleep(POLL_INTERVAL);

        let mut response = [0u8; REPORT_SIZE];
        response[0] = REPORT_ID;
        let size = device
            .get_feature_report(&mut response)
            .map_err(|err| format!("get_feature_report failed: {err}"))?;

        let parsed = parse_response(&response[..size])?;

        if parsed.status == ConfigStatus::Pending {
            continue;
        }

        if parsed.status != ConfigStatus::Timeout
            && (parsed.recipient != recipient || parsed.event_id != event_id)
        {
            return Err("Feature response does not match request".to_string());
        }

        if parsed.status != ConfigStatus::Success {
            return Err(format!("Device returned status {:?}", parsed.status));
        }

        return Ok(parsed.data);
    }

    Err("Timed out waiting for device response".to_string())
}

fn parse_response(response: &[u8]) -> Result<Response, String> {
    let offset = if response.first() == Some(&REPORT_ID) { 1 } else { 0 };

    if response.len() < offset + 4 {
        return Err("Feature response is too short".to_string());
    }

    let recipient = response[offset];
    let event_id = response[offset + 1];
    let status = ConfigStatus::from_byte(response[offset + 2])?;
    let data_len = response[offset + 3] as usize;
    let data_start = offset + 4;
    let data_end = data_start + data_len;

    if data_end > response.len() {
        return Err("Feature response data length is invalid".to_string());
    }

    Ok(Response {
        recipient,
        event_id,
        status,
        data: response[data_start..data_end].to_vec(),
    })
}

fn decode_string(data: &[u8]) -> String {
    String::from_utf8_lossy(data)
        .trim_matches(char::from(0))
        .to_string()
}
