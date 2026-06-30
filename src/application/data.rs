use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::time::Instant;

use crate::hid_backend::firmware_repository::FirmwareRepositoryManifest;

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct DiscoveredHidDevice {
    pub path: String,
    pub recipient: u8,
    pub connection_bus: HidConnectionBus,
    pub vendor_id: u16,
    pub product_id: u16,
    pub product_string: String,
    pub manufacturer_string: String,
    pub serial_number: String,
    pub board_name: String,
    pub hwid: String,
    pub max_module_id: u8,
}

#[derive(Clone, Debug)]
pub struct DeviceInformation {
    pub device: DiscoveredHidDevice,
    pub details: Option<DeviceInformationDetails>,
}

#[derive(Clone, Debug, Default)]
pub struct DeviceInformationDetails {
    pub modules: Vec<String>,
    pub identity: Option<DeviceIdentityInformation>,
    pub firmware: Option<FirmwareSummaryInformation>,
    pub bootloader_variant: Option<String>,
}

#[derive(Clone, Debug)]
pub struct DeviceIdentityInformation {
    pub vendor_id: u16,
    pub product_id: u16,
    pub generation: String,
}

#[derive(Clone, Debug)]
pub struct FirmwareSummaryInformation {
    pub flash_area_id: u8,
    pub image_len: u32,
    pub version: String,
}

#[derive(Clone, Debug)]
pub enum LocalFirmwarePackageStatus {
    NotSelected,
    Valid(LocalFirmwarePackageInformation),
    Invalid(Vec<FirmwarePackageCheck>),
}

#[derive(Debug)]
pub enum NetworkFirmwareRepositoryStatus {
    Idle,
    Loading {
        started_at: Instant,
        receiver: Receiver<Result<FirmwareRepositoryManifest, String>>,
    },
    Loaded(FirmwareRepositoryManifest),
    Failed(String),
}

#[derive(Debug)]
pub enum NetworkFirmwarePackageStatus {
    Idle,
    Downloading {
        progress: f32,
        started_at: Instant,
        receiver: Receiver<NetworkFirmwarePackageUpdate>,
    },
    Failed(String),
}

#[derive(Debug)]
pub enum NetworkFirmwarePackageUpdate {
    Progress(f32),
    Succeeded(LocalFirmwarePackageInformation),
    Failed(Vec<FirmwarePackageCheck>),
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct LocalFirmwarePackageInformation {
    pub path: PathBuf,
    pub selected_image_name: String,
    pub version: String,
    pub image_data: Vec<u8>,
    pub checks: Vec<FirmwarePackageCheck>,
}

#[derive(Clone, Debug)]
pub struct FirmwarePackageCheck {
    pub name: String,
    pub value: String,
    pub passed: bool,
}

#[derive(Debug)]
pub enum FirmwareFlashStatus {
    Idle,
    Flashing {
        progress: f32,
        started_at: Instant,
        receiver: Receiver<FirmwareFlashUpdate>,
    },
    Succeeded,
    Failed(String),
}

#[derive(Debug)]
pub enum FirmwareFlashUpdate {
    Progress(f32),
    Succeeded,
    Failed(String),
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct DeviceCapabilities {
    pub supported_features: String,
}

pub enum DeviceConnectionStatus {
    Disconnected,
    Connecting {
        complete_at: Instant,
        requested_hwid: String,
        receiver: Receiver<Vec<DiscoveredHidDevice>>,
    },
    Connected,
}

pub enum DeviceRefreshStatus {
    Idle,
    Refreshing {
        started_at: Instant,
        minimum_until: Instant,
        receiver: Receiver<Vec<DiscoveredHidDevice>>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabContentLoadingStatus {
    Idle,
    Loading {
        tab: DeviceTab,
        complete_at: Instant,
    },
    Loaded,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceTab {
    Info,
    Firmware,
    Operation,
}

impl DeviceTab {
    pub const ALL: [Self; 3] = [Self::Info, Self::Firmware, Self::Operation];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FirmwareSource {
    Network,
    Local,
}

impl FirmwareSource {
    pub const ALL: [Self; 2] = [Self::Network, Self::Local];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HidConnectionBus {
    Usb,
    Bluetooth,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectionTarget {
    DirectUsbDevice,
    DirectBluetoothDevice,
    DirectDevice,
    PeerThroughDongle,
}
