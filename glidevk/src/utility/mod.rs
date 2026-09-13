pub mod debug;
pub mod framebuffer;
pub mod hostmem;
pub mod platforms;
pub mod structures;
pub mod texture;
pub mod tools;
pub mod vertex;
pub mod vulkan;
pub mod window;

use std::ffi::CString;

use crate::utility::structures::*;
use ash::vk;

pub fn create_instance(
    entry: &ash::Entry,
    validation_info: &debug::ValidationInfo,
) -> ash::Instance {
    if validation_info.is_enable
        && debug::check_validation_layer_support(
            entry,
            &validation_info.required_validation_layers.to_vec(),
        ) == false
    {
        panic!("Validation layers requested, but not available!");
    }

    let app_name = CString::new("GlideVK").unwrap();
    let engine_name = CString::new("No Engine").unwrap();
    let app_info = vk::ApplicationInfo::default()
        .application_name(app_name.as_c_str())
        .application_version(vk::make_api_version(0, 1, 0, 0))
        .engine_name(engine_name.as_c_str())
        .engine_version(vk::make_api_version(0, 1, 0, 0))
        .api_version(vk::make_api_version(0, 1, 2, 0));

    // This create info used to debug issues in vk::createInstance and vk::destroyInstance.
    // let mut debug_utils_create_info = debug::populate_debug_messenger_create_info();

    // VK_EXT debug utils has been requested here.
    let extension_names = platforms::required_extension_names();

    // Prepare validation layer names if enabled (must live as long as create_info)
    let required_validation_layer_raw_names: Vec<CString> = if validation_info.is_enable {
        validation_info
            .required_validation_layers
            .iter()
            .map(|layer_name| CString::new(*layer_name).unwrap())
            .collect()
    } else {
        Vec::new()
    };
    let layer_names: Vec<*const i8> = required_validation_layer_raw_names
        .iter()
        .map(|layer_name| layer_name.as_ptr())
        .collect();

    let mut create_info = vk::InstanceCreateInfo::default()
        .application_info(&app_info)
        .flags(platforms::instance_create_flags())
        .enabled_extension_names(&extension_names);

    // Add validation layers if enabled
    if validation_info.is_enable {
        create_info = create_info.enabled_layer_names(&layer_names);
    }

    let instance: ash::Instance = unsafe {
        entry
            .create_instance(&create_info, None)
            .expect("Failed to create instance!")
    };

    instance
}

pub fn pick_physical_device(
    instance: &ash::Instance,
    surface: &SurfaceWrapper,
    required_device_extensions: &DeviceExtension,
) -> vk::PhysicalDevice {
    let physical_devices = unsafe {
        instance
            .enumerate_physical_devices()
            .expect("Failed to enumerate Physical Devices!")
    };

    log::debug!(
        "{} devices (GPU) found with vulkan support.",
        physical_devices.len()
    );

    // let mut result = None;
    // for &physical_device in physical_devices.iter() {
    //     if is_physical_device_suitable(instance, physical_device, surface, required_device_extensions) {
    //         if result.is_none() {
    //             result = Some(physical_device)
    //         }
    //     }
    // }

    let result = physical_devices.iter().find(|physical_device| {
        let is_suitable = is_physical_device_suitable(
            instance,
            **physical_device,
            surface,
            required_device_extensions,
        );

        if is_suitable {
            let device_properties =
                unsafe { instance.get_physical_device_properties(**physical_device) };
            let device_name = tools::vk_to_string(&device_properties.device_name);
            log::debug!("Using GPU: {}", device_name);
        }

        is_suitable
    });

    match result {
        Some(p_physical_device) => *p_physical_device,
        None => panic!("Failed to find a suitable GPU!"),
    }
}

pub fn is_physical_device_suitable(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
    surface: &SurfaceWrapper,
    required_device_extensions: &DeviceExtension,
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

    let indices = find_queue_family(instance, physical_device, surface);

    let is_queue_family_supported = indices.is_complete();
    let is_device_extension_supported =
        check_device_extension_support(instance, physical_device, required_device_extensions);
    let is_swapchain_supported = if is_device_extension_supported {
        let swapchain_support = query_swapchain_support(physical_device, surface);
        !swapchain_support.formats.is_empty() && !swapchain_support.present_modes.is_empty()
    } else {
        false
    };
    // let is_support_sampler_anisotropy = device_features.sampler_anisotropy == 1;

    return is_queue_family_supported && is_device_extension_supported && is_swapchain_supported;
    // && is_support_sampler_anisotropy;
}

