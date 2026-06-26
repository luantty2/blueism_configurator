use std::sync::mpsc::Receiver;
use std::time::Instant;

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
    Loading { tab: DeviceTab, complete_at: Instant },
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

    pub fn label(self) -> &'static str {
        match self {
            Self::Info => "Info",
            Self::Firmware => "Firmware",
            Self::Operation => "Operation",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceType {
    Dongle,
    Unknown,
}

impl DeviceType {
    pub fn label(self) -> &'static str {
        match self {
            Self::Dongle => "Dongle",
            Self::Unknown => "Unknown",
        }
    }
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

impl ConnectionTarget {
    pub fn label(self) -> &'static str {
        match self {
            Self::DirectUsbDevice => "Direct USB device",
            Self::DirectBluetoothDevice => "Direct Bluetooth device",
            Self::DirectDevice => "Direct device",
            Self::PeerThroughDongle => "Peer through dongle",
        }
    }
}
