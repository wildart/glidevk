use ash::{
    self,
    vk::{self},
    Entry, Instance,
};

use crate::utility::*;

use super::{debug::ValidationInfo, structures::DeviceExtension};

pub const DEVICE_EXTENSIONS: DeviceExtension = DeviceExtension {
    names: ["VK_KHR_swapchain"],
};

pub struct VulkanInstance {
    pub entry: Entry,
    pub instance: Instance,

    validation_info: ValidationInfo,

    debug_utils_loader: ash::ext::debug_utils::Instance,
    debug_messenger: vk::DebugUtilsMessengerEXT,

    pub surface_loader: ash::khr::surface::Instance,
}

impl VulkanInstance {
    pub fn new(validate: bool) -> Self {
        let validation_info = ValidationInfo {
            is_enable: validate,
            required_validation_layers: ["VK_LAYER_KHRONOS_validation"],
        };

        let entry = unsafe { Entry::load() }.expect("Failed to load Vulkan library");

        let instance = create_instance(&entry, &validation_info);

        let (debug_utils_loader, debug_messenger) =
            debug::setup_debug_utils(validate, &entry, &instance);

        let _physical_devices = unsafe {
            instance
                .enumerate_physical_devices()
                .expect("Failed to enumerate Physical Devices!")
        };

        let surface_loader = ash::khr::surface::Instance::new(&entry, &instance);

        Self {
            entry,
            instance,
            validation_info,
            debug_utils_loader,
            debug_messenger,
            surface_loader,
        }
    }
}

impl From<bool> for VulkanInstance {
    fn from(validate: bool) -> Self {
        Self::new(validate)
    }
}

impl From<()> for VulkanInstance {
    fn from(_: ()) -> Self {
        Self::new(false)
    }
}

impl Drop for VulkanInstance {
    fn drop(&mut self) {
        unsafe {
            if self.validation_info.is_enable {
                self.debug_utils_loader
                    .destroy_debug_utils_messenger(self.debug_messenger, None);
            }
            self.instance.destroy_instance(None);
        }
    }
}

pub fn find_queue_family(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
    surface_loader: &ash::khr::surface::Instance,
    surface: &vk::SurfaceKHR,
) -> QueueFamilyIndices {
    let queue_families =
        unsafe { instance.get_physical_device_queue_family_properties(physical_device) };

    let mut queue_family_indices = QueueFamilyIndices::new();

    let mut index = 0;
    for queue_family in queue_families.iter() {
        if queue_family.queue_count > 0
            && queue_family.queue_flags.contains(vk::QueueFlags::GRAPHICS)
        {
            queue_family_indices.graphics_family = Some(index);
        }

        let is_present_support = unsafe {
            surface_loader.get_physical_device_surface_support(
                physical_device,
                index as u32,
                *surface,
            )
        };
        if queue_family.queue_count > 0 && is_present_support.unwrap() {
            queue_family_indices.present_family = Some(index);
        }

        if queue_family_indices.is_complete() {
            break;
        }

        index += 1;
    }

    queue_family_indices
}

pub fn query_swapchain_support(
    surface_loader: &ash::khr::surface::Instance,
    physical_device: vk::PhysicalDevice,
    &surface: &vk::SurfaceKHR,
) -> SwapchainSupportDetail {
    unsafe {
        let capabilities = surface_loader
            .get_physical_device_surface_capabilities(physical_device, surface)
            .expect("Failed to query for surface capabilities.");
        let formats = surface_loader
            .get_physical_device_surface_formats(physical_device, surface)
            .expect("Failed to query for surface formats.");
        let present_modes = surface_loader
            .get_physical_device_surface_present_modes(physical_device, surface)
            .expect("Failed to query for surface present mode.");

        SwapchainSupportDetail {
            capabilities,
            formats,
            present_modes,
        }
    }
}

