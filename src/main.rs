#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod bluetooth;
mod hid;

use std::ffi::c_void;
use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};
use std::slice;
use std::sync::mpsc::{self, Receiver};
use std::sync::OnceLock;
use std::thread;
use std::time::Duration;

use windows_sys::Win32::Foundation::{COLORREF, HWND, LPARAM, POINT, RECT, SIZE, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, CreateFontW, DeleteDC, DeleteObject, DrawTextW, GetDC,
    ReleaseDC, SelectObject, SetBkMode, SetTextColor, AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION, CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, DEFAULT_QUALITY,
    DIB_RGB_COLORS, DT_END_ELLIPSIS, DT_LEFT, DT_SINGLELINE, DT_VCENTER, FW_SEMIBOLD, HGDIOBJ,
    OUT_DEFAULT_PRECIS, RGBQUAD, TRANSPARENT,
};
use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress, LoadLibraryW};
use windows_sys::Win32::UI::HiDpi::{
    SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture};
use windows_sys::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DestroyWindow,
    DispatchMessageW, GetCursorPos, GetMessageW, GetSystemMetrics, GetWindowLongPtrW,
    GetWindowRect, IsWindowVisible, LoadCursorW, LoadIconW, PostMessageW, PostQuitMessage,
    RegisterClassW, RegisterWindowMessageW, SetForegroundWindow, SetTimer, SetWindowLongPtrW,
    SetWindowPos, ShowWindow, TrackPopupMenu, TranslateMessage, UpdateLayeredWindow, CREATESTRUCTW,
    CS_HREDRAW, CS_VREDRAW, GWLP_USERDATA, HWND_TOPMOST, IDC_ARROW, IDI_APPLICATION, MF_STRING,
    MSG, SM_CXSCREEN, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW, SW_HIDE, SW_SHOW,
    TPM_RETURNCMD, TPM_RIGHTBUTTON, ULW_ALPHA, WM_APP, WM_CREATE, WM_DESTROY, WM_DISPLAYCHANGE,
    WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_NCCREATE, WM_NCDESTROY, WM_NULL, WM_RBUTTONUP,
    WM_TIMER, WNDCLASSW, WS_EX_LAYERED, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP, WS_VISIBLE,
};

const CLASS_NAME: &str = "GamepadBatteryTrackerOverlay";
const TIMER_ID: usize = 1;
const TRAY_ICON_ID: u32 = 1;
const WM_TRAY_ICON: u32 = WM_APP + 1;
const MENU_SHOW: usize = 1;
const MENU_HIDE: usize = 2;
const MENU_EXIT: usize = 3;
static TASKBAR_CREATED: OnceLock<u32> = OnceLock::new();
const POLL_MS: u32 = 1_000;
const OVERLAY_WIDTH: i32 = 380;
const BASE_HEIGHT: i32 = 94;
const ROW_HEIGHT: i32 = 48;
const HINT_HEIGHT: i32 = 20;
const XINPUT_HINT: &str = "Battery: power off, then hold B + Home";

const ERROR_SUCCESS: u32 = 0;
const BATTERY_DEVTYPE_GAMEPAD: u8 = 0x00;
const BATTERY_TYPE_DISCONNECTED: u8 = 0x00;
const BATTERY_TYPE_WIRED: u8 = 0x01;
const BATTERY_TYPE_ALKALINE: u8 = 0x02;
const BATTERY_TYPE_NIMH: u8 = 0x03;
const BATTERY_TYPE_UNKNOWN: u8 = 0xFF;

const BATTERY_LEVEL_EMPTY: u8 = 0x00;
const BATTERY_LEVEL_LOW: u8 = 0x01;
const BATTERY_LEVEL_MEDIUM: u8 = 0x02;
const BATTERY_LEVEL_FULL: u8 = 0x03;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct XInputBatteryInformation {
    battery_type: u8,
    battery_level: u8,
}

type XInputBatteryFn = unsafe extern "system" fn(u32, u8, *mut XInputBatteryInformation) -> u32;
static XINPUT_BATTERY: OnceLock<Option<XInputBatteryFn>> = OnceLock::new();
type XInputStateFn = unsafe extern "system" fn(u32, *mut XInputState) -> u32;
static XINPUT_STATE: OnceLock<Option<XInputStateFn>> = OnceLock::new();

#[repr(C)]
#[derive(Default)]
struct XInputState {
    packet_number: u32,
    buttons: u16,
    left_trigger: u8,
    right_trigger: u8,
    thumb_lx: i16,
    thumb_ly: i16,
    thumb_rx: i16,
    thumb_ry: i16,
}

