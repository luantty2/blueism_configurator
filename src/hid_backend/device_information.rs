use crate::application::data::{
    DeviceIdentityInformation, DeviceInformation, DeviceInformationDetails, DiscoveredHidDevice,
    FirmwareSummaryInformation,
};
use crate::hid_backend::{config_channel, device_discovery::BlueismHid};

pub fn read_device_information(
    hid: Option<&BlueismHid>,
    device: &DiscoveredHidDevice,
) -> DeviceInformation {
    let details = hid
        .and_then(|hid| hid.open(&device.path).ok())
        .and_then(|opened| config_channel::read_detailed_info(&opened, device.recipient).ok())
        .map(|details| DeviceInformationDetails {
            modules: details.modules,
            identity: details.identity.map(|identity| DeviceIdentityInformation {
                vendor_id: identity.vendor_id,
                product_id: identity.product_id,
                generation: identity.generation,
            }),
            firmware: details.firmware.map(|firmware| FirmwareSummaryInformation {
                flash_area_id: firmware.flash_area_id,
                image_len: firmware.image_len,
                version: firmware.version,
            }),
            bootloader_variant: details.bootloader_variant,
        });

    DeviceInformation {
        device: device.clone(),
        details,
    }
}
