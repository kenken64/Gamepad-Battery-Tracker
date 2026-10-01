#!/bin/sh
set -eu

project_dir=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
app_dir="$project_dir/target/macos/Gamepad Battery Tracker.app"
mkdir -p "$app_dir/Contents/MacOS"
cp "$project_dir/macos/Info.plist" "$app_dir/Contents/Info.plist"
swiftc -O -target "$(uname -m)-apple-macos11.0" \
    -framework AppKit -framework GameController -framework IOKit -framework CoreBluetooth \
    "$project_dir/macos/ControllerScanner.swift" \
    "$project_dir/macos/GamepadBatteryTracker.swift" \
    -o "$app_dir/Contents/MacOS/GamepadBatteryTracker"
echo "$app_dir"