fn xinput_battery_function() -> Option<XInputBatteryFn> {
    *XINPUT_BATTERY.get_or_init(|| unsafe {
        let module = LoadLibraryW(wide("xinput1_4.dll").as_ptr());
        if module.is_null() {
            return None;
        }
        let proc = GetProcAddress(module, b"XInputGetBatteryInformation\0".as_ptr())?;
        Some(std::mem::transmute::<
            unsafe extern "system" fn() -> isize,
            XInputBatteryFn,
        >(proc))
    })
}

fn xinput_state_function() -> Option<XInputStateFn> {
    *XINPUT_STATE.get_or_init(|| unsafe {
        let module = LoadLibraryW(wide("xinput1_4.dll").as_ptr());
        if module.is_null() {
            return None;
        }
        let proc = GetProcAddress(module, b"XInputGetState\0".as_ptr())?;
        Some(std::mem::transmute::<
            unsafe extern "system" fn() -> isize,
            XInputStateFn,
        >(proc))
    })
}

#[derive(Clone)]
struct ControllerBattery {
    label: String,
    status: String,
    percent: Option<u8>,
    hint: Option<&'static str>,
}

impl ControllerBattery {
    fn row_height(&self) -> i32 {
        ROW_HEIGHT + if self.hint.is_some() { HINT_HEIGHT } else { 0 }
    }
}

struct XInputController {
    battery: ControllerBattery,
    wired: bool,
}

struct AppState {
    tray_added: bool,
    dragging: bool,
    drag_start_cursor: POINT,
    drag_start_window: POINT,
    controllers: Vec<ControllerBattery>,
    bluetooth: Vec<bluetooth::BluetoothBattery>,
    bluetooth_rx: Receiver<Option<Vec<bluetooth::BluetoothBattery>>>,
    hid: Vec<hid::HidController>,
    hid_rx: Receiver<Option<Vec<hid::HidController>>>,
}

fn main() {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }

    if let Err(message) = run() {
        eprintln!("{message}");
    }
}

