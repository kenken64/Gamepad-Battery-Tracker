use windows::Devices::Bluetooth::GenericAttributeProfile::{
    GattCharacteristicUuids, GattCommunicationStatus, GattServiceUuids,
};
use windows::Devices::Bluetooth::{BluetoothConnectionStatus, BluetoothLEDevice};
use windows::Devices::Enumeration::DeviceInformation;
use windows::Storage::Streams::DataReader;

pub struct BluetoothBattery {
    pub name: String,
    pub percent: u8,
    pub address: u64,
}

pub fn query() -> windows::core::Result<Vec<BluetoothBattery>> {
    let selector = BluetoothLEDevice::GetDeviceSelector()?;
    let devices = DeviceInformation::FindAllAsyncAqsFilter(&selector)?.join()?;
    let mut batteries = Vec::new();

    for info in devices {
        let Ok(name) = info.Name() else {
            continue;
        };
        let name = name.to_string();
        if !is_gamepad_name(&name) {
            continue;
        }

        let Ok(device) = BluetoothLEDevice::FromIdAsync(&info.Id()?).and_then(|op| op.join())
        else {
            continue;
        };
        if device.ConnectionStatus() != Ok(BluetoothConnectionStatus::Connected) {
            continue;
        }

        if let Ok(Some(percent)) = read_battery(&device) {
            if let Ok(address) = device.BluetoothAddress() {
                batteries.push(BluetoothBattery {
                    name,
                    percent,
                    address,
                });
            }
        }
    }

    Ok(batteries)
}

fn is_gamepad_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    [
        "controller",
        "gamepad",
        "dualshock",
        "dualsense",
        "8bitdo",
        "game sir",
        "gamesir",
    ]
    .iter()
    .any(|word| lower.contains(word))
}

fn read_battery(device: &BluetoothLEDevice) -> windows::core::Result<Option<u8>> {
    let services = device
        .GetGattServicesForUuidAsync(GattServiceUuids::Battery()?)?
        .join()?;
    if services.Status()? != GattCommunicationStatus::Success {
        return Ok(None);
    }

    for service in services.Services()? {
        let characteristics = service
            .GetCharacteristicsForUuidAsync(GattCharacteristicUuids::BatteryLevel()?)?
            .join()?;
        if characteristics.Status()? != GattCommunicationStatus::Success {
            continue;
        }
        for characteristic in characteristics.Characteristics()? {
            let result = characteristic.ReadValueAsync()?.join()?;
            if result.Status()? != GattCommunicationStatus::Success {
                continue;
            }
            let buffer = result.Value()?;
            if buffer.Length()? > 0 {
                return Ok(Some(DataReader::FromBuffer(&buffer)?.ReadByte()?.min(100)));
            }
        }
    }

    Ok(None)
}
