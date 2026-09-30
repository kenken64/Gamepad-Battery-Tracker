use hidapi::{DeviceInfo, HidApi, HidResult};
use std::collections::HashMap;
use std::ptr::null;
use windows_sys::Win32::Devices::HumanInterfaceDevice::HidD_GetInputReport;
use windows_sys::Win32::Foundation::{CloseHandle, GENERIC_READ, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};

#[derive(Clone)]
pub struct HidController {
    pub name: String,
    pub path: String,
    pub vendor_id: u16,
    pub product_id: u16,
    pub xinput_shadow: bool,
    pub active: bool,
    pub battery: Option<u8>,
    pub charging: bool,
}

pub fn query() -> HidResult<Vec<HidController>> {
    let api = HidApi::new()?;
    let mut controllers = Vec::new();
    let mut ouya_batteries = HashMap::new();

    for info in api.device_list().filter(|info| {
        info.vendor_id() == 0x2836
            && info.product_id() == 0x0001
            && info.usage_page() == 0x0C
            && info.usage() == 0x01
    }) {
        if let (Some(serial), Some(percent)) = (info.serial_number(), read_ouya_battery(info)) {
            ouya_batteries.insert(serial.to_ascii_lowercase(), percent);
        }
    }

    for info in api.device_list() {
        if info.usage_page() != 0x01 || !matches!(info.usage(), 0x04 | 0x05) {
            continue;
        }

        let path = info.path().to_string_lossy().into_owned();
        let name = display_name(info.product_string().unwrap_or("Gamepad"));
        let (active, battery, charging) = match (info.vendor_id(), info.product_id()) {
            (0x2DC8, 0x310B) => (true, None, false),
            (0x2DC8, 0x6012 | 0x6013) => read_8bitdo_battery(&api, info),
            (0x2836, 0x0001) => (
                true,
                info.serial_number()
                    .and_then(|serial| ouya_batteries.get(&serial.to_ascii_lowercase()).copied()),
                false,
            ),
            _ => (true, None, false),
        };

        controllers.push(HidController {
            name,
            xinput_shadow: path.to_ascii_lowercase().contains("&ig_"),
            path,
            vendor_id: info.vendor_id(),
            product_id: info.product_id(),
            active,
            battery,
            charging,
        });
    }

    controllers.sort_by(|a, b| a.name.cmp(&b.name).then(a.path.cmp(&b.path)));
    Ok(controllers)
}

fn display_name(product: &str) -> String {
    let name = product
        .strip_prefix("Controller (")
        .and_then(|name| name.strip_suffix(')'))
        .unwrap_or(product);
    name.strip_suffix(" Controller for PC")
        .unwrap_or(name)
        .to_string()
}

fn read_ouya_battery(info: &DeviceInfo) -> Option<u8> {
    let path: Vec<u16> = info
        .path()
        .to_string_lossy()
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let handle = unsafe {
        CreateFileW(
            path.as_ptr(),
            GENERIC_READ,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            null(),
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return None;
    }

    let mut report = [0u8; 8];
    report[0] = 3;
    let success = unsafe {
        let success = HidD_GetInputReport(handle, report.as_mut_ptr().cast(), report.len() as u32);
        CloseHandle(handle);
        success != 0
    };
    if success {
        decode_ouya_battery(&report)
    } else {
        None
    }
}

fn decode_ouya_battery(report: &[u8]) -> Option<u8> {
    if report.len() != 8 || report[0] != 3 {
        return None;
    }
    Some(((report[1] as u16 * 100 + 127) / 255) as u8)
}

fn read_8bitdo_battery(api: &HidApi, info: &DeviceInfo) -> (bool, Option<u8>, bool) {
    let Ok(device) = info.open_device(api) else {
        return (false, None, false);
    };
    let mut report = [0u8; 64];
    let mut active = false;
    for _ in 0..3 {
        let Ok(size) = device.read_timeout(&mut report, 150) else {
            break;
        };
        active |= size > 0;
        if let Some(battery) = decode_8bitdo_report(&report[..size]) {
            return (true, Some(battery.0), battery.1);
        }
    }
    (active, None, false)
}

fn decode_8bitdo_report(report: &[u8]) -> Option<(u8, bool)> {
    if report.len() < 16 || report[0] != 0x01 {
        return None;
    }
    // Native report 0x01 stores charge in byte 14, with charging in its high bit.
    let raw = report[14];
    let percent = raw & 0x7f;
    if percent > 100 {
        return None;
    }
    Some((percent, raw & 0x80 != 0))
}

#[cfg(test)]
mod tests {
    use super::{decode_8bitdo_report, decode_ouya_battery};

    #[test]
    fn scales_ouya_hid_battery_strength() {
        assert_eq!(decode_ouya_battery(&[3, 0, 0, 0, 0, 0, 0, 0]), Some(0));
        assert_eq!(decode_ouya_battery(&[3, 0xD2, 0, 0, 0, 0, 0, 0]), Some(82));
        assert_eq!(decode_ouya_battery(&[3, 255, 0, 0, 0, 0, 0, 0]), Some(100));
        assert_eq!(decode_ouya_battery(&[7, 0xD2, 0, 0, 0, 0, 0, 0]), None);
    }

    #[test]
    fn decodes_native_battery_and_rejects_other_reports() {
        let mut report = [0u8; 34];
        report[0] = 1;
        report[14] = 82 | 0x80;
        report[15] = 1;
        assert_eq!(decode_8bitdo_report(&report), Some((82, true)));
        report[14] = 110;
        assert_eq!(decode_8bitdo_report(&report), None);
        report[14] = 0;
        assert_eq!(decode_8bitdo_report(&report), Some((0, false)));
        report[0] = 0;
        assert_eq!(decode_8bitdo_report(&report), None);

        let live_report = [
            0x01, 0x0F, 0x7F, 0x7F, 0x7F, 0x7F, 0, 0, 0, 0, 0, 0, 0, 0, 0x56, 0x6B,
        ];
        assert_eq!(decode_8bitdo_report(&live_report), Some((86, false)));
    }
}