fn run() -> Result<(), String> {
    let (bluetooth_tx, bluetooth_rx) = mpsc::channel();
    thread::spawn(move || loop {
        if bluetooth_tx.send(bluetooth::query().ok()).is_err() {
            break;
        }
        thread::sleep(Duration::from_secs(5));
    });
    let (hid_tx, hid_rx) = mpsc::channel();
    thread::spawn(move || loop {
        if hid_tx.send(hid::query().ok()).is_err() {
            break;
        }
        thread::sleep(Duration::from_secs(5));
    });

    unsafe {
        TASKBAR_CREATED.get_or_init(|| RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()));
        let hinstance = GetModuleHandleW(null());
        if hinstance.is_null() {
            return Err("Failed to get module handle".to_string());
        }

        let class_name = wide(CLASS_NAME);
        let wc = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(window_proc),
            hInstance: hinstance,
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            lpszClassName: class_name.as_ptr(),
            ..zeroed()
        };

        if RegisterClassW(&wc) == 0 {
            return Err("Failed to register window class".to_string());
        }

        let state = Box::new(AppState {
            tray_added: false,
            dragging: false,
            drag_start_cursor: POINT { x: 0, y: 0 },
            drag_start_window: POINT { x: 0, y: 0 },
            controllers: Vec::new(),
            bluetooth: Vec::new(),
            bluetooth_rx,
            hid: Vec::new(),
            hid_rx,
        });
        let state_ptr = Box::into_raw(state);

        let hwnd = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            class_name.as_ptr(),
            wide("Gamepad Battery Tracker").as_ptr(),
            WS_POPUP | WS_VISIBLE,
            (GetSystemMetrics(SM_CXSCREEN) - OVERLAY_WIDTH - 24).max(0),
            24,
            OVERLAY_WIDTH,
            BASE_HEIGHT,
            null_mut(),
            null_mut(),
            hinstance,
            state_ptr.cast::<c_void>(),
        );

        if hwnd.is_null() {
            drop(Box::from_raw(state_ptr));
            return Err("Failed to create overlay window".to_string());
        }

        ShowWindow(hwnd, SW_SHOW);
        render_overlay(hwnd);

        let mut msg: MSG = zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    Ok(())
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> isize {
    match message {
        WM_NCCREATE => {
            let create = lparam as *const CREATESTRUCTW;
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, (*create).lpCreateParams as isize);
            1
        }
        WM_CREATE => {
            SetTimer(hwnd, TIMER_ID, POLL_MS, None);
            add_tray_icon(hwnd);
            0
        }
        WM_TRAY_ICON => {
            if wparam as u32 == TRAY_ICON_ID {
                match lparam as u32 {
                    WM_LBUTTONUP => show_overlay(hwnd),
                    WM_RBUTTONUP => show_tray_menu(hwnd),
                    _ => {}
                }
            }
            0
        }
        WM_TIMER => {
            if wparam == TIMER_ID {
                render_overlay(hwnd);
            }
            0
        }
        WM_DISPLAYCHANGE => {
            render_overlay(hwnd);
            0
        }
        WM_LBUTTONDOWN => {
            if let Some(state) = state_mut(hwnd) {
                state.dragging = true;
                GetCursorPos(&mut state.drag_start_cursor);

                let mut rect: RECT = zeroed();
                GetWindowRect(hwnd, &mut rect);
                state.drag_start_window = POINT {
                    x: rect.left,
                    y: rect.top,
                };

                SetCapture(hwnd);
            }
            0
        }
        WM_MOUSEMOVE => {
            if let Some(state) = state_mut(hwnd) {
                if state.dragging {
                    let mut cursor = POINT { x: 0, y: 0 };
                    GetCursorPos(&mut cursor);

                    let x = state.drag_start_window.x + cursor.x - state.drag_start_cursor.x;
                    let y = state.drag_start_window.y + cursor.y - state.drag_start_cursor.y;
                    SetWindowPos(hwnd, HWND_TOPMOST, x, y, 0, 0, SWP_NOSIZE | SWP_NOACTIVATE);
                }
            }
            0
        }
        WM_LBUTTONUP => {
            if let Some(state) = state_mut(hwnd) {
                state.dragging = false;
                ReleaseCapture();
            }
            0
        }
        WM_RBUTTONUP => {
            if state_mut(hwnd).is_some_and(|state| state.tray_added) {
                ShowWindow(hwnd, SW_HIDE);
            } else {
                DestroyWindow(hwnd);
            }
            0
        }
        WM_DESTROY => {
            remove_tray_icon(hwnd);
            PostQuitMessage(0);
            0
        }
        WM_NCDESTROY => {
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut AppState;
            if !ptr.is_null() {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                drop(Box::from_raw(ptr));
            }
            DefWindowProcW(hwnd, message, wparam, lparam)
        }
        _ if TASKBAR_CREATED.get().copied() == Some(message) => {
            add_tray_icon(hwnd);
            0
        }
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}

unsafe fn tray_icon_data(hwnd: HWND) -> NOTIFYICONDATAW {
    let mut data: NOTIFYICONDATAW = zeroed();
    data.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
    data.hWnd = hwnd;
    data.uID = TRAY_ICON_ID;
    data
}

unsafe fn add_tray_icon(hwnd: HWND) {
    let mut data = tray_icon_data(hwnd);
    data.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
    data.uCallbackMessage = WM_TRAY_ICON;
    data.hIcon = LoadIconW(null_mut(), IDI_APPLICATION);
    for (slot, ch) in data.szTip.iter_mut().zip(wide("Gamepad Battery Tracker")) {
        *slot = ch;
    }
    if let Some(state) = state_mut(hwnd) {
        state.tray_added = Shell_NotifyIconW(NIM_ADD, &data) != 0;
    }
}

unsafe fn remove_tray_icon(hwnd: HWND) {
    if state_mut(hwnd).is_some_and(|state| state.tray_added) {
        Shell_NotifyIconW(NIM_DELETE, &tray_icon_data(hwnd));
    }
}

unsafe fn show_overlay(hwnd: HWND) {
    ShowWindow(hwnd, SW_SHOW);
    SetWindowPos(
        hwnd,
        HWND_TOPMOST,
        0,
        0,
        0,
        0,
        SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
    );
    SetForegroundWindow(hwnd);
}

