import CoreBluetooth
import Foundation
import GameController
import IOKit.hid

struct ControllerRow: Equatable {
    let name: String
    let percent: Int?
    let charging: Bool

    var status: String {
        guard let percent else { return "Battery unavailable" }
        return charging ? "\(percent)% Charging" : "\(percent)%"
    }
}

final class ControllerScanner: NSObject, CBCentralManagerDelegate, CBPeripheralDelegate {
    var onChange: (([ControllerRow]) -> Void)?

    private let hidManager: IOHIDManager
    private var central: CBCentralManager!
    private var peripherals: [UUID: CBPeripheral] = [:]
    private var bleBattery: [UUID: Int] = [:]
    private let batteryService = CBUUID(string: "180F")
    private let batteryLevel = CBUUID(string: "2A19")

    override init() {
        hidManager = IOHIDManagerCreate(kCFAllocatorDefault, IOOptionBits(kIOHIDOptionsTypeNone))
        super.init()
        let matches: [[String: Int]] = [4, 5].map {
            [kIOHIDDeviceUsagePageKey: 1, kIOHIDDeviceUsageKey: $0]
        }
        IOHIDManagerSetDeviceMatchingMultiple(hidManager, matches as CFArray)
        IOHIDManagerScheduleWithRunLoop(hidManager, CFRunLoopGetMain(), CFRunLoopMode.defaultMode.rawValue)
        IOHIDManagerOpen(hidManager, IOOptionBits(kIOHIDOptionsTypeNone))
        central = CBCentralManager(delegate: self, queue: nil)
    }

    deinit {
        IOHIDManagerClose(hidManager, IOOptionBits(kIOHIDOptionsTypeNone))
        IOHIDManagerUnscheduleFromRunLoop(hidManager, CFRunLoopGetMain(), CFRunLoopMode.defaultMode.rawValue)
    }

    func refresh() {
        if central.state == .poweredOn {
            let connected = central.retrieveConnectedPeripherals(withServices: [batteryService])
            let identifiers = Set(connected.map(\.identifier))
            bleBattery = bleBattery.filter { identifiers.contains($0.key) }
            peripherals = peripherals.filter { identifiers.contains($0.key) }
            for peripheral in connected where isGamepadName(peripheral.name ?? "") {
                let alreadyTracked = peripherals[peripheral.identifier] != nil
                peripherals[peripheral.identifier] = peripheral
                peripheral.delegate = self
                if !alreadyTracked || peripheral.state == .disconnected {
                    // Retrieving a system connection does not attach this app to it.
                    central.connect(peripheral)
                } else if peripheral.state == .connected {
                    peripheral.discoverServices([batteryService])
                }
            }
        } else {
            bleBattery.removeAll()
            peripherals.removeAll()
        }
        onChange?(snapshot())
    }

    func snapshot() -> [ControllerRow] {
        let hidDevices = (IOHIDManagerCopyDevices(hidManager) as? Set<IOHIDDevice>) ?? []
        var rows: [ControllerRow] = []
        var hidNames: [String] = []
        var seenHID: Set<String> = []

        for device in hidDevices {
            let name = property(device, kIOHIDProductKey) as? String ?? "Gamepad"
            let vendor = (property(device, kIOHIDVendorIDKey) as? NSNumber)?.intValue ?? 0
            let product = (property(device, kIOHIDProductIDKey) as? NSNumber)?.intValue ?? 0
            let serial = property(device, kIOHIDSerialNumberKey) as? String ?? ""
            let location = (property(device, kIOHIDLocationIDKey) as? NSNumber)?.intValue ?? 0
            let key = "\(vendor):\(product):\(serial):\(location)"
            guard seenHID.insert(key).inserted else { continue }
            hidNames.append(name)
            rows.append(ControllerRow(name: name, percent: nil, charging: false))
        }

        // GameController is the preferred source of battery data. HID covers gamepads
        // that macOS recognizes as input devices but doesn't expose via GameController.
        for controller in GCController.controllers() {
            let name = controller.vendorName ?? "Gamepad"
            let level = controller.battery?.batteryLevel
            let percent = level.flatMap { value -> Int? in
                // GameController uses zero as its default even when battery state is unknown.
                if value == 0 && controller.battery?.batteryState == .unknown { return nil }
                return max(0, min(100, Int((value * 100).rounded())))
            }
            let charging = controller.battery?.batteryState == .charging
            let row = ControllerRow(name: name, percent: percent, charging: charging)
            if let index = hidNames.firstIndex(where: { namesMatch($0, name) }) {
                rows[index] = row
                hidNames[index] = ""
            } else {
                rows.append(row)
            }
        }

        for peripheral in peripherals.values {
            guard let name = peripheral.name, isGamepadName(name) else { continue }
            let row = ControllerRow(name: name, percent: bleBattery[peripheral.identifier], charging: false)
            if let index = rows.firstIndex(where: { namesMatch($0.name, name) }) {
                if row.percent != nil && rows[index].percent == nil { rows[index] = row }
            } else if row.percent != nil {
                rows.append(row)
            }
        }

        return rows.sorted { $0.name.localizedCaseInsensitiveCompare($1.name) == .orderedAscending }
    }

    private func property(_ device: IOHIDDevice, _ key: String) -> AnyObject? {
        IOHIDDeviceGetProperty(device, key as CFString)
    }

    private func namesMatch(_ left: String, _ right: String) -> Bool {
        let a = left.lowercased().replacingOccurrences(of: "controller", with: "").trimmingCharacters(in: .whitespaces)
        let b = right.lowercased().replacingOccurrences(of: "controller", with: "").trimmingCharacters(in: .whitespaces)
        return a == b || (!a.isEmpty && !b.isEmpty && (a.contains(b) || b.contains(a)))
    }

    private func isGamepadName(_ name: String) -> Bool {
        let lower = name.lowercased()
        return ["controller", "gamepad", "dualshock", "dualsense", "8bitdo", "gamesir", "game sir"]
            .contains { lower.contains($0) }
    }

    func centralManagerDidUpdateState(_ central: CBCentralManager) {
        refresh()
    }

    func centralManager(_ central: CBCentralManager, didConnect peripheral: CBPeripheral) {
        peripheral.discoverServices([batteryService])
    }

    func centralManager(_ central: CBCentralManager, didDisconnectPeripheral peripheral: CBPeripheral, error: Error?) {
        bleBattery.removeValue(forKey: peripheral.identifier)
        onChange?(snapshot())
    }

    func centralManager(_ central: CBCentralManager, didFailToConnect peripheral: CBPeripheral, error: Error?) {
        bleBattery.removeValue(forKey: peripheral.identifier)
        onChange?(snapshot())
    }

    func peripheral(_ peripheral: CBPeripheral, didDiscoverServices error: Error?) {
        guard error == nil else { return }
        for service in peripheral.services ?? [] where service.uuid == batteryService {
            peripheral.discoverCharacteristics([batteryLevel], for: service)
        }
    }

    func peripheral(_ peripheral: CBPeripheral, didDiscoverCharacteristicsFor service: CBService, error: Error?) {
        guard error == nil else { return }
        for characteristic in service.characteristics ?? [] where characteristic.uuid == batteryLevel {
            peripheral.readValue(for: characteristic)
        }
    }

    func peripheral(_ peripheral: CBPeripheral, didUpdateValueFor characteristic: CBCharacteristic, error: Error?) {
        guard error == nil, characteristic.uuid == batteryLevel,
              let value = characteristic.value?.first, value <= 100 else { return }
        bleBattery[peripheral.identifier] = Int(value)
        onChange?(snapshot())
    }
}