pub fn find_queue_family(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
    surface_stuff: &SurfaceWrapper,
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
            surface_stuff
                .surface_loader
                .get_physical_device_surface_support(
                    physical_device,
                    index as u32,
                    surface_stuff.surface,
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

pub fn check_device_extension_support(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
    device_extensions: &DeviceExtension,
) -> bool {
    let available_extensions = unsafe {
        instance
            .enumerate_device_extension_properties(physical_device)
            .expect("Failed to get device extension properties.")
    };
    let mut available_extension_names = vec![];

    for extension in available_extensions.iter() {
        let extension_name = tools::vk_to_string(&extension.extension_name);

        available_extension_names.push(extension_name);
    }

    use std::collections::HashSet;
    let mut required_extensions = HashSet::new();
    for extension in device_extensions.names.iter() {
        required_extensions.insert(extension.to_string());
    }

    for extension_name in available_extension_names.iter() {
        required_extensions.remove(extension_name);
    }

    return required_extensions.is_empty();
}

pub fn create_logical_device(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
    device_extensions: &DeviceExtension,
    surface: &SurfaceWrapper,
) -> (ash::Device, QueueFamilyIndices) {
    let indices = find_queue_family(instance, physical_device, surface);

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

    let extensions_names = device_extensions.get_extensions_raw_names();

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

pub fn query_swapchain_support(
    physical_device: vk::PhysicalDevice,
    surface_stuff: &SurfaceWrapper,
) -> SwapchainSupportDetail {
    unsafe {
        let capabilities = surface_stuff
            .surface_loader
            .get_physical_device_surface_capabilities(physical_device, surface_stuff.surface)
            .expect("Failed to query for surface capabilities.");
        let formats = surface_stuff
            .surface_loader
            .get_physical_device_surface_formats(physical_device, surface_stuff.surface)
            .expect("Failed to query for surface formats.");
        let present_modes = surface_stuff
            .surface_loader
            .get_physical_device_surface_present_modes(physical_device, surface_stuff.surface)
            .expect("Failed to query for surface present mode.");

        SwapchainSupportDetail {
            capabilities,
            formats,
            present_modes,
        }
    }
}

pub fn create_swapchain(
    instance: &ash::Instance,
    device: &ash::Device,
    physical_device: vk::PhysicalDevice,
    window: &winit::window::Window,
    surface_stuff: &SurfaceWrapper,
    queue_family: &QueueFamilyIndices,
) -> SwapchainWrapper {
    let swapchain_support = query_swapchain_support(physical_device, surface_stuff);

    let surface_format = choose_swapchain_format(&swapchain_support.formats);
    let present_mode = choose_swapchain_present_mode(&swapchain_support.present_modes);
    let extent = choose_swapchain_extent(&swapchain_support.capabilities, window);

    let image_count = swapchain_support.capabilities.min_image_count + 1;
    let image_count = if swapchain_support.capabilities.max_image_count > 0 {
        image_count.min(swapchain_support.capabilities.max_image_count)
    } else {
        image_count
    };

    let (image_sharing_mode, _queue_family_index_count, queue_family_indices) =
        if queue_family.graphics_family != queue_family.present_family {
            (
                vk::SharingMode::CONCURRENT,
                2,
                vec![
                    queue_family.graphics_family.unwrap(),
                    queue_family.present_family.unwrap(),
                ],
            )
        } else {
            (vk::SharingMode::EXCLUSIVE, 0, vec![])
        };

    let swapchain_create_info = vk::SwapchainCreateInfoKHR::default()
        .surface(surface_stuff.surface)
        .min_image_count(image_count)
        .image_color_space(surface_format.color_space)
        .image_format(surface_format.format)
        .image_extent(extent)
        .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
        .image_sharing_mode(image_sharing_mode)
        .queue_family_indices(&queue_family_indices)
        .pre_transform(swapchain_support.capabilities.current_transform)
        .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
        .present_mode(present_mode)
        .clipped(true)
        .image_array_layers(1);

    let swapchain_loader = ash::khr::swapchain::Device::new(instance, device);
    let swapchain = unsafe {
        swapchain_loader
            .create_swapchain(&swapchain_create_info, None)
            .expect("Failed to create Swapchain!")
    };

    let swapchain_images = unsafe {
        swapchain_loader
            .get_swapchain_images(swapchain)
            .expect("Failed to get Swapchain Images.")
    };

    SwapchainWrapper {
        swapchain_loader,
        swapchain,
        swapchain_format: surface_format.format,
        swapchain_extent: extent,
        swapchain_images,
    }
}

pub fn choose_swapchain_format(
    available_formats: &Vec<vk::SurfaceFormatKHR>,
) -> vk::SurfaceFormatKHR {
    for available_format in available_formats {
        if available_format.format == vk::Format::B8G8R8A8_UNORM
            && available_format.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR
        {
            return available_format.clone();
        }
    }

    return available_formats.first().unwrap().clone();
}

pub fn choose_swapchain_present_mode(
    available_present_modes: &Vec<vk::PresentModeKHR>,
) -> vk::PresentModeKHR {
    for &available_present_mode in available_present_modes.iter() {
        if available_present_mode == vk::PresentModeKHR::MAILBOX {
            return available_present_mode;
        }
    }

    vk::PresentModeKHR::FIFO
}

pub fn choose_swapchain_extent(
    capabilities: &vk::SurfaceCapabilitiesKHR,
    window: &winit::window::Window,
) -> vk::Extent2D {
    let window_size = window.inner_size();
    println!(
        "\t\tInner Window Size: ({}, {})",
        window_size.width, window_size.height
    );
    choose_swapchain_extent_size(
        capabilities,
        window_size.width as u32,
        window_size.height as u32,
    )
}

pub fn choose_swapchain_extent_size(
    capabilities: &vk::SurfaceCapabilitiesKHR,
    window_width: u32,
    window_height: u32,
) -> vk::Extent2D {
    // Some window systems dictate the exact surface extent. Only choose and
    // clamp an extent when the surface reports the Vulkan "undefined" sentinel.
    if capabilities.current_extent.width != u32::MAX {
        return capabilities.current_extent;
    }

    vk::Extent2D {
        width: window_width.clamp(
            capabilities.min_image_extent.width,
            capabilities.max_image_extent.width,
        ),
        height: window_height.clamp(
            capabilities.min_image_extent.height,
            capabilities.max_image_extent.height,
        ),
    }
}

#[cfg(test)]
mod swapchain_extent_tests {
    use super::*;

    #[test]
    fn uses_surface_defined_extent() {
        let capabilities = vk::SurfaceCapabilitiesKHR {
            current_extent: vk::Extent2D {
                width: 1280,
                height: 720,
            },
            min_image_extent: vk::Extent2D {
                width: 64,
                height: 64,
            },
            max_image_extent: vk::Extent2D {
                width: 4096,
                height: 4096,
            },
            ..Default::default()
        };

        assert_eq!(
            choose_swapchain_extent_size(&capabilities, 800, 600),
            capabilities.current_extent
        );
    }

    #[test]
    fn clamps_requested_extent_when_surface_leaves_it_undefined() {
        let capabilities = vk::SurfaceCapabilitiesKHR {
            current_extent: vk::Extent2D {
                width: u32::MAX,
                height: u32::MAX,
            },
            min_image_extent: vk::Extent2D {
                width: 320,
                height: 200,
            },
            max_image_extent: vk::Extent2D {
                width: 1920,
                height: 1080,
            },
            ..Default::default()
        };

        assert_eq!(
            choose_swapchain_extent_size(&capabilities, 2560, 100),
            vk::Extent2D {
                width: 1920,
                height: 200,
            }
        );
    }
}

pub fn create_image_views(
    device: &ash::Device,
    surface_format: vk::Format,
    images: &Vec<vk::Image>,
) -> Vec<vk::ImageView> {
    let swapchain_imageviews: Vec<vk::ImageView> = images
        .iter()
        .map(|&image| {
            create_image_view(
                device,
                image,
                surface_format,
                vk::ImageAspectFlags::COLOR,
                1,
            )
        })
        .collect();

    swapchain_imageviews
}

pub fn create_image_view(
    device: &ash::Device,
    image: vk::Image,
    format: vk::Format,
    aspect_flags: vk::ImageAspectFlags,
    mip_levels: u32,
) -> vk::ImageView {
    let imageview_create_info = vk::ImageViewCreateInfo::default()
        .view_type(vk::ImageViewType::TYPE_2D)
        .format(format)
        .components(vk::ComponentMapping {
            r: vk::ComponentSwizzle::IDENTITY,
            g: vk::ComponentSwizzle::IDENTITY,
            b: vk::ComponentSwizzle::IDENTITY,
            a: vk::ComponentSwizzle::IDENTITY,
        })
        .subresource_range(vk::ImageSubresourceRange {
            aspect_mask: aspect_flags,
            base_mip_level: 0,
            level_count: mip_levels,
            base_array_layer: 0,
            layer_count: 1,
        })
        .image(image);

    unsafe {
        device
            .create_image_view(&imageview_create_info, None)
            .expect("Failed to create Image View!")
    }
}

/// Find a supported depth format
pub fn find_depth_format(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
) -> vk::Format {
    let candidates = [
        vk::Format::D32_SFLOAT,
        vk::Format::D32_SFLOAT_S8_UINT,
        vk::Format::D24_UNORM_S8_UINT,
    ];

    for &format in &candidates {
        let props =
            unsafe { instance.get_physical_device_format_properties(physical_device, format) };
        if props
            .optimal_tiling_features
            .contains(vk::FormatFeatureFlags::DEPTH_STENCIL_ATTACHMENT)
        {
            return format;
        }
    }

    panic!("Failed to find supported depth format!");
}

/// Create depth buffer resources (image, memory, view)
pub fn create_depth_resources(
    device: &ash::Device,
    host_memory: &hostmem::HostMemory,
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
    extent: vk::Extent2D,
    depth_format: vk::Format,
) -> (vk::Image, vk::DeviceMemory, vk::ImageView) {
    // Create depth image
    let image_info = vk::ImageCreateInfo::default()
        .image_type(vk::ImageType::TYPE_2D)
        .extent(vk::Extent3D {
            width: extent.width,
            height: extent.height,
            depth: 1,
        })
        .mip_levels(1)
        .array_layers(1)
        .format(depth_format)
        .tiling(vk::ImageTiling::OPTIMAL)
        .initial_layout(vk::ImageLayout::UNDEFINED)
        .usage(vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_DST)
        .samples(vk::SampleCountFlags::TYPE_1)
        .sharing_mode(vk::SharingMode::EXCLUSIVE);

    let depth_image = unsafe {
        device
            .create_image(&image_info, None)
            .expect("Failed to create depth image!")
    };

    // Allocate memory for depth image
    let mem_requirements = unsafe { device.get_image_memory_requirements(depth_image) };
    let depth_image_memory = unsafe {
        host_memory.allocate_image(
            device,
            instance,
            physical_device,
            mem_requirements,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
        )
    };

    unsafe {
        device
            .bind_image_memory(depth_image, depth_image_memory, 0)
            .expect("Failed to bind depth image memory!");
    }

    // Create depth image view
    let depth_image_view = create_image_view(
        device,
        depth_image,
        depth_format,
        vk::ImageAspectFlags::DEPTH,
        1,
    );

    (depth_image, depth_image_memory, depth_image_view)
}

fn find_memory_type_index(
    type_filter: u32,
    required_properties: vk::MemoryPropertyFlags,
    mem_properties: &vk::PhysicalDeviceMemoryProperties,
) -> u32 {
    for i in 0..mem_properties.memory_type_count {
        if (type_filter & (1 << i)) != 0
            && mem_properties.memory_types[i as usize]
                .property_flags
                .contains(required_properties)
        {
            return i;
        }
    }
    panic!("Failed to find suitable memory type!");
}

pub fn create_render_pass(
    device: &ash::Device,
    surface_format: vk::Format,
    depth_format: vk::Format,
) -> vk::RenderPass {
    // Color attachment
    let color_attachment = vk::AttachmentDescription {
        format: surface_format,
        samples: vk::SampleCountFlags::TYPE_1,
        load_op: vk::AttachmentLoadOp::CLEAR,
        store_op: vk::AttachmentStoreOp::STORE,
        stencil_load_op: vk::AttachmentLoadOp::DONT_CARE,
        stencil_store_op: vk::AttachmentStoreOp::DONT_CARE,
        initial_layout: vk::ImageLayout::UNDEFINED,
        final_layout: vk::ImageLayout::PRESENT_SRC_KHR,
        ..Default::default()
    };

    // Depth attachment
    let depth_attachment = vk::AttachmentDescription {
        format: depth_format,
        samples: vk::SampleCountFlags::TYPE_1,
        load_op: vk::AttachmentLoadOp::CLEAR,
        store_op: vk::AttachmentStoreOp::DONT_CARE,
        stencil_load_op: vk::AttachmentLoadOp::DONT_CARE,
        stencil_store_op: vk::AttachmentStoreOp::DONT_CARE,
        initial_layout: vk::ImageLayout::UNDEFINED,
        final_layout: vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
        ..Default::default()
    };

    let color_attachment_ref = vk::AttachmentReference {
        attachment: 0,
        layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
    };

    let depth_attachment_ref = vk::AttachmentReference {
        attachment: 1,
        layout: vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
    };

    let color_attachment_refs = [color_attachment_ref];

    let subpass = vk::SubpassDescription::default()
        .color_attachments(&color_attachment_refs)
        .depth_stencil_attachment(&depth_attachment_ref)
        .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS);

    let dependencies = [vk::SubpassDependency {
        src_subpass: vk::SUBPASS_EXTERNAL,
        dst_subpass: 0,
        src_stage_mask: vk::PipelineStageFlags::ALL_COMMANDS,
        dst_stage_mask: vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
            | vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS
            | vk::PipelineStageFlags::LATE_FRAGMENT_TESTS,
        src_access_mask: vk::AccessFlags::MEMORY_WRITE,
        dst_access_mask: vk::AccessFlags::COLOR_ATTACHMENT_WRITE
            | vk::AccessFlags::COLOR_ATTACHMENT_READ
            | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE
            | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ,
        ..Default::default()
    }];

    let attachments = [color_attachment, depth_attachment];

    let renderpass_create_info = vk::RenderPassCreateInfo::default()
        .attachments(&attachments)
        .subpasses(std::slice::from_ref(&subpass))
        .dependencies(&dependencies);

    unsafe {
        device
            .create_render_pass(&renderpass_create_info, None)
            .expect("Failed to create render pass!")
    }
}
pub fn create_scene_render_pass(
    device: &ash::Device,
    surface_format: vk::Format,
    depth_format: vk::Format,
) -> vk::RenderPass {
    // Color attachment
    let color_attachment = vk::AttachmentDescription {
        format: surface_format,
        samples: vk::SampleCountFlags::TYPE_1,
        load_op: vk::AttachmentLoadOp::LOAD,
        store_op: vk::AttachmentStoreOp::STORE,
        stencil_load_op: vk::AttachmentLoadOp::DONT_CARE,
        stencil_store_op: vk::AttachmentStoreOp::STORE,
        initial_layout: vk::ImageLayout::GENERAL,
        final_layout: vk::ImageLayout::GENERAL,
        ..Default::default()
    };

    // Depth attachment
    let depth_attachment = vk::AttachmentDescription {
        format: depth_format,
        samples: vk::SampleCountFlags::TYPE_1,
        load_op: vk::AttachmentLoadOp::LOAD,
        store_op: vk::AttachmentStoreOp::STORE,
        stencil_load_op: vk::AttachmentLoadOp::DONT_CARE,
        stencil_store_op: vk::AttachmentStoreOp::STORE,
        initial_layout: vk::ImageLayout::GENERAL,
        final_layout: vk::ImageLayout::GENERAL,
        ..Default::default()
    };

    let color_attachment_ref = vk::AttachmentReference {
        attachment: 0,
        layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
    };

    let depth_attachment_ref = vk::AttachmentReference {
        attachment: 1,
        layout: vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
    };

    let color_attachment_refs = [color_attachment_ref];

    let subpass = vk::SubpassDescription::default()
        .color_attachments(&color_attachment_refs)
        .depth_stencil_attachment(&depth_attachment_ref)
        .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS);

    let dependencies = [vk::SubpassDependency {
        src_subpass: vk::SUBPASS_EXTERNAL,
        dst_subpass: 0,
        src_stage_mask: vk::PipelineStageFlags::ALL_COMMANDS,
        dst_stage_mask: vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
            | vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS
            | vk::PipelineStageFlags::LATE_FRAGMENT_TESTS,
        src_access_mask: vk::AccessFlags::MEMORY_WRITE,
        dst_access_mask: vk::AccessFlags::COLOR_ATTACHMENT_WRITE
            | vk::AccessFlags::COLOR_ATTACHMENT_READ
            | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE
            | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ,
        ..Default::default()
    }];

    let attachments = [color_attachment, depth_attachment];

    let renderpass_create_info = vk::RenderPassCreateInfo::default()
        .attachments(&attachments)
        .subpasses(std::slice::from_ref(&subpass))
        .dependencies(&dependencies);

    unsafe {
        device
            .create_render_pass(&renderpass_create_info, None)
            .expect("Failed to create render pass!")
    }
}

pub fn create_shader_module(device: &ash::Device, code: &[u32]) -> vk::ShaderModule {
    let shader_module_create_info = vk::ShaderModuleCreateInfo::default().code(code);

    unsafe {
        device
            .create_shader_module(&shader_module_create_info, None)
            .expect("Failed to create Shader Module!")
    }
}

// Embedded SPIR-V shaders (compiled at build time)
const GLIDE_VERT_SPV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/glide.vert.spv"));
const GLIDE_FRAG_SPV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/glide.frag.spv"));

/// Convert embedded SPIR-V bytes to u32 words
fn spv_to_words(bytes: &[u8]) -> Vec<u32> {
    bytes
        .chunks_exact(4)
        .map(|chunk| u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect()
}

/// Creates a graphics pipeline with vertex input for Glide rendering
/// Create all graphics pipeline variants for different blend modes.
/// Returns a HashMap of BlendMode → Pipeline, plus the shared PipelineLayout.
pub fn create_glide_graphics_pipelines(
    device: &ash::Device,
    render_pass: vk::RenderPass,
    swapchain_extent: vk::Extent2D,
    descriptor_set_layout: vk::DescriptorSetLayout,
) -> (
    std::collections::HashMap<vertex::BlendMode, vk::Pipeline>,
    vk::PipelineLayout,
) {
    use vertex::BlendMode;

    let blend_configs: [(BlendMode, vk::BlendFactor, vk::BlendFactor); 4] = [
        (
            BlendMode::Opaque,
            vk::BlendFactor::ONE,
            vk::BlendFactor::ZERO,
        ),
        (
            BlendMode::AlphaBlend,
            vk::BlendFactor::SRC_ALPHA,
            vk::BlendFactor::ONE_MINUS_SRC_ALPHA,
        ),
        (
            BlendMode::Additive,
            vk::BlendFactor::ONE,
            vk::BlendFactor::ONE,
        ),
        (
            BlendMode::Multiplicative,
            vk::BlendFactor::ZERO,
            vk::BlendFactor::SRC_COLOR,
        ),
    ];

    let (first_pipeline, pipeline_layout) = create_glide_graphics_pipeline_with_blend(
        device,
        render_pass,
        swapchain_extent,
        descriptor_set_layout,
        blend_configs[0].1,
        blend_configs[0].2,
        None,
    );

    let mut pipelines = std::collections::HashMap::new();
    pipelines.insert(blend_configs[0].0, first_pipeline);

    for &(mode, src, dst) in &blend_configs[1..] {
        let (pipeline, _) = create_glide_graphics_pipeline_with_blend(
            device,
            render_pass,
            swapchain_extent,
            descriptor_set_layout,
            src,
            dst,
            Some(pipeline_layout),
        );
        pipelines.insert(mode, pipeline);
    }

    (pipelines, pipeline_layout)
}

fn create_glide_graphics_pipeline_with_blend(
    device: &ash::Device,
    render_pass: vk::RenderPass,
    swapchain_extent: vk::Extent2D,
    descriptor_set_layout: vk::DescriptorSetLayout,
    src_color_blend: vk::BlendFactor,
    dst_color_blend: vk::BlendFactor,
    existing_layout: Option<vk::PipelineLayout>,
) -> (vk::Pipeline, vk::PipelineLayout) {
    create_state_pipeline(
        device,
        render_pass,
        swapchain_extent,
        descriptor_set_layout,
        vertex::PipelineState {
            blend: [
                src_color_blend.as_raw(),
                dst_color_blend.as_raw(),
                vk::BlendFactor::ONE.as_raw(),
                vk::BlendFactor::ZERO.as_raw(),
            ],
            ..Default::default()
        },
        existing_layout,
    )
}

pub fn create_state_pipeline(
    device: &ash::Device,
    render_pass: vk::RenderPass,
    swapchain_extent: vk::Extent2D,
    descriptor_set_layout: vk::DescriptorSetLayout,
    state: vertex::PipelineState,
    existing_layout: Option<vk::PipelineLayout>,
) -> (vk::Pipeline, vk::PipelineLayout) {
    let vert_shader_code = spv_to_words(GLIDE_VERT_SPV);
    let frag_shader_code = spv_to_words(GLIDE_FRAG_SPV);

    let vert_shader_module = create_shader_module(device, &vert_shader_code);
    let frag_shader_module = create_shader_module(device, &frag_shader_code);

    let main_function_name = CString::new("main").unwrap();

    let shader_stages = [
        vk::PipelineShaderStageCreateInfo::default()
            .module(vert_shader_module)
            .name(&main_function_name)
            .stage(vk::ShaderStageFlags::VERTEX),
        vk::PipelineShaderStageCreateInfo::default()
            .module(frag_shader_module)
            .name(&main_function_name)
            .stage(vk::ShaderStageFlags::FRAGMENT),
    ];

    // Vertex input with our GpuVertex format
    let binding_description = vertex::GpuVertex::get_binding_description();
    let attribute_descriptions = vertex::GpuVertex::get_attribute_descriptions();

    let vertex_input_state_create_info = vk::PipelineVertexInputStateCreateInfo::default()
        .vertex_binding_descriptions(std::slice::from_ref(&binding_description))
        .vertex_attribute_descriptions(&attribute_descriptions);

    let vertex_input_assembly_state_info = vk::PipelineInputAssemblyStateCreateInfo::default()
        .primitive_restart_enable(false)
        .topology(vk::PrimitiveTopology::TRIANGLE_LIST);

    let viewports = [vk::Viewport::default()
        .x(0.0)
        .y(0.0)
        .width(swapchain_extent.width as f32)
        .height(swapchain_extent.height as f32)
        .min_depth(0.0)
        .max_depth(1.0)];

    let scissors = [vk::Rect2D::default()
        .offset(vk::Offset2D { x: 0, y: 0 })
        .extent(swapchain_extent)];

    let viewport_state_create_info = vk::PipelineViewportStateCreateInfo::default()
        .viewports(&viewports)
        .scissors(&scissors);

    // No backface culling - Glide handles this in software
    let rasterization_state_create_info = vk::PipelineRasterizationStateCreateInfo::default()
        .cull_mode(vk::CullModeFlags::NONE)
        .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
        .line_width(1.0)
        .polygon_mode(vk::PolygonMode::FILL)
        .rasterizer_discard_enable(false)
        .depth_clamp_enable(false)
        .depth_bias_enable(false);

    let multisample_state_create_info = vk::PipelineMultisampleStateCreateInfo::default()
        .rasterization_samples(vk::SampleCountFlags::TYPE_1)
        .sample_shading_enable(false);

    let depth_state_create_info = vk::PipelineDepthStencilStateCreateInfo::default()
        .depth_test_enable(state.depth_test)
        .depth_write_enable(state.depth_write)
        .depth_bounds_test_enable(false)
        .depth_compare_op(vk::CompareOp::from_raw(state.depth_compare))
        .stencil_test_enable(false);

    let color_blend_attachment_states = [vk::PipelineColorBlendAttachmentState::default()
        .blend_enable(true)
        .color_write_mask(vk::ColorComponentFlags::from_raw(state.color_mask))
        .src_color_blend_factor(vk::BlendFactor::from_raw(state.blend[0]))
        .dst_color_blend_factor(vk::BlendFactor::from_raw(state.blend[1]))
        .color_blend_op(vk::BlendOp::ADD)
        .src_alpha_blend_factor(vk::BlendFactor::from_raw(state.blend[2]))
        .dst_alpha_blend_factor(vk::BlendFactor::from_raw(state.blend[3]))
        .alpha_blend_op(vk::BlendOp::ADD)];

    let color_blend_state = vk::PipelineColorBlendStateCreateInfo::default()
        .logic_op_enable(false)
        .attachments(&color_blend_attachment_states);

    // Push constants for screen size, texture size, and chromakey
    let push_constant_range = vk::PushConstantRange::default()
        .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT)
        .offset(0)
        .size(std::mem::size_of::<vertex::PushConstants>() as u32);

    let set_layouts = [descriptor_set_layout];
    let pipeline_layout_create_info = vk::PipelineLayoutCreateInfo::default()
        .set_layouts(&set_layouts)
        .push_constant_ranges(std::slice::from_ref(&push_constant_range));

    let pipeline_layout = if let Some(layout) = existing_layout {
        layout
    } else {
        unsafe {
            device
                .create_pipeline_layout(&pipeline_layout_create_info, None)
                .expect("Failed to create pipeline layout!")
        }
    };

    let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
    let dynamic_state_info =
        vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);

    let graphic_pipeline_info = vk::GraphicsPipelineCreateInfo::default()
        .stages(&shader_stages)
        .vertex_input_state(&vertex_input_state_create_info)
        .input_assembly_state(&vertex_input_assembly_state_info)
        .viewport_state(&viewport_state_create_info)
        .rasterization_state(&rasterization_state_create_info)
        .multisample_state(&multisample_state_create_info)
        .depth_stencil_state(&depth_state_create_info)
        .color_blend_state(&color_blend_state)
        .layout(pipeline_layout)
        .render_pass(render_pass)
        .dynamic_state(&dynamic_state_info);

    let graphics_pipelines = unsafe {
        device
            .create_graphics_pipelines(vk::PipelineCache::null(), &[graphic_pipeline_info], None)
            .expect("Failed to create Glide Graphics Pipeline!")
    };

    unsafe {
        device.destroy_shader_module(vert_shader_module, None);
        device.destroy_shader_module(frag_shader_module, None);
    }

    (graphics_pipelines[0], pipeline_layout)
}

