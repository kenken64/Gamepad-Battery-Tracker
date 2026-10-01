# Gamepad Battery Tracker

A desktop overlay for Windows and macOS. It has a translucent background, stays on top, detects connected gamepads, and can be dragged between monitors with the left mouse button.

## Run on Windows

```powershell
cargo run --release
```

Or double-click `Launch Gamepad Battery Tracker.cmd` in File Explorer. The launcher builds the app on first use.

## Run on macOS

Requires Xcode command-line tools or Xcode. From the project directory, run:

```sh
./macos/run.sh
```

This builds `target/macos/Gamepad Battery Tracker.app` and opens it. To print a one-time controller snapshot in Terminal instead, run `./macos/run.sh --once`.

## Controls

- Drag anywhere on the overlay to move it between monitors.
- Right-click the overlay to hide it. Use the tray icon on Windows or menu bar icon on macOS to show, hide, or quit.
- On Windows, the notification-area overflow (`^`) may contain the icon.

## Notes

- Bluetooth LE controllers expose exact percentages when they provide the standard GATT Battery Service.
- OUYA Game Controllers (Bluetooth Classic, `2836:0001`) expose a HID battery-strength report. The overlay scales its 0-255 value to an approximate percentage; actual remaining life of AA batteries can differ.
- 8BitDo Ultimate 2 Wireless controllers expose exact percentages over their native HID mode on compatible firmware. With the mode switch on 2.4G, turn the controller off, hold B, then press Home to turn it on. The app detects the new mode automatically. Native mode uses DInput, which may change how some games recognize the controller.
- XInput controllers report only Empty, Low, Medium, or Full when a battery report is available. A USB receiver can report itself as wired even while the controller is wireless; the app keeps the 8BitDo receiver visible as `Wired receiver (XInput)` and shows how to switch modes for battery percentage. This indicates the receiver is plugged in, not necessarily that the gamepad is powered on.
- Xbox controllers connected by USB may remain usable while XInput reports no battery data. The overlay shows `Battery unavailable` for these controllers; connect over Bluetooth to read a current battery level when the controller exposes the GATT Battery Service.
- Other HID gamepads are detected by name. When a device does not expose battery data through a supported interface, the overlay shows `Battery unavailable`.
- On macOS, the app reads battery levels exposed by Game Controller or the standard Bluetooth LE Battery Service. macOS HID gamepads without battery data are still listed as `Battery unavailable`. The Windows-specific XInput and custom HID battery reports are not available on macOS.
- The tested original 8Bitdo Zero GamePad (`05A0:3232`, firmware 1.0.8) did not expose a battery percentage to macOS. [8BitDo's ZERO manual](https://download.8bitdo.com/Manual/Controller/ZERO/ZERO_Manual_V4.pdf) describes a blinking red LED for low battery and a green LED for charging.
