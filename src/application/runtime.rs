use crate::application::data::{
    DeviceConnectionStatus, DeviceInformation, DeviceRefreshStatus, DeviceTab,
    DiscoveredHidDevice, TabContentLoadingStatus,
};
use crate::hid_backend::{device_discovery, device_discovery::BlueismHid, device_information};
use std::sync::mpsc::{self, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

pub struct ConfiguratorRuntime {
    hid: Option<BlueismHid>,
    devices: Vec<DiscoveredHidDevice>,
    selected_device: Option<usize>,
    connection_status: DeviceConnectionStatus,
    active_tab: DeviceTab,
    tab_loading_status: TabContentLoadingStatus,
    loaded_device_information: Option<DeviceInformation>,
    refresh_status: DeviceRefreshStatus,
}

impl ConfiguratorRuntime {
    pub fn new(hid: Option<BlueismHid>) -> Self {
        let mut runtime = Self {
            hid,
            devices: Vec::new(),
            selected_device: None,
            connection_status: DeviceConnectionStatus::Disconnected,
            active_tab: DeviceTab::Info,
            tab_loading_status: TabContentLoadingStatus::Idle,
            loaded_device_information: None,
            refresh_status: DeviceRefreshStatus::Idle,
        };

        runtime.start_device_refresh();
        runtime
    }

    pub fn poll_background_tasks(&mut self) {
        self.complete_connection_if_ready();
        self.complete_tab_loading_if_ready();
        self.apply_refresh_result_if_ready();
    }

    pub fn start_device_refresh(&mut self) {
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let devices = device_discovery::scan_devices();
            let _ = sender.send(devices);
        });

        let now = Instant::now();
        self.refresh_status = DeviceRefreshStatus::Refreshing {
            started_at: now,
            minimum_until: now + Duration::from_secs(1),
            receiver,
        };
    }

    pub fn select_device(&mut self, index: usize) {
        if self.can_change_selected_device() {
            self.selected_device = Some(index);
            self.connection_status = DeviceConnectionStatus::Disconnected;
            self.tab_loading_status = TabContentLoadingStatus::Idle;
            self.loaded_device_information = None;
        }
    }

    pub fn start_connection(&mut self) {
        if self.can_press_connection_button() {
            let Some(device) = self.selected_device() else {
                return;
            };
            let requested_hwid = device.hwid.clone();
            let (sender, receiver) = mpsc::channel();
            thread::spawn(move || {
                let devices = device_discovery::scan_devices();
                let _ = sender.send(devices);
            });

            self.connection_status = DeviceConnectionStatus::Connecting {
                complete_at: Instant::now() + Duration::from_secs(2),
                requested_hwid,
                receiver,
            };
        }
    }

    pub fn disconnect_current_device(&mut self) {
        self.connection_status = DeviceConnectionStatus::Disconnected;
        self.tab_loading_status = TabContentLoadingStatus::Idle;
        self.loaded_device_information = None;
    }

    pub fn select_tab(&mut self, tab: DeviceTab) {
        self.active_tab = tab;
        if tab == DeviceTab::Info {
            self.loaded_device_information = None;
        }
        self.tab_loading_status = TabContentLoadingStatus::Loading {
            tab,
            complete_at: Instant::now() + Duration::from_secs(1),
        };
    }

    pub fn selected_device_label(&self) -> String {
        self.selected_device()
            .map(format_discovered_device_label)
            .unwrap_or_else(|| "No device selected".to_owned())
    }

    pub fn can_refresh_devices(&self) -> bool {
        !self.is_refreshing() && !self.is_connected() && !self.is_connecting()
    }

    pub fn can_change_selected_device(&self) -> bool {
        !self.is_refreshing() && !self.is_connected() && !self.is_connecting()
    }

    pub fn can_press_connection_button(&self) -> bool {
        self.selected_device.is_some() && !self.is_connecting() && !self.is_refreshing()
    }

    pub fn devices(&self) -> &[DiscoveredHidDevice] {
        &self.devices
    }

    pub fn selected_device_index(&self) -> Option<usize> {
        self.selected_device
    }

    pub fn connection_status(&self) -> &DeviceConnectionStatus {
        &self.connection_status
    }

    pub fn refresh_status(&self) -> &DeviceRefreshStatus {
        &self.refresh_status
    }

    pub fn active_tab(&self) -> DeviceTab {
        self.active_tab
    }

    pub fn tab_loading_status(&self) -> TabContentLoadingStatus {
        self.tab_loading_status
    }

    pub fn loaded_device_information(&self) -> Option<&DeviceInformation> {
        self.loaded_device_information.as_ref()
    }

    pub fn is_refreshing(&self) -> bool {
        matches!(self.refresh_status, DeviceRefreshStatus::Refreshing { .. })
    }

    pub fn is_connecting(&self) -> bool {
        matches!(self.connection_status, DeviceConnectionStatus::Connecting { .. })
    }

    pub fn is_connected(&self) -> bool {
        matches!(self.connection_status, DeviceConnectionStatus::Connected)
    }

    fn selected_device(&self) -> Option<&DiscoveredHidDevice> {
        self.selected_device
            .and_then(|index| self.devices.get(index))
    }

    fn apply_scanned_devices(&mut self, devices: Vec<DiscoveredHidDevice>) {
        self.devices = devices;
        if self
            .selected_device
            .is_some_and(|index| index >= self.devices.len())
        {
            self.selected_device = None;
        }
    }

    fn complete_connection_if_ready(&mut self) {
        let scan_result = match &self.connection_status {
            DeviceConnectionStatus::Connecting {
                complete_at,
                receiver,
                ..
            } if Instant::now() >= *complete_at => match receiver.try_recv() {
                Ok(devices) => Some(devices),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => Some(Vec::new()),
            },
            DeviceConnectionStatus::Disconnected
            | DeviceConnectionStatus::Connecting { .. }
            | DeviceConnectionStatus::Connected => None,
        };

        let Some(devices) = scan_result else {
            return;
        };

        let requested_hwid = match &self.connection_status {
            DeviceConnectionStatus::Connecting { requested_hwid, .. } => requested_hwid.clone(),
            _ => return,
        };

        let selected_device = devices
            .iter()
            .position(|device| device.hwid == requested_hwid);

        self.apply_scanned_devices(devices);

        if let Some(index) = selected_device {
            self.selected_device = Some(index);
            self.connection_status = DeviceConnectionStatus::Connected;
            self.active_tab = DeviceTab::Info;
            self.tab_loading_status = TabContentLoadingStatus::Loading {
                tab: DeviceTab::Info,
                complete_at: Instant::now() + Duration::from_secs(1),
            };
        } else {
            self.selected_device = None;
            self.connection_status = DeviceConnectionStatus::Disconnected;
            self.tab_loading_status = TabContentLoadingStatus::Idle;
            self.loaded_device_information = None;
        }
    }

    fn complete_tab_loading_if_ready(&mut self) {
        if matches!(
            self.tab_loading_status,
            TabContentLoadingStatus::Loading { complete_at, .. } if Instant::now() >= complete_at
        ) {
            if matches!(
                self.tab_loading_status,
                TabContentLoadingStatus::Loading {
                    tab: DeviceTab::Info,
                    ..
                }
            ) {
                self.load_selected_device_information();
            }
            self.tab_loading_status = TabContentLoadingStatus::Loaded;
        }
    }

    fn apply_refresh_result_if_ready(&mut self) {
        let refresh_result = match &self.refresh_status {
            DeviceRefreshStatus::Refreshing {
                minimum_until,
                receiver,
                ..
            } if Instant::now() >= *minimum_until => match receiver.try_recv() {
                Ok(devices) => Some(devices),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => Some(Vec::new()),
            },
            DeviceRefreshStatus::Idle | DeviceRefreshStatus::Refreshing { .. } => None,
        };

        if let Some(devices) = refresh_result {
            self.refresh_status = DeviceRefreshStatus::Idle;
            self.apply_scanned_devices(devices);
        }
    }

    fn load_selected_device_information(&mut self) {
        self.loaded_device_information = self
            .selected_device()
            .map(|device| device_information::read_device_information(self.hid.as_ref(), device));
    }
}

pub fn format_discovered_device_label(device: &DiscoveredHidDevice) -> String {
    format!("{} (HW ID: {})", device.board_name, device.hwid)
}
