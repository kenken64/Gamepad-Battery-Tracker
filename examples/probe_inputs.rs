use hidapi::HidApi;
use windows::Gaming::Input::{Gamepad, RawGameController};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let gamepads = Gamepad::Gamepads()?;
    println!("WGI gamepads: {}", gamepads.Size()?);
    for gamepad in gamepads {
        println!("  wireless: {:?}", gamepad.IsWireless());
        if let Ok(report) = gamepad.TryGetBatteryReport() {
            println!(
                "  battery: {:?} / {:?}",
                report.RemainingCapacityInMilliwattHours(),
                report.FullChargeCapacityInMilliwattHours()
            );
        }
    }

    let raw = RawGameController::RawGameControllers()?;
    println!("Raw gamepads: {}", raw.Size()?);
    for gamepad in raw {
        println!(
            "  {} {:04X}:{:04X} wireless {:?}",
            gamepad.DisplayName()?,
            gamepad.HardwareVendorId()?,
            gamepad.HardwareProductId()?,
            gamepad.IsWireless()
        );
    }

    let api = HidApi::new()?;
    for info in api.device_list().filter(|d| {
        d.vendor_id() == 0x2DC8
            || d.vendor_id() == 0x2836
            || (d.usage_page() == 1 && (d.usage() == 4 || d.usage() == 5))
    }) {
        println!(
            "HID {:04X}:{:04X} interface {} usage {:04X}:{:04X} {}",
            info.vendor_id(),
            info.product_id(),
            info.interface_number(),
            info.usage_page(),
            info.usage(),
            info.product_string().unwrap_or("")
        );
        println!("  path: {}", info.path().to_string_lossy());
        if let Ok(device) = info.open_device(&api) {
            if info.vendor_id() == 0x2836 {
                println!("  serial: {:?}", device.get_serial_number_string());
            }
            if (info.vendor_id() == 0x2DC8 && info.product_id() == 0x310B)
                || info.vendor_id() == 0x2836
            {
                let mut descriptor = [0u8; 4096];
                match device.get_report_descriptor(&mut descriptor) {
                    Ok(size) => println!("  descriptor {size}: {:02X?}", &descriptor[..size]),
                    Err(error) => println!("  descriptor error: {error}"),
                }
            }
            let mut report = [0u8; 64];
            match device.read_timeout(&mut report, 200) {
                Ok(size) => println!("  report {size}: {:02X?}", &report[..size]),
                Err(error) => println!("  read error: {error}"),
            }
        } else {
            println!("  could not open");
        }
    }
    Ok(())
}