fn is_physical_device_suitable(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
    surface_loader: &ash::khr::surface::Instance,
    surface: &vk::SurfaceKHR,
) -> bool {
    let device_properties = unsafe { instance.get_physical_device_properties(physical_device) };
    let device_features = unsafe { instance.get_physical_device_features(physical_device) };
    let device_queue_families =
        unsafe { instance.get_physical_device_queue_family_properties(physical_device) };

    let device_type = match device_properties.device_type {
        vk::PhysicalDeviceType::CPU => "Cpu",
        vk::PhysicalDeviceType::INTEGRATED_GPU => "Integrated GPU",
        vk::PhysicalDeviceType::DISCRETE_GPU => "Discrete GPU",
        vk::PhysicalDeviceType::VIRTUAL_GPU => "Virtual GPU",
        vk::PhysicalDeviceType::OTHER => "Unknown",
        _ => panic!(),
    };

    let device_name = tools::vk_to_string(&device_properties.device_name);
    log::trace!(
        "\tDevice Name: {}, id: {}, type: {}",
        device_name,
        device_properties.device_id,
        device_type
    );

    let major_version = vk::api_version_major(device_properties.api_version);
    let minor_version = vk::api_version_minor(device_properties.api_version);
    let patch_version = vk::api_version_patch(device_properties.api_version);

    log::trace!(
        "\tAPI Version: {}.{}.{}",
        major_version,
        minor_version,
        patch_version
    );

    log::trace!("\tSupport Queue Family: {}", device_queue_families.len());
    log::trace!("\t\tQueue Count | Graphics, Compute, Transfer, Sparse Binding");
    for queue_family in device_queue_families.iter() {
        let is_graphics_support = if queue_family.queue_flags.contains(vk::QueueFlags::GRAPHICS) {
            "support"
        } else {
            "unsupport"
        };
        let is_compute_support = if queue_family.queue_flags.contains(vk::QueueFlags::COMPUTE) {
            "support"
        } else {
            "unsupport"
        };
        let is_transfer_support = if queue_family.queue_flags.contains(vk::QueueFlags::TRANSFER) {
            "support"
        } else {
            "unsupport"
        };
        let is_sparse_support = if queue_family
            .queue_flags
            .contains(vk::QueueFlags::SPARSE_BINDING)
        {
            "support"
        } else {
            "unsupport"
        };

        log::trace!(
            "\t\t{}\t    | {},  {},  {},  {}",
            queue_family.queue_count,
            is_graphics_support,
            is_compute_support,
            is_transfer_support,
            is_sparse_support
        );
    }

    // there are plenty of features
    log::trace!(
        "\tGeometry Shader support: {}",
        if device_features.geometry_shader == 1 {
            "Support"
        } else {
            "Unsupport"
        }
    );

    let indices = find_queue_family(instance, physical_device, surface_loader, surface);

    let is_queue_family_supported = indices.is_complete();
    let is_device_extension_supported =
        check_device_extension_support(instance, physical_device, &DEVICE_EXTENSIONS);
    let is_swapchain_supported = if is_device_extension_supported {
        let swapchain_support = query_swapchain_support(surface_loader, physical_device, surface);
        !swapchain_support.formats.is_empty() && !swapchain_support.present_modes.is_empty()
    } else {
        false
    };
    // let is_support_sampler_anisotropy = device_features.sampler_anisotropy == 1;

    return is_queue_family_supported && is_device_extension_supported && is_swapchain_supported;
    // && is_support_sampler_anisotropy;
}

pub fn pick_physical_device(
    instance: &Instance,
    surface_loader: &ash::khr::surface::Instance,
    surface: &vk::SurfaceKHR,
) -> vk::PhysicalDevice {
    let physical_devices = unsafe {
        instance
            .enumerate_physical_devices()
            .expect("Failed to enumerate Physical Devices!")
    };

    // Score devices: prefer discrete GPUs over integrated, integrated over CPU
    let mut best_device: Option<(vk::PhysicalDevice, i32)> = None;

    for physical_device in physical_devices.iter() {
        if !is_physical_device_suitable(instance, *physical_device, surface_loader, surface) {
            continue;
        }

        let device_properties =
            unsafe { instance.get_physical_device_properties(*physical_device) };
        let device_name = tools::vk_to_string(&device_properties.device_name);

        // Score based on device type
        let score = match device_properties.device_type {
            vk::PhysicalDeviceType::DISCRETE_GPU => 100,
            vk::PhysicalDeviceType::INTEGRATED_GPU => 50,
            vk::PhysicalDeviceType::VIRTUAL_GPU => 25,
            vk::PhysicalDeviceType::CPU => 10,
            _ => 1,
        };

        log::trace!(
            "Device '{}' score: {} (type: {:?})",
            device_name,
            score,
            device_properties.device_type
        );

        if best_device.is_none() || score > best_device.unwrap().1 {
            best_device = Some((*physical_device, score));
        }
    }

    match best_device {
        Some((device, _)) => {
            let device_properties = unsafe { instance.get_physical_device_properties(device) };
            let device_name = tools::vk_to_string(&device_properties.device_name);
            log::debug!("Using GPU: {}", device_name);
            device
        }
        None => panic!("Failed to find a suitable GPU!"),
    }
}