unsafe fn show_tray_menu(hwnd: HWND) {
    let menu = CreatePopupMenu();
    if menu.is_null() {
        return;
    }

    let visible = IsWindowVisible(hwnd) != 0;
    let toggle_id = if visible { MENU_HIDE } else { MENU_SHOW };
    let toggle_text = if visible {
        "Hide overlay"
    } else {
        "Show overlay"
    };
    AppendMenuW(menu, MF_STRING, toggle_id, wide(toggle_text).as_ptr());
    AppendMenuW(menu, MF_STRING, MENU_EXIT, wide("Exit").as_ptr());

    let mut cursor = POINT { x: 0, y: 0 };
    GetCursorPos(&mut cursor);
    SetForegroundWindow(hwnd);
    let chosen = TrackPopupMenu(
        menu,
        TPM_RETURNCMD | TPM_RIGHTBUTTON,
        cursor.x,
        cursor.y,
        0,
        hwnd,
        null(),
    ) as usize;
    PostMessageW(hwnd, WM_NULL, 0, 0);
    DestroyMenu(menu);

    match chosen {
        MENU_SHOW => show_overlay(hwnd),
        MENU_HIDE => {
            ShowWindow(hwnd, SW_HIDE);
        }
        MENU_EXIT => {
            DestroyWindow(hwnd);
        }
        _ => {}
    }
}

unsafe fn state_mut(hwnd: HWND) -> Option<&'static mut AppState> {
    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut AppState;
    ptr.as_mut()
}

