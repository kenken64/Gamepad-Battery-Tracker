# Gamepad Battery Tracker

A Windows desktop overlay written in Rust. It has a translucent background, stays on top, detects connected gamepads, and can be dragged between monitors with the left mouse button.

## Run

```powershell
cargo run --release
```

Or double-click `Launch Gamepad Battery Tracker.cmd` in File Explorer. The launcher builds the app on first use.

## Controls

- Drag anywhere on the overlay to move it between monitors.
- Right-click the overlay to hide it in the taskbar notification area.
- Click the notification-area icon to show and bring the overlay to the front. Right-click the icon for Show/Hide and Exit.
- If the icon is not visible beside the clock, open the notification-area overflow with the `^` button.

## Notes

- Bluetooth LE controllers expose exact percentages when they provide the standard GATT Battery Service.
- OUYA Game Controllers (Bluetooth Classic, `2836:0001`) expose a HID battery-strength report. The overlay scales its 0-255 value to an approximate percentage; actual remaining life of AA batteries can differ.
- 8BitDo Ultimate 2 Wireless controllers expose exact percentages over their native HID mode on compatible firmware. With the mode switch on 2.4G, turn the controller off, hold B, then press Home to turn it on. The app detects the new mode automatically. Native mode uses DInput, which may change how some games recognize the controller.
- XInput controllers report only Empty, Low, Medium, or Full when a battery report is available. A USB receiver can report itself as wired even while the controller is wireless; the app keeps the 8BitDo receiver visible as `Wired receiver (XInput)` and shows how to switch modes for battery percentage. This indicates the receiver is plugged in, not necessarily that the gamepad is powered on.
- Xbox controllers connected by USB may remain usable while XInput reports no battery data. The overlay shows `Battery unavailable` for these controllers; connect over Bluetooth to read a current battery level when the controller exposes the GATT Battery Service.
- Other HID gamepads are detected by name. When a device does not expose battery data through a supported interface, the overlay shows `Battery unavailable`.