pub fn create_logical_device(
    instance: &Instance,
    surface_loader: &ash::khr::surface::Instance,
    surface: &vk::SurfaceKHR,
    physical_device: vk::PhysicalDevice,
) -> (ash::Device, QueueFamilyIndices) {
    let indices = find_queue_family(&instance, physical_device, &surface_loader, surface);

    use std::collections::HashSet;
    let mut unique_queue_families = HashSet::new();
    unique_queue_families.insert(indices.graphics_family.unwrap());
    unique_queue_families.insert(indices.present_family.unwrap());

    let queue_priorities = [1.0_f32];
    let mut queue_create_infos = vec![];
    for &queue_family in unique_queue_families.iter() {
        let queue_create_info = vk::DeviceQueueCreateInfo::default()
            .flags(vk::DeviceQueueCreateFlags::empty())
            .queue_priorities(&queue_priorities)
            .queue_family_index(queue_family);
        queue_create_infos.push(queue_create_info);
    }

    let physical_device_features = vk::PhysicalDeviceFeatures {
        ..Default::default() // default just enable no feature.
    };

    let extensions_names = DEVICE_EXTENSIONS.get_extensions_raw_names();

    let device_create_info = vk::DeviceCreateInfo::default()
        .queue_create_infos(&queue_create_infos)
        .enabled_features(&physical_device_features)
        .enabled_extension_names(&extensions_names);

    let device: ash::Device = unsafe {
        instance
            .create_device(physical_device, &device_create_info, None)
            .expect("Failed to create logical Device!")
    };

    (device, indices)
}

pub fn create_command_pool(
    device: &ash::Device,
    queue_families: &QueueFamilyIndices,
) -> vk::CommandPool {
    let command_pool_create_info = vk::CommandPoolCreateInfo::default()
        .queue_family_index(queue_families.graphics_family.unwrap())
        .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);

    unsafe {
        device
            .create_command_pool(&command_pool_create_info, None)
            .expect("Failed to create Command Pool!")
    }
}

pub fn create_command_buffers(
    device: &ash::Device,
    command_pool: vk::CommandPool,
    _graphics_pipeline: vk::Pipeline,
    framebuffers: &Vec<vk::Framebuffer>,
    _render_pass: vk::RenderPass,
    _surface_extent: vk::Extent2D,
) -> Vec<vk::CommandBuffer> {
    let command_buffer_allocate_info = vk::CommandBufferAllocateInfo::default()
        .command_pool(command_pool)
        .level(vk::CommandBufferLevel::PRIMARY)
        .command_buffer_count(framebuffers.len() as u32);

    let command_buffers = unsafe {
        device
            .allocate_command_buffers(&command_buffer_allocate_info)
            .expect("Failed to allocate Command Buffers!")
    };

    command_buffers
}

pub struct VulkanApp {
    pub device: ash::Device, // Logical Device

    pub graphics_queue: vk::Queue,
    pub present_queue: vk::Queue,

    pub render_pass: vk::RenderPass,
    pub pipeline_layout: vk::PipelineLayout,
    pub graphics_pipelines: std::collections::HashMap<super::vertex::BlendMode, vk::Pipeline>,

    pub swapchain_loader: ash::khr::swapchain::Device,
    pub swapchain: vk::SwapchainKHR,
    pub swapchain_format: vk::Format,
    pub swapchain_extent: vk::Extent2D,
    pub swapchain_images: Vec<vk::Image>,
    pub swapchain_imageviews: Vec<vk::ImageView>,
    pub swapchain_framebuffers: Vec<vk::Framebuffer>,
    pub command_pool: vk::CommandPool,
    pub command_buffers: Vec<vk::CommandBuffer>,

    // Synchronization objects (one per frame-in-flight)
    pub image_available_semaphores: Vec<vk::Semaphore>,
    pub render_finished_semaphores: Vec<vk::Semaphore>,
    pub in_flight_fences: Vec<vk::Fence>,

    // Current frame state
    pub current_frame: usize,
    pub current_image_index: u32,