unsafe fn render_overlay(hwnd: HWND) {
    let Some(state) = state_mut(hwnd) else {
        return;
    };

    while let Ok(batteries) = state.bluetooth_rx.try_recv() {
        if let Some(batteries) = batteries {
            state.bluetooth = batteries;
        }
    }
    while let Ok(devices) = state.hid_rx.try_recv() {
        if let Some(devices) = devices {
            state.hid = devices;
        }
    }
    state.controllers = merge_controllers(&state.bluetooth, &state.hid, query_controllers());
    let width = OVERLAY_WIDTH;
    let height = if state.controllers.is_empty() {
        BASE_HEIGHT
    } else {
        BASE_HEIGHT - ROW_HEIGHT
            + state
                .controllers
                .iter()
                .map(ControllerBattery::row_height)
                .sum::<i32>()
    };

    let mut rect: RECT = zeroed();
    GetWindowRect(hwnd, &mut rect);
    SetWindowPos(
        hwnd,
        HWND_TOPMOST,
        rect.left,
        rect.top,
        width,
        height,
        SWP_NOACTIVATE,
    );

    let screen_dc = GetDC(null_mut());
    if screen_dc.is_null() {
        return;
    }

    let memory_dc = CreateCompatibleDC(screen_dc);
    if memory_dc.is_null() {
        ReleaseDC(null_mut(), screen_dc);
        return;
    }

    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..zeroed()
        },
        bmiColors: [RGBQUAD {
            rgbBlue: 0,
            rgbGreen: 0,
            rgbRed: 0,
            rgbReserved: 0,
        }; 1],
    };

    let mut bits: *mut c_void = null_mut();
    let bitmap = CreateDIBSection(memory_dc, &bmi, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
    if bitmap.is_null() || bits.is_null() {
        DeleteDC(memory_dc);
        ReleaseDC(null_mut(), screen_dc);
        return;
    }

    let old_bitmap = SelectObject(memory_dc, bitmap as HGDIOBJ);
    let pixels = slice::from_raw_parts_mut(bits as *mut u32, (width * height) as usize);
    draw_overlay_pixels(pixels, width, height, &state.controllers);
    let text_bitmap = CreateDIBSection(memory_dc, &bmi, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
    if !text_bitmap.is_null() && !bits.is_null() {
        let old_text_bitmap = SelectObject(memory_dc, text_bitmap as HGDIOBJ);
        let text_pixels = slice::from_raw_parts_mut(bits as *mut u32, (width * height) as usize);
        text_pixels.fill(0);
        draw_overlay_text(memory_dc, width, &state.controllers);
        blend_text(pixels, text_pixels);
        SelectObject(memory_dc, old_text_bitmap);
        DeleteObject(text_bitmap as HGDIOBJ);
    }

    let destination = POINT {
        x: rect.left,
        y: rect.top,
    };
    let size = SIZE {
        cx: width,
        cy: height,
    };
    let source = POINT { x: 0, y: 0 };
    let blend = BLENDFUNCTION {
        BlendOp: AC_SRC_OVER as u8,
        BlendFlags: 0,
        SourceConstantAlpha: 255,
        AlphaFormat: AC_SRC_ALPHA as u8,
    };

    UpdateLayeredWindow(
        hwnd,
        screen_dc,
        &destination,
        &size,
        memory_dc,
        &source,
        0,
        &blend,
        ULW_ALPHA,
    );

    SelectObject(memory_dc, old_bitmap);
    DeleteObject(bitmap as HGDIOBJ);
    DeleteDC(memory_dc);
    ReleaseDC(null_mut(), screen_dc);
}

fn merge_controllers(
    bluetooth: &[bluetooth::BluetoothBattery],
    hid: &[hid::HidController],
    mut xinput: Vec<XInputController>,
) -> Vec<ControllerBattery> {
    let mut controllers: Vec<_> = bluetooth
        .iter()
        .map(|device| ControllerBattery {
            label: device.name.clone(),
            status: format!("{}%", device.percent),
            percent: Some(device.percent),
            hint: None,
        })
        .collect();

    for device in hid {
        let path = device.path.to_ascii_lowercase();
        if bluetooth
            .iter()
            .any(|bt| path.contains(&format!("{:012x}", bt.address)))
        {
            continue;
        }

        let is_8bitdo_receiver = device.vendor_id == 0x2DC8 && device.product_id == 0x310B;
        if !device.active && !is_8bitdo_receiver {
            continue;
        }
        let matching_xinput = if is_8bitdo_receiver {
            xinput.iter().position(|controller| controller.wired)
        } else if device.xinput_shadow && xinput.len() == 1 {
            Some(0)
        } else {
            None
        };

        if device.xinput_shadow && matching_xinput.is_none() && !is_8bitdo_receiver {
            continue;
        }

        let xinput_battery = matching_xinput.map(|index| xinput.remove(index).battery);
        let battery = if let Some(percent) = device.battery {
            ControllerBattery {
                label: device.name.clone(),
                status: if device.vendor_id == 0x2836 && device.product_id == 0x0001 {
                    format!("~{}% (HID estimate)", percent)
                } else if device.charging {
                    format!("{}% Charging", percent)
                } else {
                    format!("{}%", percent)
                },
                percent: Some(percent),
                hint: None,
            }
        } else if is_8bitdo_receiver {
            ControllerBattery {
                label: device.name.clone(),
                status: "Wired receiver (XInput)".to_string(),
                percent: None,
                hint: Some(XINPUT_HINT),
            }
        } else if let Some(mut xinput_battery) = xinput_battery {
            xinput_battery.label = device.name.clone();
            xinput_battery
        } else {
            ControllerBattery {
                label: device.name.clone(),
                status: "Battery unavailable".to_string(),
                percent: None,
                hint: None,
            }
        };
        controllers.push(battery);
    }

    // Windows also surfaces a Bluetooth-connected gamepad as an XInput slot, but that
    // slot's battery query typically returns nothing useful. Drop up to one such
    // uninformative XInput entry per Bluetooth device already reported above, so the
    // same physical controller isn't listed twice.
    let mut redundant_slots = bluetooth.len();
    for controller in xinput {
        let battery = controller.battery;
        let uninformative = battery.percent.is_none() && battery.status == "Battery unavailable";
        if uninformative && redundant_slots > 0 {
            redundant_slots -= 1;
            continue;
        }
        controllers.push(battery);
    }

    controllers
}

fn query_controllers() -> Vec<XInputController> {
    let mut controllers = Vec::new();
    let Some(get_battery) = xinput_battery_function() else {
        return controllers;
    };
    let Some(get_state) = xinput_state_function() else {
        return controllers;
    };

    for index in 0..4 {
        let mut state = XInputState::default();
        if unsafe { get_state(index, &mut state) } != ERROR_SUCCESS {
            continue;
        }
        let mut info = XInputBatteryInformation::default();
        let result = unsafe { get_battery(index, BATTERY_DEVTYPE_GAMEPAD, &mut info) };

        let battery_type = if result == ERROR_SUCCESS {
            info.battery_type
        } else {
            BATTERY_TYPE_UNKNOWN
        };
        let wired = battery_type == BATTERY_TYPE_WIRED;
        let (status, percent) = match battery_type {
            BATTERY_TYPE_DISCONNECTED => ("Battery unavailable".to_string(), None),
            BATTERY_TYPE_WIRED => ("Wired USB".to_string(), None),
            BATTERY_TYPE_ALKALINE | BATTERY_TYPE_NIMH => {
                let (level, approx) = match info.battery_level {
                    BATTERY_LEVEL_EMPTY => ("Empty", 5),
                    BATTERY_LEVEL_LOW => ("Low", 25),
                    BATTERY_LEVEL_MEDIUM => ("Medium", 60),
                    BATTERY_LEVEL_FULL => ("Full", 95),
                    _ => ("Unknown", 0),
                };
                (
                    format!("{} {} (approx.)", level, battery_kind(battery_type)),
                    if level == "Unknown" {
                        None
                    } else {
                        Some(approx)
                    },
                )
            }
            _ => ("Battery unavailable".to_string(), None),
        };

        controllers.push(XInputController {
            battery: ControllerBattery {
                label: format!("P{}", index + 1),
                status,
                percent,
                hint: None,
            },
            wired,
        });
    }

    controllers
}

fn battery_kind(kind: u8) -> &'static str {
    match kind {
        BATTERY_TYPE_WIRED => "USB",
        BATTERY_TYPE_ALKALINE => "Alkaline",
        BATTERY_TYPE_NIMH => "NiMH",
        BATTERY_TYPE_UNKNOWN => "Battery",
        _ => "Battery",
    }
}

