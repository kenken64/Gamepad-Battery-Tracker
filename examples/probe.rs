use windows::Devices::Bluetooth::BluetoothLEDevice;
use windows::Devices::Bluetooth::GenericAttributeProfile::{
    GattCharacteristicUuids, GattServiceUuids,
};
use windows::Devices::Enumeration::DeviceInformation;
use windows::Storage::Streams::DataReader;

fn main() -> windows::core::Result<()> {
    let selector = BluetoothLEDevice::GetDeviceSelector()?;
    let devices = DeviceInformation::FindAllAsyncAqsFilter(&selector)?.join()?;
    println!("BLE devices: {}", devices.Size()?);
    for info in devices {
        let name = info.Name()?.to_string();
        if !name.to_lowercase().contains("controller") {
            continue;
        }
        let device = BluetoothLEDevice::FromIdAsync(&info.Id()?)?.join()?;
        println!("{name}: {:?}", device.ConnectionStatus());
        let services = device
            .GetGattServicesForUuidAsync(GattServiceUuids::Battery()?)?
            .join()?;
        for service in services.Services()? {
            let chars = service
                .GetCharacteristicsForUuidAsync(GattCharacteristicUuids::BatteryLevel()?)?
                .join()?;
            for characteristic in chars.Characteristics()? {
                let result = characteristic.ReadValueAsync()?.join()?;
                if let Ok(buffer) = result.Value() {
                    println!("battery: {}%", DataReader::FromBuffer(&buffer)?.ReadByte()?);
                }
            }
        }
    }
    Ok(())
}
