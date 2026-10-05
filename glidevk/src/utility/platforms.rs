use ash::khr::surface;
use ash::vk;

/// Shared by presentation and mouse input so letterboxing uses the same coordinates.
pub(crate) fn viewport(
    (game_width, game_height): (u32, u32),
    (window_width, window_height): (u32, u32),
    keep_aspect_ratio: bool,
) -> (u32, u32, u32, u32) {
    if !keep_aspect_ratio || (window_width == game_width && window_height == game_height) {
        return (0, 0, window_width, window_height);
    }
    let game_aspect = game_width as f32 / game_height as f32;
    let window_aspect = window_width as f32 / window_height as f32;
    if window_aspect > game_aspect {
        let width = (window_height as f32 * game_aspect) as u32;
        ((window_width - width) / 2, 0, width, window_height)
    } else {
        let height = (window_width as f32 / game_aspect) as u32;
        (0, (window_height - height) / 2, window_width, height)
    }
}

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

/// Client rect of `hwnd` in screen coordinates.
#[cfg(target_os = "windows")]
unsafe fn client_rect_on_screen(
    hwnd: winapi::shared::windef::HWND,
) -> Option<winapi::shared::windef::RECT> {
    use winapi::shared::windef::{POINT, RECT};
    use winapi::um::errhandlingapi::{GetLastError, SetLastError};
    use winapi::um::winuser::{GetClientRect, MapWindowPoints};

    let mut rect: RECT = std::mem::zeroed();
    if GetClientRect(hwnd, &mut rect) == 0 || rect.right <= rect.left || rect.bottom <= rect.top {
        return None;
    }

    SetLastError(0);
    if MapWindowPoints(
        hwnd,
        std::ptr::null_mut(),
        &mut rect as *mut RECT as *mut POINT,
        2,
    ) == 0
        && GetLastError() != 0
    {
        return None;
    }
    Some(rect)
}

