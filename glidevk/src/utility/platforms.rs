use ash::khr::surface;
use ash::vk;

#[cfg(not(target_os = "windows"))]
use ash::ext::debug_utils;

#[cfg(target_os = "windows")]
use ash::khr::win32_surface;

#[cfg(all(unix, not(target_os = "android"), not(target_os = "macos")))]
use ash::khr::xlib_surface;

#[cfg(target_os = "macos")]
use ash::ext::metal_surface;

#[cfg(target_os = "macos")]
use ash::khr::portability_enumeration;

/// Get required instance extensions.

#[cfg(target_os = "windows")]
pub fn required_extension_names() -> Vec<*const i8> {
    // Note: debug_utils excluded on Windows as Wine may not support it
    // and validation is disabled for Windows builds anyway
    vec![surface::NAME.as_ptr(), win32_surface::NAME.as_ptr()]
}

#[cfg(all(unix, not(target_os = "android"), not(target_os = "macos")))]
pub fn required_extension_names() -> Vec<*const i8> {
    vec![
        surface::NAME.as_ptr(),
        xlib_surface::NAME.as_ptr(),
        debug_utils::NAME.as_ptr(),
    ]
}

#[cfg(target_os = "macos")]
pub fn required_extension_names() -> Vec<*const i8> {
    vec![
        surface::NAME.as_ptr(),
        metal_surface::NAME.as_ptr(),
        debug_utils::NAME.as_ptr(),
        portability_enumeration::NAME.as_ptr(),
    ]
}

/// Get instance creation flags (needed for macOS portability)
#[cfg(target_os = "macos")]
pub fn instance_create_flags() -> vk::InstanceCreateFlags {
    vk::InstanceCreateFlags::ENUMERATE_PORTABILITY_KHR
}

#[cfg(not(target_os = "macos"))]
pub fn instance_create_flags() -> vk::InstanceCreateFlags {
    vk::InstanceCreateFlags::empty()
}

/// Create a Vulkan surface for Windows using HWND
#[cfg(target_os = "windows")]
pub unsafe fn create_surface(
    entry: &ash::Entry,
    instance: &ash::Instance,
    hwnd: u32,
) -> Result<vk::SurfaceKHR, vk::Result> {
    use winapi::um::libloaderapi::GetModuleHandleW;

    let hinstance = GetModuleHandleW(std::ptr::null());

    let create_info = vk::Win32SurfaceCreateInfoKHR::default()
        .hinstance(hinstance as isize)
        .hwnd(hwnd as isize);

    let win32_surface_loader = win32_surface::Instance::new(entry, instance);
    win32_surface_loader.create_win32_surface(&create_info, None)
}

/// Apply GlideVK's windowed-mode configuration to an existing Win32 window.
///
/// Games pass their own HWND to grSstWinOpen, so winit cannot manage its style
/// or dimensions. This updates the native style directly and returns the
/// resulting client-area size.
#[cfg(target_os = "windows")]
pub unsafe fn configure_native_window(
    hwnd: u32,
    client_width: u32,
    client_height: u32,
    resizable: bool,
    centered: bool,
) -> Option<(u32, u32)> {
    use std::ptr::null_mut;
    use winapi::shared::windef::{HWND, RECT};
    use winapi::um::winuser::{
        AdjustWindowRectEx, GetWindowLongW, GetWindowRect, SetWindowLongW, SetWindowPos,
        GWL_EXSTYLE, GWL_STYLE, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOZORDER, WS_MAXIMIZEBOX,
        WS_THICKFRAME,
    };

    let raw_hwnd = hwnd;
    let hwnd = raw_hwnd as usize as HWND;
    if hwnd.is_null() {
        return None;
    }

    let mut style = GetWindowLongW(hwnd, GWL_STYLE) as u32;
    if resizable {
        style |= WS_THICKFRAME | WS_MAXIMIZEBOX;
    } else {
        style &= !(WS_THICKFRAME | WS_MAXIMIZEBOX);
    }
    SetWindowLongW(hwnd, GWL_STYLE, style as i32);

    let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
    let mut outer_rect = RECT {
        left: 0,
        top: 0,
        right: client_width as i32,
        bottom: client_height as i32,
    };
    if AdjustWindowRectEx(&mut outer_rect, style, 0, ex_style) == 0 {
        return native_window_client_size(raw_hwnd);
    }

    let outer_width = outer_rect.right - outer_rect.left;
    let outer_height = outer_rect.bottom - outer_rect.top;

    let mut current_rect: RECT = std::mem::zeroed();
    GetWindowRect(hwnd, &mut current_rect);
    let mut x = current_rect.left;
    let mut y = current_rect.top;

    if centered {
        if let Some(monitor_info) = native_monitor_info(raw_hwnd) {
            x = monitor_info.rcWork.left
                + (monitor_info.rcWork.right - monitor_info.rcWork.left - outer_width) / 2;
            y = monitor_info.rcWork.top
                + (monitor_info.rcWork.bottom - monitor_info.rcWork.top - outer_height) / 2;
        }
    }

    SetWindowPos(
        hwnd,
        null_mut(),
        x,
        y,
        outer_width,
        outer_height,
        SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
    );

    native_window_client_size(raw_hwnd)
}

/// Return the current Win32 client-area dimensions for a game-owned HWND.
#[cfg(target_os = "windows")]
pub unsafe fn native_window_client_size(hwnd: u32) -> Option<(u32, u32)> {
    use winapi::shared::windef::{HWND, RECT};
    use winapi::um::winuser::GetClientRect;

    let hwnd = hwnd as usize as HWND;
    let mut rect: RECT = std::mem::zeroed();
    if hwnd.is_null() || GetClientRect(hwnd, &mut rect) == 0 {
        return None;
    }

    let width = (rect.right - rect.left).max(0) as u32;
    let height = (rect.bottom - rect.top).max(0) as u32;
    Some((width, height))
}