fn draw_overlay_pixels(
    pixels: &mut [u32],
    width: i32,
    height: i32,
    controllers: &[ControllerBattery],
) {
    pixels.fill(0);

    draw_rounded_rect(
        pixels,
        width,
        height,
        0,
        0,
        width,
        height,
        7,
        rgba(16, 22, 27, 52),
    );

    let mut y = 42;
    for controller in controllers {
        if let Some(percent) = controller.percent {
            draw_battery_bar(pixels, width, height, 292, y + 23, 68, 18, percent);
        }
        y += controller.row_height();
    }
}

unsafe fn draw_overlay_text(memory_dc: *mut c_void, width: i32, controllers: &[ControllerBattery]) {
    SetBkMode(memory_dc, TRANSPARENT as i32);

    let font_name = wide("Segoe UI");
    let header_font = CreateFontW(
        -18,
        0,
        0,
        0,
        FW_SEMIBOLD as i32,
        0,
        0,
        0,
        DEFAULT_CHARSET.into(),
        OUT_DEFAULT_PRECIS.into(),
        CLIP_DEFAULT_PRECIS.into(),
        DEFAULT_QUALITY.into(),
        0,
        font_name.as_ptr(),
    );
    let body_font = CreateFontW(
        -16,
        0,
        0,
        0,
        FW_SEMIBOLD as i32,
        0,
        0,
        0,
        DEFAULT_CHARSET.into(),
        OUT_DEFAULT_PRECIS.into(),
        CLIP_DEFAULT_PRECIS.into(),
        DEFAULT_QUALITY.into(),
        0,
        font_name.as_ptr(),
    );

    let old_font = SelectObject(memory_dc, header_font as HGDIOBJ);
    SetTextColor(memory_dc, rgb(246, 248, 251));
    draw_text(memory_dc, "Gamepad Battery", 18, 13, width - 36, 26);

    SelectObject(memory_dc, body_font as HGDIOBJ);
    if controllers.is_empty() {
        SetTextColor(memory_dc, rgb(172, 179, 189));
        draw_text(memory_dc, "No gamepad connected", 18, 50, width - 46, 24);
    } else {
        let mut y = 42;
        for controller in controllers {
            SetTextColor(memory_dc, rgb(226, 232, 240));
            draw_text(memory_dc, &controller.label, 18, y, width - 36, 22);
            SetTextColor(memory_dc, rgb(174, 208, 216));
            draw_text(
                memory_dc,
                &controller.status,
                18,
                y + 22,
                if controller.percent.is_some() {
                    260
                } else {
                    width - 36
                },
                20,
            );
            if let Some(hint) = controller.hint {
                SetTextColor(memory_dc, rgb(172, 179, 189));
                draw_text(memory_dc, hint, 18, y + 42, width - 36, 20);
            }
            y += controller.row_height();
        }
    }

    SelectObject(memory_dc, old_font);
    DeleteObject(header_font as HGDIOBJ);
    DeleteObject(body_font as HGDIOBJ);
}

unsafe fn draw_text(dc: *mut c_void, text: &str, x: i32, y: i32, width: i32, height: i32) {
    let text = wide(text);
    let mut rect = RECT {
        left: x,
        top: y,
        right: x + width,
        bottom: y + height,
    };
    DrawTextW(
        dc,
        text.as_ptr(),
        -1,
        &mut rect,
        DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS,
    );
}

fn blend_text(background: &mut [u32], text: &[u32]) {
    for (destination, source) in background.iter_mut().zip(text) {
        let blue = source & 0xff;
        let green = (source >> 8) & 0xff;
        let red = (source >> 16) & 0xff;
        let alpha = red.max(green).max(blue) * 255 / 246;
        if alpha == 0 {
            continue;
        }
        let inverse = 255 - alpha;
        let old = *destination;
        let out_blue = (blue + (old & 0xff) * inverse / 255).min(255);
        let out_green = (green + ((old >> 8) & 0xff) * inverse / 255).min(255);
        let out_red = (red + ((old >> 16) & 0xff) * inverse / 255).min(255);
        let out_alpha = (alpha + (old >> 24) * inverse / 255).min(255);
        *destination = out_blue | (out_green << 8) | (out_red << 16) | (out_alpha << 24);
    }
}

