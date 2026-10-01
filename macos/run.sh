#!/bin/sh
set -eu

project_dir=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
app_dir=$(/bin/sh "$project_dir/macos/build.sh")
if [ "$#" -gt 0 ]; then
    "$app_dir/Contents/MacOS/GamepadBatteryTracker" "$@"
else
    open "$app_dir"
fi