    // Vertex buffer for dynamic geometry
    pub vertex_buffer: Option<super::vertex::VertexBuffer>,
    pub physical_device: vk::PhysicalDevice,

    // Depth buffer resources
    pub depth_image: vk::Image,
    pub depth_image_memory: vk::DeviceMemory,
    pub depth_image_view: vk::ImageView,
    pub depth_format: vk::Format,

    // Texture cache for Glide textures
    pub texture_cache: super::texture::TextureCache,
}

impl VulkanApp {
    pub fn get_device(&self) -> &ash::Device {
        &self.device
    }

    ///
    /// Remove swapchain frame buffers, image views, pipeline, pipeline layout, render pass
    pub fn cleanup_swapchain(&self) {
        unsafe {
            // self.device.free_command_buffers(self.command_pool, &self.command_buffers);
            for &framebuffer in self.swapchain_framebuffers.iter() {
                self.device.destroy_framebuffer(framebuffer, None);
            }

            for (_, pipeline) in &self.graphics_pipelines {
                self.device.destroy_pipeline(*pipeline, None);
            }
            self.device
                .destroy_pipeline_layout(self.pipeline_layout, None);
            self.device.destroy_render_pass(self.render_pass, None);

            // Destroy depth resources
            self.device.destroy_image_view(self.depth_image_view, None);
            self.device.destroy_image(self.depth_image, None);
            self.device.free_memory(self.depth_image_memory, None);

            for &image_view in self.swapchain_imageviews.iter() {
                self.device.destroy_image_view(image_view, None);
            }
            self.swapchain_loader
                .destroy_swapchain(self.swapchain, None);
        }
    }
}

// pub fn get_swapchain_info(
//     swapchain_support: SwapchainSupportDetail,
//     surface: &vk::SurfaceKHR,
//     surface_format: vk::SurfaceFormatKHR,
//     present_mode: vk::PresentModeKHR,
//     queue_family: &QueueFamilyIndices,
//     width: u32,
//     height: u32,
// ) -> vk::SwapchainCreateInfoKHR<'_> {
//     let is_clipped = true;

//     let extent = choose_swapchain_extent_size(&swapchain_support.capabilities, width, height);

//     let image_count = swapchain_support.capabilities.min_image_count + 1;
//     let image_count = if swapchain_support.capabilities.max_image_count > 0 {
//         image_count.min(swapchain_support.capabilities.max_image_count)
//     } else {
//         image_count
//     };

//     let (image_sharing_mode, queue_family_indices) =
//         if queue_family.graphics_family != queue_family.present_family {
//             (
//                 vk::SharingMode::CONCURRENT,
//                 vec![
//                     queue_family.graphics_family.unwrap(),
//                     queue_family.present_family.unwrap(),
//                 ],
//             )
//         } else {
//             (vk::SharingMode::EXCLUSIVE, vec![])
//         };

//     vk::SwapchainCreateInfoKHR::default()
//         .surface(*surface)
//         .min_image_count(image_count)
//         .image_color_space(surface_format.color_space)
//         .image_format(surface_format.format)
//         .image_extent(extent)
//         .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
//         .image_sharing_mode(image_sharing_mode)
//         .queue_family_indices(&queue_family_indices)
//         .pre_transform(swapchain_support.capabilities.current_transform)
//         .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
//         .present_mode(present_mode)
//         .clipped(is_clipped)
//         .old_swapchain(vk::SwapchainKHR::null())
//         .image_array_layers(1);
// }

impl Drop for VulkanApp {
    fn drop(&mut self) {
        log::debug!("Dropping VulkanApp");
        unsafe {
            // Wait for device to be idle before cleanup
            self.device
                .device_wait_idle()
                .expect("Failed to wait for device idle");

            // Destroy sync objects
            for &sem in &self.image_available_semaphores {
                self.device.destroy_semaphore(sem, None);
            }
            for &sem in &self.render_finished_semaphores {
                self.device.destroy_semaphore(sem, None);
            }
            for &fence in &self.in_flight_fences {
                self.device.destroy_fence(fence, None);
            }

            // Destroy vertex buffer if present
            if let Some(ref vb) = self.vertex_buffer {
                vb.destroy(&self.device);
            }
            
            self.texture_cache.destroy(&self.device);

            self.cleanup_swapchain();

            self.device.destroy_command_pool(self.command_pool, None);
            self.device.destroy_device(None);
        }
        log::debug!("VulkanApp dropped");
    }
}