pub fn create_framebuffers(
    device: &ash::Device,
    render_pass: vk::RenderPass,
    image_views: &Vec<vk::ImageView>,
    depth_image_view: vk::ImageView,
    swapchain_extent: &vk::Extent2D,
) -> Vec<vk::Framebuffer> {
    let mut framebuffers = vec![];

    for &image_view in image_views.iter() {
        // Attachments: color (index 0), depth (index 1)
        let attachments = [image_view, depth_image_view];

        let framebuffer_create_info = vk::FramebufferCreateInfo::default()
            .render_pass(render_pass)
            .attachments(&attachments)
            .width(swapchain_extent.width)
            .height(swapchain_extent.height)
            .layers(1);

        let framebuffer = unsafe {
            device
                .create_framebuffer(&framebuffer_create_info, None)
                .expect("Failed to create Framebuffer!")
        };

        framebuffers.push(framebuffer);
    }

    framebuffers
}

pub fn create_command_pool(
    device: &ash::Device,
    queue_families: &QueueFamilyIndices,
) -> vk::CommandPool {
    let command_pool_create_info = vk::CommandPoolCreateInfo::default()
        .queue_family_index(queue_families.graphics_family.unwrap());

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
        .command_buffer_count(framebuffers.len() as u32)
        .level(vk::CommandBufferLevel::PRIMARY);

    let command_buffers = unsafe {
        device
            .allocate_command_buffers(&command_buffer_allocate_info)
            .expect("Failed to allocate Command Buffers!")
    };

    command_buffers
}

pub fn create_sync_objects(device: &ash::Device, max_frame_in_flight: usize) -> SyncObjects {
    let mut sync_objects = SyncObjects {
        image_available_semaphores: vec![],
        render_finished_semaphores: vec![],
        inflight_fences: vec![],
    };

    let semaphore_create_info = vk::SemaphoreCreateInfo::default();
    let fence_create_info = vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED);

    for _ in 0..max_frame_in_flight {
        unsafe {
            let image_available_semaphore = device
                .create_semaphore(&semaphore_create_info, None)
                .expect("Failed to create Semaphore Object!");
            let render_finished_semaphore = device
                .create_semaphore(&semaphore_create_info, None)
                .expect("Failed to create Semaphore Object!");
            let inflight_fence = device
                .create_fence(&fence_create_info, None)
                .expect("Failed to create Fence Object!");

            sync_objects
                .image_available_semaphores
                .push(image_available_semaphore);
            sync_objects
                .render_finished_semaphores
                .push(render_finished_semaphore);
            sync_objects.inflight_fences.push(inflight_fence);
        }
    }

    sync_objects
}
