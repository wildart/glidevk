//! Host-visible device memory that survives Wine's WoW64 address-space leak.
//!
//! In a 32-bit process, Wine 11 (`dlls/win32u/vulkan.c`) backs every
//! host-visible `vkAllocateMemory` with a `VirtualAlloc` block imported through
//! `VK_EXT_external_memory_host`, but never records the block, so
//! `vkFreeMemory` leaks it. Texture-heavy 32-bit games run out of address space
//! after a few thousand texture uploads. Wine skips its own block when the application
//! supplies `VkImportMemoryHostPointerInfoEXT`, so under Wine we import our own
//! block and release it in [`HostMemory::free`]. Other buffers use ordinary
//! allocation; host-visible images on the affected path are rejected.
use ash::vk;
use std::collections::HashMap;
use std::sync::Mutex;

enum Mode {
    Ordinary,
    Import { alignment: u64 },
}

/// Host-visible memory mode and imported blocks owned by one logical device.
pub struct HostMemory {
    mode: Mode,
    blocks: Mutex<HashMap<u64, usize>>,
}

impl HostMemory {
    pub fn new(
        instance: &ash::Instance,
        physical_device: vk::PhysicalDevice,
        enabled: bool,
    ) -> Self {
        Self {
            mode: import_alignment(instance, physical_device, enabled)
                .map_or(Mode::Ordinary, |alignment| Mode::Import { alignment }),
            blocks: Mutex::new(HashMap::new()),
        }
    }

    pub fn requires_extension(&self) -> bool {
        matches!(self.mode, Mode::Import { .. })
    }

    pub fn validate_buffer_import(
        &self,
        instance: &ash::Instance,
        physical_device: vk::PhysicalDevice,
        usage: vk::BufferUsageFlags,
    ) {
        if !self.requires_extension() {
            return;
        }
        let handle = vk::ExternalMemoryHandleTypeFlags::HOST_ALLOCATION_EXT;
        let info = vk::PhysicalDeviceExternalBufferInfo::default()
            .usage(usage)
            .handle_type(handle);
        let mut properties = vk::ExternalBufferProperties::default();
        unsafe {
            instance.get_physical_device_external_buffer_properties(
                physical_device,
                &info,
                &mut properties,
            );
        }
        let external = properties.external_memory_properties;
        if !external
            .external_memory_features
            .contains(vk::ExternalMemoryFeatureFlags::IMPORTABLE)
            || !external.compatible_handle_types.contains(handle)
        {
            panic!("VK_EXT_external_memory_host does not support buffer usage {usage:?}");
        }
    }

    /// Allocate buffer memory matching `requirements` with the given `properties`.
    pub unsafe fn allocate_buffer(
        &self,
        device: &ash::Device,
        instance: &ash::Instance,
        physical_device: vk::PhysicalDevice,
        requirements: vk::MemoryRequirements,
        properties: vk::MemoryPropertyFlags,
    ) -> vk::DeviceMemory {
        let memory_properties = instance.get_physical_device_memory_properties(physical_device);
        let find_type = |bits: u32| {
            (0..memory_properties.memory_type_count).find(|&i| {
                bits & (1 << i) != 0
                    && memory_properties.memory_types[i as usize]
                        .property_flags
                        .contains(properties)
            })
        };
        let index =
            find_type(requirements.memory_type_bits).expect("Failed to find suitable memory type!");
        let selected = memory_properties.memory_types[index as usize].property_flags;
        if selected.contains(vk::MemoryPropertyFlags::HOST_VISIBLE) {
            if let Mode::Import { alignment } = self.mode {
                return import(
                    device,
                    instance,
                    &requirements,
                    &find_type,
                    alignment,
                    &self.blocks,
                )
                .unwrap_or_else(|error| panic!("Wine host-memory import failed: {error}"));
            }
        }
        allocate_ordinary(device, &requirements, index, "buffer")
    }

    /// Allocate image memory, rejecting host-visible image types on affected Wine.
    pub unsafe fn allocate_image(
        &self,
        device: &ash::Device,
        instance: &ash::Instance,
        physical_device: vk::PhysicalDevice,
        requirements: vk::MemoryRequirements,
        properties: vk::MemoryPropertyFlags,
    ) -> vk::DeviceMemory {
        let memory_properties = instance.get_physical_device_memory_properties(physical_device);
        let index = find_memory_type(
            &memory_properties,
            requirements.memory_type_bits,
            properties,
        );
        let selected = memory_properties.memory_types[index as usize].property_flags;
        if matches!(self.mode, Mode::Import { .. })
            && selected.contains(vk::MemoryPropertyFlags::HOST_VISIBLE)
        {
            panic!(
                "Wine host-memory workaround does not support host-visible image memory ({selected:?})"
            );
        }
        allocate_ordinary(device, &requirements, index, "image")
    }

    /// Free memory obtained from this shim, releasing any imported block.
    pub unsafe fn free(&self, device: &ash::Device, memory: vk::DeviceMemory) {
        use ash::vk::Handle;
        let block = self.blocks.lock().unwrap().remove(&memory.as_raw());
        device.free_memory(memory, None);
        if let Some(block) = block {
            release(block);
        }
    }
}

fn find_memory_type(
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    bits: u32,
    properties: vk::MemoryPropertyFlags,
) -> u32 {
    (0..memory_properties.memory_type_count)
        .find(|&i| {
            bits & (1 << i) != 0
                && memory_properties.memory_types[i as usize]
                    .property_flags
                    .contains(properties)
        })
        .expect("Failed to find suitable memory type!")
}

