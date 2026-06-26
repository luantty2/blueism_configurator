#![allow(dead_code)]

use crate::config_channel;
use hidapi::{HidApi, HidDevice};
use serde::Serialize;
use std::{collections::HashSet, ffi::CString};

const NORDIC_VID: u16 = 0x1915;

#[derive(Clone, Debug, Serialize)]
pub struct DeviceSummary {
    pub path: String,
    pub recipient: u8,
    pub vendor_id: u16,
    pub product_id: u16,
    pub product_string: String,
    pub manufacturer_string: String,
    pub serial_number: String,
    pub board_name: String,
    pub hwid: String,
    pub max_module_id: u8,
}

pub struct BlueismHid {
    api: HidApi,
}

impl BlueismHid {
    pub fn new() -> Result<Self, String> {
        HidApi::new()
            .map(|api| Self { api })
            .map_err(|err| err.to_string())
    }

    pub fn refresh(&mut self) -> Vec<DeviceSummary> {
        if self.api.refresh_devices().is_err() {
            return Vec::new();
        }

        scan_with_api(&self.api)
    }

    pub fn open(&self, path: &str) -> Result<HidDevice, String> {
        let path = CString::new(path).map_err(|err| err.to_string())?;
        let device = self
            .api
            .open_path(&path)
            .map_err(|err| err.to_string())?;
        Ok(device)
    }
}

pub fn scan_devices() -> Vec<DeviceSummary> {
    let Ok(mut api) = HidApi::new() else {
        return Vec::new();
    };
    if api.refresh_devices().is_err() {
        return Vec::new();
    }

    scan_with_api(&api)
}

fn scan_with_api(api: &HidApi) -> Vec<DeviceSummary> {
        let mut seen_hwids = HashSet::new();
        let mut devices = Vec::new();
        let candidates = api
            .device_list()
            .filter(|device| device.vendor_id() == NORDIC_VID)
            .map(|device| DeviceCandidate {
                path: device.path().to_string_lossy().to_string(),
                vendor_id: device.vendor_id(),
                product_id: device.product_id(),
                product_string: device.product_string().unwrap_or("").to_string(),
                manufacturer_string: device.manufacturer_string().unwrap_or("").to_string(),
                serial_number: device.serial_number().unwrap_or("").to_string(),
            })
            .collect::<Vec<_>>();

        for candidate in candidates {
            let Ok(opened_device) = open_with_api(api, &candidate.path) else {
                continue;
            };

            if let Ok(info) = config_channel::read_basic_info(&opened_device) {
                if seen_hwids.insert(info.hwid.clone()) {
                    devices.push(DeviceSummary {
                        path: candidate.path.clone(),
                        recipient: config_channel::LOCAL_RECIPIENT,
                        vendor_id: candidate.vendor_id,
                        product_id: candidate.product_id,
                        product_string: candidate.product_string.clone(),
                        manufacturer_string: candidate.manufacturer_string.clone(),
                        serial_number: candidate.serial_number.clone(),
                        board_name: info.board_name,
                        hwid: info.hwid,
                        max_module_id: info.max_module_id,
                    });
                }
            }

            let Ok(peer_recipients) = config_channel::read_connected_peers(&opened_device) else {
                continue;
            };
            for recipient in peer_recipients {
                let Ok(info) =
                    config_channel::read_basic_info_for_recipient(&opened_device, recipient)
                else {
                    continue;
                };
                if !seen_hwids.insert(info.hwid.clone()) {
                    continue;
                }

                devices.push(DeviceSummary {
                    path: candidate.path.clone(),
                    recipient,
                    vendor_id: candidate.vendor_id,
                    product_id: candidate.product_id,
                    product_string: candidate.product_string.clone(),
                    manufacturer_string: candidate.manufacturer_string.clone(),
                    serial_number: candidate.serial_number.clone(),
                    board_name: info.board_name,
                    hwid: info.hwid,
                    max_module_id: info.max_module_id,
                });
            }
        }

        devices
}

fn open_with_api(api: &HidApi, path: &str) -> Result<HidDevice, String> {
    let path = CString::new(path).map_err(|err| err.to_string())?;
    api.open_path(&path).map_err(|err| err.to_string())
}

struct DeviceCandidate {
    path: String,
    vendor_id: u16,
    product_id: u16,
    product_string: String,
    manufacturer_string: String,
    serial_number: String,
}