#[cfg(target_os = "windows")]
pub unsafe fn native_monitor_info(hwnd: u32) -> Option<winapi::um::winuser::MONITORINFO> {
    use winapi::um::winuser::{
        GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    };
    let monitor = MonitorFromWindow(hwnd as usize as _, MONITOR_DEFAULTTONEAREST);
    let mut info: MONITORINFO = std::mem::zeroed();
    info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
    if monitor.is_null() || GetMonitorInfoW(monitor, &mut info) == 0 {
        None
    } else {
        Some(info)
    }
}

/// Client rect of `hwnd` in screen coordinates as [left, top, right, bottom].
#[cfg(target_os = "windows")]
unsafe fn client_rect_on_screen(hwnd: winapi::shared::windef::HWND) -> Option<[i32; 4]> {
    use winapi::shared::windef::{POINT, RECT};
    use winapi::um::winuser::{ClientToScreen, GetClientRect};

    let mut rect: RECT = std::mem::zeroed();
    if GetClientRect(hwnd, &mut rect) == 0 || rect.right <= rect.left || rect.bottom <= rect.top {
        return None;
    }
    let mut corners = [
        POINT {
            x: rect.left,
            y: rect.top,
        },
        POINT {
            x: rect.right,
            y: rect.bottom,
        },
    ];
    for corner in &mut corners {
        if ClientToScreen(hwnd, corner) == 0 {
            return None;
        }
    }
    Some([corners[0].x, corners[0].y, corners[1].x, corners[1].y])
}

#[cfg(target_os = "windows")]
unsafe fn clip_cursor(hwnd: winapi::shared::windef::HWND) {
    use winapi::shared::windef::RECT;
    use winapi::um::winuser::{ClipCursor, GetForegroundWindow, IsIconic, IsWindowVisible};

    if hwnd.is_null()
        || GetForegroundWindow() != hwnd
        || IsIconic(hwnd) != 0
        || IsWindowVisible(hwnd) == 0
    {
        return;
    }

    match client_rect_on_screen(hwnd) {
        Some([left, top, right, bottom]) => {
            let rect = RECT {
                left,
                top,
                right,
                bottom,
            };
            ClipCursor(&rect);
        }
        None => {
            ClipCursor(std::ptr::null());
        }
    }
}

#[cfg(target_os = "windows")]
unsafe fn unclip_cursor() {
    winapi::um::winuser::ClipCursor(std::ptr::null());
}

#[cfg(target_os = "windows")]
unsafe extern "system" fn cursor_clip_subclass_proc(
    hwnd: winapi::shared::windef::HWND,
    message: winapi::shared::minwindef::UINT,
    wparam: winapi::shared::minwindef::WPARAM,
    lparam: winapi::shared::minwindef::LPARAM,
    _subclass_id: winapi::shared::basetsd::UINT_PTR,
    _ref_data: winapi::shared::basetsd::DWORD_PTR,
) -> winapi::shared::minwindef::LRESULT {
    use winapi::um::commctrl::{DefSubclassProc, RemoveWindowSubclass};
    use winapi::um::winuser::{
        SIZE_MINIMIZED, WA_INACTIVE, WM_ACTIVATE, WM_ACTIVATEAPP, WM_CLOSE, WM_DESTROY,
        WM_KILLFOCUS, WM_MOVE, WM_NCDESTROY, WM_SETFOCUS, WM_SIZE,
    };

    match message {
        WM_ACTIVATEAPP if wparam == 0 => unclip_cursor(),
        WM_ACTIVATE if (wparam & 0xffff) == WA_INACTIVE as usize => unclip_cursor(),
        WM_KILLFOCUS | WM_CLOSE | WM_DESTROY | WM_NCDESTROY => unclip_cursor(),
        WM_SIZE if wparam == SIZE_MINIMIZED => unclip_cursor(),
        WM_ACTIVATEAPP | WM_ACTIVATE | WM_SETFOCUS | WM_MOVE | WM_SIZE => clip_cursor(hwnd),
        _ => {}
    }

    if message == WM_NCDESTROY {
        RemoveWindowSubclass(
            hwnd,
            Some(cursor_clip_subclass_proc),
            CURSOR_CLIP_SUBCLASS_ID,
        );
    }

    DefSubclassProc(hwnd, message, wparam, lparam)
}

#[cfg(target_os = "windows")]
const CURSOR_CLIP_SUBCLASS_ID: usize = 0x474c_564b;

/// Confine the cursor to this game-owned window while it is active.
#[cfg(target_os = "windows")]
pub unsafe fn install_cursor_clip(hwnd: u32) -> bool {
    use winapi::shared::windef::HWND;
    use winapi::um::commctrl::SetWindowSubclass;

    let hwnd = hwnd as usize as HWND;
    if hwnd.is_null()
        || SetWindowSubclass(
            hwnd,
            Some(cursor_clip_subclass_proc),
            CURSOR_CLIP_SUBCLASS_ID,
            0,
        ) == 0
    {
        return false;
    }

    clip_cursor(hwnd);
    true
}

/// Remove cursor confinement and the game window subclass.
#[cfg(target_os = "windows")]
pub unsafe fn remove_cursor_clip(hwnd: u32) {
    use winapi::shared::windef::HWND;
    use winapi::um::commctrl::RemoveWindowSubclass;

    let hwnd = hwnd as usize as HWND;
    if !hwnd.is_null() {
        RemoveWindowSubclass(
            hwnd,
            Some(cursor_clip_subclass_proc),
            CURSOR_CLIP_SUBCLASS_ID,
        );
    }
    unclip_cursor();
}