#[cfg(target_os = "windows")]
unsafe fn clip_cursor(hwnd: winapi::shared::windef::HWND) {
    use winapi::um::winuser::{ClipCursor, GetForegroundWindow, IsIconic, IsWindowVisible};

    if hwnd.is_null()
        || GetForegroundWindow() != hwnd
        || IsIconic(hwnd) != 0
        || IsWindowVisible(hwnd) == 0
    {
        return;
    }

    match client_rect_on_screen(hwnd) {
        Some(rect) => {
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
#[derive(Clone, Copy)]
struct WindowInput {
    game_size: (u32, u32),
    keep_aspect_ratio: bool,
    clip_cursor: bool,
}

#[cfg(target_os = "windows")]
unsafe extern "system" fn window_input_subclass_proc(
    hwnd: winapi::shared::windef::HWND,
    message: winapi::shared::minwindef::UINT,
    wparam: winapi::shared::minwindef::WPARAM,
    mut lparam: winapi::shared::minwindef::LPARAM,
    _subclass_id: winapi::shared::basetsd::UINT_PTR,
    ref_data: winapi::shared::basetsd::DWORD_PTR,
) -> winapi::shared::minwindef::LRESULT {
    use winapi::um::commctrl::DefSubclassProc;
    use winapi::um::winuser::{
        SIZE_MINIMIZED, WA_INACTIVE, WM_ACTIVATE, WM_ACTIVATEAPP, WM_CLOSE, WM_DESTROY,
        WM_KILLFOCUS, WM_LBUTTONDBLCLK, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDBLCLK,
        WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEMOVE, WM_MOVE, WM_NCDESTROY, WM_RBUTTONDBLCLK,
        WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SETFOCUS, WM_SIZE, WM_XBUTTONDBLCLK, WM_XBUTTONDOWN,
        WM_XBUTTONUP,
    };

    // Copy before calling the game: its handler may close the Glide context and remove us.
    let input = *(ref_data as *const WindowInput);
    if message == WM_NCDESTROY {
        remove_window_input(hwnd as usize as u32);
        return DefSubclassProc(hwnd, message, wparam, lparam);
    }

    // Wheel messages use screen coordinates; only these messages carry client coordinates.
    if matches!(
        message,
        WM_MOUSEMOVE
            | WM_LBUTTONDOWN
            | WM_LBUTTONUP
            | WM_LBUTTONDBLCLK
            | WM_RBUTTONDOWN
            | WM_RBUTTONUP
            | WM_RBUTTONDBLCLK
            | WM_MBUTTONDOWN
            | WM_MBUTTONUP
            | WM_MBUTTONDBLCLK
            | WM_XBUTTONDOWN
            | WM_XBUTTONUP
            | WM_XBUTTONDBLCLK
    ) {
        if let Some(size) = native_window_client_size(hwnd as usize as u32) {
            let (left, top, width, height) =
                viewport(input.game_size, size, input.keep_aspect_ratio);
            if width != 0 && height != 0 {
                let x = lparam as u16 as i16 as i64;
                let y = (lparam >> 16) as u16 as i16 as i64;
                let x = ((x - left as i64) * input.game_size.0 as i64 / width as i64)
                    .clamp(0, input.game_size.0 as i64 - 1);
                let y = ((y - top as i64) * input.game_size.1 as i64 / height as i64)
                    .clamp(0, input.game_size.1 as i64 - 1);
                lparam = (x | (y << 16)) as _;
            }
        }
    }

    let result = DefSubclassProc(hwnd, message, wparam, lparam);
    if input.clip_cursor {
        // The game can change window geometry during its handler.
        match message {
            WM_ACTIVATEAPP if wparam == 0 => unclip_cursor(),
            WM_ACTIVATE if (wparam & 0xffff) == WA_INACTIVE as usize => unclip_cursor(),
            WM_KILLFOCUS | WM_CLOSE | WM_DESTROY => unclip_cursor(),
            WM_SIZE if wparam == SIZE_MINIMIZED => unclip_cursor(),
            WM_ACTIVATEAPP | WM_ACTIVATE | WM_SETFOCUS | WM_MOVE | WM_SIZE => clip_cursor(hwnd),
            _ => {}
        }
    }
    result
}

/// Ctrl+Shift+F12 is held while the game window has focus.
#[cfg(target_os = "windows")]
pub unsafe fn pause_hotkey_down(hwnd: u32) -> bool {
    use winapi::shared::windef::HWND;
    use winapi::um::winuser::{
        GetAncestor, GetAsyncKeyState, GetForegroundWindow, GA_ROOT, VK_CONTROL, VK_F12, VK_SHIFT,
    };

    let down = |key| GetAsyncKeyState(key) as u16 & 0x8000 != 0;
    let hwnd = hwnd as usize as HWND;
    // The game may hand Glide a child window; focus belongs to its top-level window.
    !hwnd.is_null()
        && GetForegroundWindow() == GetAncestor(hwnd, GA_ROOT)
        && down(VK_CONTROL)
        && down(VK_SHIFT)
        && down(VK_F12)
}

#[cfg(target_os = "windows")]
const WINDOW_INPUT_SUBCLASS_ID: usize = 0x474c_564b;
#[cfg(target_os = "windows")]
const WINDOW_INPUT_PROPERTY: &[u8] = b"glidevk.window_input\0";

/// Translate input to game coordinates and optionally confine it to the active client area.
#[cfg(target_os = "windows")]
pub unsafe fn install_window_input(
    hwnd: u32,
    game_size: (u32, u32),
    keep_aspect_ratio: bool,
    clip: bool,
) -> bool {
    use winapi::shared::windef::HWND;
    use winapi::um::commctrl::SetWindowSubclass;
    use winapi::um::winuser::{GetPropA, RemovePropA, SetPropA};

    remove_window_input(hwnd);
    let hwnd = hwnd as usize as HWND;
    let property = WINDOW_INPUT_PROPERTY.as_ptr().cast();
    if hwnd.is_null() || game_size.0 == 0 || game_size.1 == 0 || !GetPropA(hwnd, property).is_null()
    {
        return false;
    }
    let input = Box::into_raw(Box::new(WindowInput {
        game_size,
        keep_aspect_ratio,
        clip_cursor: clip,
    }));
    // Wine's comctl32 v5 lacks GetWindowSubclass; a window property owns the allocation.
    if SetPropA(hwnd, property, input.cast()) == 0 {
        drop(Box::from_raw(input));
        return false;
    }
    if SetWindowSubclass(
        hwnd,
        Some(window_input_subclass_proc),
        WINDOW_INPUT_SUBCLASS_ID,
        input as usize,
    ) == 0
    {
        RemovePropA(hwnd, property);
        drop(Box::from_raw(input));
        return false;
    }
    if clip {
        clip_cursor(hwnd);
    }
    true
}

/// Remove mouse translation and release any cursor confinement owned by this subclass.
#[cfg(target_os = "windows")]
pub unsafe fn remove_window_input(hwnd: u32) {
    use winapi::shared::windef::HWND;
    use winapi::um::commctrl::RemoveWindowSubclass;
    use winapi::um::winuser::{GetPropA, RemovePropA};

    let hwnd = hwnd as usize as HWND;
    let property = WINDOW_INPUT_PROPERTY.as_ptr().cast();
    let data = GetPropA(hwnd, property);
    if !data.is_null()
        && RemoveWindowSubclass(
            hwnd,
            Some(window_input_subclass_proc),
            WINDOW_INPUT_SUBCLASS_ID,
        ) != 0
    {
        RemovePropA(hwnd, property);
        let input = Box::from_raw(data as *mut WindowInput);
        if input.clip_cursor {
            unclip_cursor();
        }
    }
}
