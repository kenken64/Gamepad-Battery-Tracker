@echo off
cd /d "%~dp0"
set "CARGO_TARGET_DIR=%~dp0.target"
if not exist ".target\release\gamepad-battery-tracker.exe" (
    cargo build --release
    if errorlevel 1 (
        pause
        exit /b 1
    )
)
start "" ".target\release\gamepad-battery-tracker.exe"