fn draw_battery_bar(
    pixels: &mut [u32],
    width: i32,
    height: i32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    percent: u8,
) {
    draw_rounded_rect(
        pixels,
        width,
        height,
        x,
        y,
        w,
        h,
        5,
        rgba(255, 255, 255, 26),
    );
    draw_rounded_border(
        pixels,
        width,
        height,
        x,
        y,
        w,
        h,
        5,
        rgba(255, 255, 255, 84),
    );
    draw_rounded_rect(
        pixels,
        width,
        height,
        x + w + 3,
        y + 5,
        4,
        h - 10,
        2,
        rgba(255, 255, 255, 84),
    );

    let fill_width = (((w - 6) * percent as i32) / 100).max(if percent > 0 { 3 } else { 0 });
    let fill = if percent <= 25 {
        rgba(255, 103, 103, 240)
    } else if percent <= 60 {
        rgba(255, 190, 92, 240)
    } else {
        rgba(101, 204, 255, 240)
    };

    if fill_width > 0 {
        draw_rounded_rect(
            pixels,
            width,
            height,
            x + 3,
            y + 3,
            fill_width,
            h - 6,
            3,
            fill,
        );
    }
}

fn draw_rounded_border(
    pixels: &mut [u32],
    width: i32,
    height: i32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    radius: i32,
    color: u32,
) {
    draw_rounded_rect(pixels, width, height, x, y, w, 1, radius, color);
    draw_rounded_rect(pixels, width, height, x, y + h - 1, w, 1, radius, color);
    draw_rounded_rect(pixels, width, height, x, y, 1, h, radius, color);
    draw_rounded_rect(pixels, width, height, x + w - 1, y, 1, h, radius, color);
}

fn draw_rounded_rect(
    pixels: &mut [u32],
    width: i32,
    height: i32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    radius: i32,
    color: u32,
) {
    for py in y.max(0)..(y + h).min(height) {
        for px in x.max(0)..(x + w).min(width) {
            if inside_rounded_rect(px - x, py - y, w, h, radius) {
                pixels[(py * width + px) as usize] = color;
            }
        }
    }
}

fn inside_rounded_rect(px: i32, py: i32, w: i32, h: i32, radius: i32) -> bool {
    if radius <= 0 {
        return true;
    }

    let rx = if px < radius {
        radius - px
    } else if px >= w - radius {
        px - (w - radius - 1)
    } else {
        0
    };
    let ry = if py < radius {
        radius - py
    } else if py >= h - radius {
        py - (h - radius - 1)
    } else {
        0
    };

    rx == 0 || ry == 0 || rx * rx + ry * ry <= radius * radius
}

fn rgb(red: u8, green: u8, blue: u8) -> COLORREF {
    red as u32 | ((green as u32) << 8) | ((blue as u32) << 16)
}