unsafe fn allocate_ordinary(
    device: &ash::Device,
    requirements: &vk::MemoryRequirements,
    index: u32,
    resource: &str,
) -> vk::DeviceMemory {
    device
        .allocate_memory(
            &vk::MemoryAllocateInfo::default()
                .allocation_size(requirements.size)
                .memory_type_index(index),
            None,
        )
        .unwrap_or_else(|error| panic!("Failed to allocate {resource} memory: {error:?}"))
}

#[cfg(all(target_os = "windows", target_pointer_width = "32"))]
fn import_alignment(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
    enabled: bool,
) -> Option<u64> {
    use winapi::um::libloaderapi::{GetModuleHandleA, GetProcAddress};
    if !enabled {
        return None;
    }
    let under_wine = unsafe {
        let ntdll = GetModuleHandleA(b"ntdll.dll\0".as_ptr().cast());
        !GetProcAddress(ntdll, b"wine_get_version\0".as_ptr().cast()).is_null()
    };
    if !under_wine {
        return None;
    }
    let supported = unsafe { instance.enumerate_device_extension_properties(physical_device) }
        .ok()?
        .iter()
        .any(|e| e.extension_name_as_c_str().ok() == Some(ash::ext::external_memory_host::NAME));
    if !supported {
        log::warn!("Wine without VK_EXT_external_memory_host: host-visible memory will leak");
        return None;
    }
    let mut host = vk::PhysicalDeviceExternalMemoryHostPropertiesEXT::default();
    let mut properties = vk::PhysicalDeviceProperties2::default().push_next(&mut host);
    unsafe { instance.get_physical_device_properties2(physical_device, &mut properties) };
    let align = host.min_imported_host_pointer_alignment.max(1);
    // Wine only marks buffers as externally backed on its 32-bit WoW64 path.
    // Importing on 64-bit Wine violates VUID-vkBindBufferMemory-memory-02985.
    log::info!("Wine detected: importing host-visible memory (alignment {align})");
    Some(align)
}

#[cfg(all(target_os = "windows", target_pointer_width = "32"))]
unsafe fn import(
    device: &ash::Device,
    instance: &ash::Instance,
    requirements: &vk::MemoryRequirements,
    find_type: &dyn Fn(u32) -> Option<u32>,
    align: u64,
    blocks: &Mutex<HashMap<u64, usize>>,
) -> Result<vk::DeviceMemory, String> {
    use ash::vk::Handle;
    use winapi::um::memoryapi::VirtualAlloc;
    use winapi::um::winnt::{MEM_COMMIT, MEM_RESERVE, PAGE_READWRITE};

    let size = (requirements.size + align - 1) & !(align - 1);
    let host: *mut std::ffi::c_void = VirtualAlloc(
        std::ptr::null_mut(),
        size as usize,
        MEM_COMMIT | MEM_RESERVE,
        PAGE_READWRITE,
    )
    .cast();
    if host.is_null() {
        return Err("VirtualAlloc failed".into());
    }
    let handle_type = vk::ExternalMemoryHandleTypeFlags::HOST_ALLOCATION_EXT;
    let loader = ash::ext::external_memory_host::Device::new(instance, device);
    let mut host_properties = vk::MemoryHostPointerPropertiesEXT::default();
    let result = (loader.fp().get_memory_host_pointer_properties_ext)(
        device.handle(),
        handle_type,
        host,
        &mut host_properties,
    );
    if result != vk::Result::SUCCESS {
        release(host as usize);
        return Err(format!(
            "vkGetMemoryHostPointerPropertiesEXT returned {result:?}"
        ));
    }
    let Some(index) = find_type(host_properties.memory_type_bits & requirements.memory_type_bits)
    else {
        release(host as usize);
        return Err("no compatible imported memory type".into());
    };
    let mut import_info = vk::ImportMemoryHostPointerInfoEXT::default()
        .handle_type(handle_type)
        .host_pointer(host);
    let info = vk::MemoryAllocateInfo::default()
        .allocation_size(size)
        .push_next(&mut import_info);
    match device.allocate_memory(&info.memory_type_index(index), None) {
        Ok(memory) => {
            blocks
                .lock()
                .unwrap()
                .insert(memory.as_raw(), host as usize);
            Ok(memory)
        }
        Err(error) => {
            release(host as usize);
            Err(format!("vkAllocateMemory returned {error:?}"))
        }
    }
}

#[cfg(all(target_os = "windows", target_pointer_width = "32"))]
fn release(block: usize) {
    use winapi::um::memoryapi::VirtualFree;
    use winapi::um::winnt::MEM_RELEASE;
    unsafe { VirtualFree(block as *mut _, 0, MEM_RELEASE) };
}

#[cfg(not(all(target_os = "windows", target_pointer_width = "32")))]
fn import_alignment(_: &ash::Instance, _: vk::PhysicalDevice, _: bool) -> Option<u64> {
    None
}

#[cfg(not(all(target_os = "windows", target_pointer_width = "32")))]
unsafe fn import(
    _: &ash::Device,
    _: &ash::Instance,
    _: &vk::MemoryRequirements,
    _: &dyn Fn(u32) -> Option<u32>,
    _: u64,
    _: &Mutex<HashMap<u64, usize>>,
) -> Result<vk::DeviceMemory, String> {
    unreachable!()
}

#[cfg(not(all(target_os = "windows", target_pointer_width = "32")))]
fn release(_: usize) {}
