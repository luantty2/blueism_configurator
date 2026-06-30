use crate::application::data::{
    DeviceConnectionStatus, DeviceInformation, DeviceRefreshStatus, DeviceTab, DiscoveredHidDevice,
    FirmwareFlashStatus, FirmwareFlashUpdate, FirmwareSource, LocalFirmwarePackageStatus,
    NetworkFirmwarePackageStatus, NetworkFirmwarePackageUpdate, NetworkFirmwareRepositoryStatus,
    TabContentLoadingStatus,
};
use crate::hid_backend::{
    config_channel, device_discovery, device_discovery::BlueismHid, device_information,
    firmware_dfu, firmware_package, firmware_repository,
};
use std::path::PathBuf;
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
    firmware_source: FirmwareSource,
    local_firmware_package_status: LocalFirmwarePackageStatus,
    network_firmware_repository_status: NetworkFirmwareRepositoryStatus,
    network_firmware_package_status: NetworkFirmwarePackageStatus,
    selected_network_firmware: Option<usize>,
    firmware_flash_status: FirmwareFlashStatus,
    refresh_status: DeviceRefreshStatus,
    reconnect_after_refresh_hwid: Option<String>,
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
            firmware_source: FirmwareSource::Network,
            local_firmware_package_status: LocalFirmwarePackageStatus::NotSelected,
            network_firmware_repository_status: NetworkFirmwareRepositoryStatus::Idle,
            network_firmware_package_status: NetworkFirmwarePackageStatus::Idle,
            selected_network_firmware: None,
            firmware_flash_status: FirmwareFlashStatus::Idle,
            refresh_status: DeviceRefreshStatus::Idle,
            reconnect_after_refresh_hwid: None,
        };

        runtime.start_device_refresh();
        runtime
    }

    pub fn poll_background_tasks(&mut self) {
        self.complete_connection_if_ready();
        self.complete_tab_loading_if_ready();
        self.apply_refresh_result_if_ready();
        self.apply_network_firmware_repository_result_if_ready();
        self.apply_network_firmware_package_result_if_ready();
        self.apply_firmware_flash_update_if_ready();
    }

    pub fn start_device_refresh(&mut self) {
        self.start_device_refresh_with_minimum(Duration::from_secs(1));
    }

    fn start_device_refresh_with_minimum(&mut self, minimum_duration: Duration) {
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let devices = device_discovery::scan_devices();
            let _ = sender.send(devices);
        });

        let now = Instant::now();
        self.refresh_status = DeviceRefreshStatus::Refreshing {
            started_at: now,
            minimum_until: now + minimum_duration,
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
            self.start_connection_for_hwid(requested_hwid, Duration::from_secs(2));
        }
    }

    pub fn disconnect_current_device(&mut self) {
        self.connection_status = DeviceConnectionStatus::Disconnected;
        self.tab_loading_status = TabContentLoadingStatus::Idle;
        self.loaded_device_information = None;
    }

    pub fn reset_current_device(&mut self) {
        if self.is_firmware_flashing() {
            return;
        }

        let Some(device) = self.selected_device().cloned() else {
            return;
        };

        thread::spawn(move || {
            if let Ok(hid) = BlueismHid::new() {
                if let Ok(opened_device) = hid.open(&device.path) {
                    if let Ok(dfu_config) =
                        config_channel::discover_dfu_config(&opened_device, device.recipient)
                    {
                        let _ = config_channel::dfu_reboot_with_config(
                            &opened_device,
                            device.recipient,
                            &dfu_config,
                        );
                    }
                }
            }
        });

        self.connection_status = DeviceConnectionStatus::Disconnected;
        self.tab_loading_status = TabContentLoadingStatus::Idle;
        self.loaded_device_information = None;
        self.start_device_refresh_with_minimum(Duration::from_secs(2));
    }

    pub fn select_tab(&mut self, tab: DeviceTab) {
        if self.active_tab == DeviceTab::Firmware && tab != DeviceTab::Firmware {
            self.reset_firmware_tab_state();
        }

        self.active_tab = tab;
        if tab == DeviceTab::Info {
            self.loaded_device_information = None;
        }
        self.tab_loading_status = TabContentLoadingStatus::Loading {
            tab,
            complete_at: Instant::now() + Duration::from_secs(1),
        };
    }

    pub fn select_firmware_source(&mut self, source: FirmwareSource) {
        self.firmware_source = source;
        self.firmware_flash_status = FirmwareFlashStatus::Idle;
        self.local_firmware_package_status = LocalFirmwarePackageStatus::NotSelected;
        self.network_firmware_package_status = NetworkFirmwarePackageStatus::Idle;
        self.selected_network_firmware = None;
        if source == FirmwareSource::Network {
            self.start_network_firmware_repository_fetch();
        }
    }

    pub fn select_local_firmware_package(&mut self, path: PathBuf) {
        self.firmware_flash_status = FirmwareFlashStatus::Idle;
        let Some(device) = self.selected_device().cloned() else {
            self.local_firmware_package_status = LocalFirmwarePackageStatus::Invalid(vec![
                crate::application::data::FirmwarePackageCheck {
                    name: "Selected device".to_owned(),
                    value: "No device selected".to_owned(),
                    passed: false,
                },
            ]);
            return;
        };

        let device_information =
            device_information::read_device_information(self.hid.as_ref(), &device);
        self.local_firmware_package_status =
            match firmware_package::analyze_local_firmware_package(&path, &device_information) {
                Ok(package) => LocalFirmwarePackageStatus::Valid(package),
                Err(error) => LocalFirmwarePackageStatus::Invalid(error),
            };
    }

    pub fn start_firmware_flash(&mut self) {
        if self.can_flash_firmware() {
            let Some(device) = self.selected_device().cloned() else {
                self.firmware_flash_status =
                    FirmwareFlashStatus::Failed("No device selected".to_owned());
                return;
            };
            let LocalFirmwarePackageStatus::Valid(package) =
                self.local_firmware_package_status.clone()
            else {
                return;
            };
            let (sender, receiver) = mpsc::channel();

            thread::spawn(move || {
                let result = firmware_dfu::transfer_firmware_image(
                    &device.path,
                    device.recipient,
                    &package.image_data,
                    |progress| {
                        let _ = sender.send(FirmwareFlashUpdate::Progress(progress));
                    },
                );

                let update = match result {
                    Ok(()) => FirmwareFlashUpdate::Succeeded,
                    Err(error) => FirmwareFlashUpdate::Failed(error),
                };
                let _ = sender.send(update);
            });

            self.firmware_flash_status = FirmwareFlashStatus::Flashing {
                progress: 0.0,
                started_at: Instant::now(),
                receiver,
            };
        }
    }

    pub fn selected_device_label(&self) -> Option<String> {
        self.selected_device().map(format_discovered_device_label)
    }

    pub fn can_refresh_devices(&self) -> bool {
        !self.is_refreshing() && !self.is_connected() && !self.is_connecting()
    }

    pub fn can_change_selected_device(&self) -> bool {
        !self.is_refreshing() && !self.is_connected() && !self.is_connecting()
    }

    pub fn can_press_connection_button(&self) -> bool {
        self.selected_device.is_some()
            && !self.is_connecting()
            && !self.is_refreshing()
            && !self.is_firmware_flashing()
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

    pub fn firmware_source(&self) -> FirmwareSource {
        self.firmware_source
    }

    pub fn local_firmware_package_status(&self) -> &LocalFirmwarePackageStatus {
        &self.local_firmware_package_status
    }

    pub fn network_firmware_repository_status(&self) -> &NetworkFirmwareRepositoryStatus {
        &self.network_firmware_repository_status
    }

    pub fn network_firmware_package_status(&self) -> &NetworkFirmwarePackageStatus {
        &self.network_firmware_package_status
    }

    pub fn selected_network_firmware(&self) -> Option<usize> {
        self.selected_network_firmware
    }

    pub fn network_firmware_options(&self) -> Vec<firmware_repository::FirmwareRepositoryFirmware> {
        let Some(device) = self.selected_device() else {
            return Vec::new();
        };
        let bootloader = self
            .loaded_device_information
            .as_ref()
            .and_then(|info| info.details.as_ref())
            .and_then(|details| details.bootloader_variant.as_deref());

        let NetworkFirmwareRepositoryStatus::Loaded(manifest) =
            &self.network_firmware_repository_status
        else {
            return Vec::new();
        };

        manifest
            .devices
            .iter()
            .filter(|entry| entry.board_name == device.board_name)
            .filter(|entry| bootloader.is_none_or(|bootloader| entry.bootloader == bootloader))
            .flat_map(|entry| entry.firmwares.iter().cloned())
            .collect()
    }

    pub fn firmware_flash_status(&self) -> &FirmwareFlashStatus {
        &self.firmware_flash_status
    }

    pub fn can_flash_firmware(&self) -> bool {
        matches!(
            &self.local_firmware_package_status,
            LocalFirmwarePackageStatus::Valid(_)
        ) && matches!(&self.firmware_flash_status, FirmwareFlashStatus::Idle)
    }

    pub fn is_firmware_flashing(&self) -> bool {
        matches!(
            &self.firmware_flash_status,
            FirmwareFlashStatus::Flashing { .. }
        )
    }

    pub fn is_network_firmware_repository_loading(&self) -> bool {
        matches!(
            self.network_firmware_repository_status,
            NetworkFirmwareRepositoryStatus::Loading { .. }
        )
    }

    pub fn is_network_firmware_package_downloading(&self) -> bool {
        matches!(
            self.network_firmware_package_status,
            NetworkFirmwarePackageStatus::Downloading { .. }
        )
    }

    pub fn select_network_firmware(
        &mut self,
        index: usize,
        firmware: firmware_repository::FirmwareRepositoryFirmware,
    ) {
        if self.is_firmware_flashing() {
            return;
        }

        self.selected_network_firmware = Some(index);
        self.local_firmware_package_status = LocalFirmwarePackageStatus::NotSelected;
        self.firmware_flash_status = FirmwareFlashStatus::Idle;

        let Some(device) = self.selected_device().cloned() else {
            self.network_firmware_package_status =
                NetworkFirmwarePackageStatus::Failed("No device selected".to_owned());
            return;
        };
        let manifest_url = firmware_repository::DEFAULT_MANIFEST_URL.to_owned();
        let cache_dir = firmware_repository::firmware_cache_dir();

        let hid = BlueismHid::new().ok();
        let device_information = device_information::read_device_information(hid.as_ref(), &device);
        match firmware_repository::cached_package_path(&firmware, &cache_dir) {
            Ok(Some(path)) => {
                self.local_firmware_package_status =
                    match firmware_package::analyze_local_firmware_package(
                        &path,
                        &device_information,
                    ) {
                        Ok(package) => LocalFirmwarePackageStatus::Valid(package),
                        Err(checks) => LocalFirmwarePackageStatus::Invalid(checks),
                    };
                self.network_firmware_package_status = NetworkFirmwarePackageStatus::Idle;
                return;
            }
            Ok(None) => {}
            Err(error) => {
                self.local_firmware_package_status = LocalFirmwarePackageStatus::Invalid(vec![
                    crate::application::data::FirmwarePackageCheck {
                        name: "Cached firmware".to_owned(),
                        value: error,
                        passed: false,
                    },
                ]);
                self.network_firmware_package_status = NetworkFirmwarePackageStatus::Idle;
                return;
            }
        }

        let (sender, receiver) = mpsc::channel();

        thread::spawn(move || {
            let hid = BlueismHid::new().ok();
            let device_information =
                device_information::read_device_information(hid.as_ref(), &device);
            let result = match firmware_repository::download_package_with_progress(
                &manifest_url,
                &firmware,
                &cache_dir,
                Duration::from_secs(10),
                |progress| {
                    let _ = sender.send(NetworkFirmwarePackageUpdate::Progress(progress));
                },
            ) {
                Ok(path) => {
                    firmware_package::analyze_local_firmware_package(&path, &device_information)
                }
                Err(error) => Err(vec![crate::application::data::FirmwarePackageCheck {
                    name: "Download firmware".to_owned(),
                    value: error,
                    passed: false,
                }]),
            };
            let update = match result {
                Ok(package) => NetworkFirmwarePackageUpdate::Succeeded(package),
                Err(checks) => NetworkFirmwarePackageUpdate::Failed(checks),
            };
            let _ = sender.send(update);
        });

        self.network_firmware_package_status = NetworkFirmwarePackageStatus::Downloading {
            progress: 0.0,
            started_at: Instant::now(),
            receiver,
        };
    }

    pub fn is_refreshing(&self) -> bool {
        matches!(self.refresh_status, DeviceRefreshStatus::Refreshing { .. })
    }

    pub fn is_connecting(&self) -> bool {
        matches!(
            self.connection_status,
            DeviceConnectionStatus::Connecting { .. }
        )
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
            } else if matches!(
                self.tab_loading_status,
                TabContentLoadingStatus::Loading {
                    tab: DeviceTab::Firmware,
                    ..
                }
            ) && self.firmware_source == FirmwareSource::Network
            {
                self.selected_network_firmware = None;
                self.local_firmware_package_status = LocalFirmwarePackageStatus::NotSelected;
                self.network_firmware_package_status = NetworkFirmwarePackageStatus::Idle;
                self.start_network_firmware_repository_fetch();
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
            self.start_pending_reconnection_if_possible();
        }
    }

    fn start_network_firmware_repository_fetch(&mut self) {
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let result = firmware_repository::fetch_manifest(
                firmware_repository::DEFAULT_MANIFEST_URL,
                Duration::from_secs(10),
            );
            let _ = sender.send(result);
        });

        self.network_firmware_repository_status = NetworkFirmwareRepositoryStatus::Loading {
            started_at: Instant::now(),
            receiver,
        };
    }

    fn apply_network_firmware_repository_result_if_ready(&mut self) {
        let result = match &self.network_firmware_repository_status {
            NetworkFirmwareRepositoryStatus::Loading {
                started_at,
                receiver,
            } => {
                if started_at.elapsed() >= Duration::from_secs(10) {
                    Some(Err("Network request timed out".to_owned()))
                } else {
                    match receiver.try_recv() {
                        Ok(result) => Some(result),
                        Err(TryRecvError::Empty) => None,
                        Err(TryRecvError::Disconnected) => {
                            Some(Err("Network request failed".to_owned()))
                        }
                    }
                }
            }
            NetworkFirmwareRepositoryStatus::Idle
            | NetworkFirmwareRepositoryStatus::Loaded(_)
            | NetworkFirmwareRepositoryStatus::Failed(_) => None,
        };

        if let Some(result) = result {
            self.network_firmware_repository_status = match result {
                Ok(manifest) => NetworkFirmwareRepositoryStatus::Loaded(manifest),
                Err(error) => NetworkFirmwareRepositoryStatus::Failed(error),
            };
        }
    }

    fn apply_network_firmware_package_result_if_ready(&mut self) {
        let mut completed = None;

        if let NetworkFirmwarePackageStatus::Downloading {
            progress, receiver, ..
        } = &mut self.network_firmware_package_status
        {
            loop {
                match receiver.try_recv() {
                    Ok(NetworkFirmwarePackageUpdate::Progress(next_progress)) => {
                        *progress = next_progress.clamp(0.0, 1.0);
                    }
                    Ok(NetworkFirmwarePackageUpdate::Succeeded(package)) => {
                        completed = Some(Ok(package));
                        break;
                    }
                    Ok(NetworkFirmwarePackageUpdate::Failed(checks)) => {
                        completed = Some(Err(checks));
                        break;
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        completed =
                            Some(Err(vec![crate::application::data::FirmwarePackageCheck {
                                name: "Download firmware".to_owned(),
                                value: "Firmware download failed".to_owned(),
                                passed: false,
                            }]));
                        break;
                    }
                }
            }
        }

        if let Some(result) = completed {
            self.network_firmware_package_status = NetworkFirmwarePackageStatus::Idle;
            self.local_firmware_package_status = match result {
                Ok(package) => LocalFirmwarePackageStatus::Valid(package),
                Err(checks) => LocalFirmwarePackageStatus::Invalid(checks),
            };
        }
    }

    fn load_selected_device_information(&mut self) {
        self.loaded_device_information = self
            .selected_device()
            .map(|device| device_information::read_device_information(self.hid.as_ref(), device));
    }

    fn apply_firmware_flash_update_if_ready(&mut self) {
        let mut next_status = None;

        if let FirmwareFlashStatus::Flashing {
            progress, receiver, ..
        } = &mut self.firmware_flash_status
        {
            while let Ok(update) = receiver.try_recv() {
                match update {
                    FirmwareFlashUpdate::Progress(next_progress) => {
                        *progress = next_progress.clamp(0.0, 1.0);
                    }
                    FirmwareFlashUpdate::Succeeded => {
                        next_status = Some(FirmwareFlashStatus::Succeeded);
                    }
                    FirmwareFlashUpdate::Failed(error) => {
                        next_status = Some(FirmwareFlashStatus::Failed(error));
                    }
                }
            }
        }

        if let Some(status) = next_status {
            self.firmware_flash_status = status;
            if matches!(self.firmware_flash_status, FirmwareFlashStatus::Succeeded) {
                self.start_reconnection_after_firmware_reboot();
            }
        }
    }

    fn start_reconnection_after_firmware_reboot(&mut self) {
        let Some(hwid) = self.selected_device().map(|device| device.hwid.clone()) else {
            return;
        };

        self.reconnect_after_refresh_hwid = Some(hwid);
        self.connection_status = DeviceConnectionStatus::Disconnected;
        self.tab_loading_status = TabContentLoadingStatus::Idle;
        self.loaded_device_information = None;
        self.active_tab = DeviceTab::Info;
        self.start_device_refresh_with_minimum(Duration::from_secs(3));
    }

    fn start_pending_reconnection_if_possible(&mut self) {
        let Some(hwid) = self.reconnect_after_refresh_hwid.clone() else {
            return;
        };

        let Some(index) = self.devices.iter().position(|device| device.hwid == hwid) else {
            self.start_device_refresh_with_minimum(Duration::from_secs(2));
            return;
        };

        self.reconnect_after_refresh_hwid = None;
        self.selected_device = Some(index);
        self.start_connection_for_hwid(hwid, Duration::from_millis(500));
    }

    fn start_connection_for_hwid(&mut self, requested_hwid: String, delay: Duration) {
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let devices = device_discovery::scan_devices();
            let _ = sender.send(devices);
        });

        self.connection_status = DeviceConnectionStatus::Connecting {
            complete_at: Instant::now() + delay,
            requested_hwid,
            receiver,
        };
    }

    fn reset_firmware_tab_state(&mut self) {
        self.firmware_source = FirmwareSource::Network;
        self.local_firmware_package_status = LocalFirmwarePackageStatus::NotSelected;
        self.network_firmware_package_status = NetworkFirmwarePackageStatus::Idle;
        self.selected_network_firmware = None;
        self.firmware_flash_status = FirmwareFlashStatus::Idle;
    }
}

pub fn format_discovered_device_label(device: &DiscoveredHidDevice) -> String {
    format!("{} (HW ID: {})", device.board_name, device.hwid)
}