fn rgba(red: u8, green: u8, blue: u8, alpha: u8) -> u32 {
    let a = alpha as u32;
    let b = blue as u32 * a / 255;
    let g = green as u32 * a / 255;
    let r = red as u32 * a / 255;
    b | (g << 8) | (r << 16) | (a << 24)
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_bluetooth_and_8bitdo_as_separate_devices() {
        let bluetooth = [bluetooth::BluetoothBattery {
            name: "Xbox Wireless Controller".to_string(),
            percent: 86,
            address: 0x9CAA1B7BDACF,
        }];
        let hid = [hid::HidController {
            name: "8BitDo Ultimate 2 Wireless".to_string(),
            path: "HID#VID_2DC8&PID_310B&IG_00".to_string(),
            vendor_id: 0x2DC8,
            product_id: 0x310B,
            xinput_shadow: true,
            active: true,
            battery: None,
            charging: false,
        }];
        let xinput = vec![XInputController {
            battery: ControllerBattery {
                label: "P1".to_string(),
                status: "Wired USB".to_string(),
                percent: None,
                hint: None,
            },
            wired: true,
        }];

        let result = merge_controllers(&bluetooth, &hid, xinput);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].percent, Some(86));
        assert_eq!(result[1].label, "8BitDo Ultimate 2 Wireless");
        assert_eq!(result[1].status, "Wired receiver (XInput)");
        assert_eq!(result[1].hint, Some(XINPUT_HINT));
        assert_eq!(result[1].percent, None);
    }

    #[test]
    fn prefers_native_hid_battery_when_available() {
        let hid = [hid::HidController {
            name: "8BitDo Ultimate 2 Wireless".to_string(),
            path: "HID#VID_2DC8&PID_6012&MI_00".to_string(),
            vendor_id: 0x2DC8,
            product_id: 0x6012,
            xinput_shadow: false,
            active: true,
            battery: Some(82),
            charging: true,
        }];
        let result = merge_controllers(&[], &hid, vec![]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].percent, Some(82));
        assert_eq!(result[0].status, "82% Charging");
    }

    #[test]
    fn keeps_8bitdo_receiver_visible_without_input_reports() {
        let hid = [hid::HidController {
            name: "8BitDo Ultimate 2 Wireless".to_string(),
            path: "HID#VID_2DC8&PID_310B&IG_00".to_string(),
            vendor_id: 0x2DC8,
            product_id: 0x310B,
            xinput_shadow: true,
            active: false,
            battery: None,
            charging: false,
        }];
        let xinput = vec![XInputController {
            battery: ControllerBattery {
                label: "P1".to_string(),
                status: "Wired USB".to_string(),
                percent: None,
                hint: None,
            },
            wired: true,
        }];
        let result = merge_controllers(&[], &hid, xinput);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].status, "Wired receiver (XInput)");
        assert_eq!(result[0].hint, Some(XINPUT_HINT));
    }

    #[test]
    fn shows_receiver_even_when_xinput_has_no_slot() {
        let hid = [hid::HidController {
            name: "8BitDo Ultimate 2 Wireless".to_string(),
            path: "HID#VID_2DC8&PID_310B&IG_00".to_string(),
            vendor_id: 0x2DC8,
            product_id: 0x310B,
            xinput_shadow: true,
            active: false,
            battery: None,
            charging: false,
        }];
        let result = merge_controllers(&[], &hid, vec![]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].status, "Wired receiver (XInput)");
    }

    #[test]
    fn labels_ouya_battery_as_estimated() {
        let hid = [hid::HidController {
            name: "OUYA Game Controller".to_string(),
            path: "HID#VID_2836&PID_0001".to_string(),
            vendor_id: 0x2836,
            product_id: 0x0001,
            xinput_shadow: false,
            active: true,
            battery: Some(82),
            charging: false,
        }];
        let result = merge_controllers(&[], &hid, vec![]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].status, "~82% (HID estimate)");
        assert_eq!(result[0].percent, Some(82));
    }

    #[test]
    fn hides_xinput_duplicate_of_bluetooth_controller() {
        let bluetooth = [bluetooth::BluetoothBattery {
            name: "Xbox Wireless Controller".to_string(),
            percent: 85,
            address: 0x9CAA1B7BDACF,
        }];
        let xinput = vec![XInputController {
            battery: ControllerBattery {
                label: "P1".to_string(),
                status: "Battery unavailable".to_string(),
                percent: None,
                hint: None,
            },
            wired: false,
        }];

        let result = merge_controllers(&bluetooth, &[], xinput);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].label, "Xbox Wireless Controller");
        assert_eq!(result[0].percent, Some(85));
    }

    #[test]
    fn keeps_unrelated_xinput_controller_with_no_bluetooth_devices() {
        let xinput = vec![XInputController {
            battery: ControllerBattery {
                label: "P1".to_string(),
                status: "Battery unavailable".to_string(),
                percent: None,
                hint: None,
            },
            wired: false,
        }];

        let result = merge_controllers(&[], &[], xinput);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].label, "P1");
    }

    #[test]
    fn keeps_wired_xinput_controller_alongside_bluetooth_controller() {
        let bluetooth = [bluetooth::BluetoothBattery {
            name: "Xbox Wireless Controller".to_string(),
            percent: 85,
            address: 0x9CAA1B7BDACF,
        }];
        let xinput = vec![XInputController {
            battery: ControllerBattery {
                label: "P2".to_string(),
                status: "Wired USB".to_string(),
                percent: None,
                hint: None,
            },
            wired: true,
        }];

        let result = merge_controllers(&bluetooth, &[], xinput);
        assert_eq!(result.len(), 2);
        assert_eq!(result[1].label, "P2");
        assert_eq!(result[1].status, "Wired USB");
    }
}
