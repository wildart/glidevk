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
