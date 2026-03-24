use std::collections::HashMap;

use ash::vk;

use crate::glidesys::{AspectRatio, TextureFormat, LOD};

/// Texture data stored in CPU memory
#[derive(Debug)]
pub struct TextureData {
    pub width: u32,
    pub height: u32,
    pub format: TextureFormat,
    pub data: Vec<u8>,
}

impl TextureData {
    pub fn new(width: u32, height: u32, format: TextureFormat, data: Vec<u8>) -> Self {
        TextureData {
            width,
            height,
            format,
            data,
        }
    }
}

/// Palette table for indexed color textures
#[derive(Debug, Clone)]
pub struct PaletteTable {
    pub entries: Vec<u32>, // ARGB colors
}

impl PaletteTable {
    pub fn new() -> Self {
        PaletteTable {
            entries: vec![0; 256],
        }
    }

    pub fn from_data(data: &[u8]) -> Self {
        let mut entries = Vec::with_capacity(256);
        for chunk in data.chunks(4).take(256) {
            if chunk.len() == 4 {
                let color = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                entries.push(color);
            }
        }
        // Pad to 256 entries if needed
        while entries.len() < 256 {
            entries.push(0);
        }
        PaletteTable { entries }
    }
}

impl Default for PaletteTable {
    fn default() -> Self {
        Self::new()
    }
}

/// Texture memory manager for Glide textures
/// Stores textures indexed by their Glide memory address
pub struct TextureMemory {
    textures: HashMap<u32, TextureData>,
    palettes: [PaletteTable; 2], // NCC tables or color palettes per TMU
    total_memory: usize,
    max_memory: usize,
}

impl TextureMemory {
    pub fn new(max_memory_per_tmu: usize) -> Self {
        TextureMemory {
            textures: HashMap::new(),
            palettes: [PaletteTable::new(), PaletteTable::new()],
            total_memory: 0,
            max_memory: max_memory_per_tmu,
        }
    }

    /// Store a texture at the given Glide address
    pub fn store_texture(&mut self, address: u32, texture: TextureData) {
        let size = texture.data.len();

        // Remove old texture at this address if it exists
        if let Some(old) = self.textures.remove(&address) {
            self.total_memory = self.total_memory.saturating_sub(old.data.len());
        }

        self.total_memory += size;
        self.textures.insert(address, texture);

        // Logging disabled to avoid filling log buffer during bulk texture loads
    }

    /// Get a texture at the given Glide address
    pub fn get_texture(&self, address: u32) -> Option<&TextureData> {
        self.textures.get(&address)
    }

    /// Get a mutable texture at the given Glide address
    pub fn get_texture_mut(&mut self, address: u32) -> Option<&mut TextureData> {
        self.textures.get_mut(&address)
    }

    /// Store a palette table for a TMU
    pub fn store_palette(&mut self, tmu: usize, palette: PaletteTable) {
        if tmu < 2 {
            self.palettes[tmu] = palette;
            log::debug!("Stored palette for TMU{}", tmu);
        }
    }

    /// Get palette for a TMU
    pub fn get_palette(&self, tmu: usize) -> Option<&PaletteTable> {
        if tmu < 2 {
            Some(&self.palettes[tmu])
        } else {
            None
        }
    }

    /// Calculate texture size from LOD and aspect ratio
    pub fn calculate_texture_size(lod: LOD, aspect_ratio: AspectRatio) -> (u32, u32) {
        let (w, h) = lod.to_texture_size(aspect_ratio);
        (w as u32, h as u32)
    }

    /// Clear all textures
    pub fn clear(&mut self) {
        self.textures.clear();
        self.total_memory = 0;
    }

    /// Get total memory used
    pub fn memory_used(&self) -> usize {
        self.total_memory
    }

    /// Get max memory available
    pub fn memory_available(&self) -> usize {
        self.max_memory
    }

    /// Set max memory available (for configuration)
    pub fn set_max_memory(&mut self, max_memory: usize) {
        self.max_memory = max_memory;
    }
}

impl Default for TextureMemory {
    fn default() -> Self {
        // 4MB per TMU (typical Voodoo card)
        Self::new(4 * 1024 * 1024)
    }
}

/// Vulkan texture resources for a single texture
pub struct VulkanTexture {
    pub image: vk::Image,
    pub memory: vk::DeviceMemory,
    pub view: vk::ImageView,
    pub width: u32,
    pub height: u32,
}

impl VulkanTexture {
    pub fn new(
        device: &ash::Device,
        instance: &ash::Instance,
        physical_device: vk::PhysicalDevice,
        command_pool: vk::CommandPool,
        graphics_queue: vk::Queue,
        width: u32,
        height: u32,
        rgba_data: &[u8],
        upload_fence: Option<vk::Fence>,
    ) -> Self {
        let image_size = (width * height * 4) as vk::DeviceSize;

        // Create staging buffer
        let (staging_buffer, staging_memory) = Self::create_buffer(
            device,
            instance,
            physical_device,
            image_size,
            vk::BufferUsageFlags::TRANSFER_SRC,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        );

        // Copy data to staging buffer
        unsafe {
            let data_ptr = device
                .map_memory(staging_memory, 0, image_size, vk::MemoryMapFlags::empty())
                .expect("Failed to map staging buffer");
            std::ptr::copy_nonoverlapping(rgba_data.as_ptr(), data_ptr as *mut u8, rgba_data.len());
            device.unmap_memory(staging_memory);
        }

        // Create the image
        let image_create_info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .extent(vk::Extent3D {
                width,
                height,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .format(vk::Format::R8G8B8A8_UNORM)
            .tiling(vk::ImageTiling::OPTIMAL)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .usage(vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .samples(vk::SampleCountFlags::TYPE_1);

        let image = unsafe {
            device
                .create_image(&image_create_info, None)
                .expect("Failed to create texture image")
        };

        // Allocate image memory
        let mem_requirements = unsafe { device.get_image_memory_requirements(image) };
        let memory_type = Self::find_memory_type(
            instance,
            physical_device,
            mem_requirements.memory_type_bits,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
        );

        let alloc_info = vk::MemoryAllocateInfo::default()
            .allocation_size(mem_requirements.size)
            .memory_type_index(memory_type);

        let memory = unsafe {
            device
                .allocate_memory(&alloc_info, None)
                .expect("Failed to allocate texture memory")
        };

        unsafe {
            device
                .bind_image_memory(image, memory, 0)
                .expect("Failed to bind texture memory");
        }

        // Record all transitions and copy in a single command buffer
        {
            let cmd = Self::begin_single_time_commands(device, command_pool);

            // Barrier: UNDEFINED → TRANSFER_DST_OPTIMAL
            let barrier_to_dst = vk::ImageMemoryBarrier::default()
                .old_layout(vk::ImageLayout::UNDEFINED)
                .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(image)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                })
                .src_access_mask(vk::AccessFlags::empty())
                .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE);

            unsafe {
                device.cmd_pipeline_barrier(
                    cmd,
                    vk::PipelineStageFlags::TOP_OF_PIPE,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &[barrier_to_dst],
                );
            }

            // Copy buffer to image
            let region = vk::BufferImageCopy::default()
                .buffer_offset(0)
                .buffer_row_length(0)
                .buffer_image_height(0)
                .image_subresource(vk::ImageSubresourceLayers {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    mip_level: 0,
                    base_array_layer: 0,
                    layer_count: 1,
                })
                .image_offset(vk::Offset3D { x: 0, y: 0, z: 0 })
                .image_extent(vk::Extent3D {
                    width,
                    height,
                    depth: 1,
                });

            unsafe {
                device.cmd_copy_buffer_to_image(
                    cmd,
                    staging_buffer,
                    image,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &[region],
                );
            }

            // Barrier: TRANSFER_DST_OPTIMAL → SHADER_READ_ONLY_OPTIMAL
            let barrier_to_shader = vk::ImageMemoryBarrier::default()
                .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(image)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                })
                .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .dst_access_mask(vk::AccessFlags::SHADER_READ);

            unsafe {
                device.cmd_pipeline_barrier(
                    cmd,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::PipelineStageFlags::FRAGMENT_SHADER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &[barrier_to_shader],
                );
            }

            // Single submission for all operations
            if let Some(fence) = upload_fence {
                Self::end_single_time_commands_with_fence(
                    device, command_pool, graphics_queue, cmd, fence,
                );
            } else {
                Self::end_single_time_commands(device, command_pool, graphics_queue, cmd);
            }
        }

        // Cleanup staging buffer
        unsafe {
            device.destroy_buffer(staging_buffer, None);
            device.free_memory(staging_memory, None);
        }

        // Create image view
        let view_create_info = vk::ImageViewCreateInfo::default()
            .image(image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(vk::Format::R8G8B8A8_UNORM)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            });

        let view = unsafe {
            device
                .create_image_view(&view_create_info, None)
                .expect("Failed to create texture image view")
        };

        VulkanTexture {
            image,
            memory,
            view,
            width,
            height,
        }
    }

    /// Create a texture and record upload commands into an external command buffer.
    /// Returns the texture and staging resources to be cleaned up after the cmd buffer completes.
    pub fn new_into_cmd(
        device: &ash::Device,
        instance: &ash::Instance,
        physical_device: vk::PhysicalDevice,
        cmd: vk::CommandBuffer,
        width: u32,
        height: u32,
        rgba_data: &[u8],
    ) -> (Self, vk::Buffer, vk::DeviceMemory) {
        let image_size = (width * height * 4) as vk::DeviceSize;

        let (staging_buffer, staging_memory) = Self::create_buffer(
            device, instance, physical_device, image_size,
            vk::BufferUsageFlags::TRANSFER_SRC,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        );

        unsafe {
            let data_ptr = device
                .map_memory(staging_memory, 0, image_size, vk::MemoryMapFlags::empty())
                .expect("Failed to map staging buffer");
            std::ptr::copy_nonoverlapping(rgba_data.as_ptr(), data_ptr as *mut u8, rgba_data.len());
            device.unmap_memory(staging_memory);
        }

        let image_create_info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .extent(vk::Extent3D { width, height, depth: 1 })
            .mip_levels(1)
            .array_layers(1)
            .format(vk::Format::R8G8B8A8_UNORM)
            .tiling(vk::ImageTiling::OPTIMAL)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .usage(vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .samples(vk::SampleCountFlags::TYPE_1);

        let image = unsafe {
            device.create_image(&image_create_info, None).expect("Failed to create texture image")
        };

        let mem_requirements = unsafe { device.get_image_memory_requirements(image) };
        let memory_type = Self::find_memory_type(
            instance, physical_device, mem_requirements.memory_type_bits,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
        );

        let alloc_info = vk::MemoryAllocateInfo::default()
            .allocation_size(mem_requirements.size)
            .memory_type_index(memory_type);

        let memory = unsafe {
            device.allocate_memory(&alloc_info, None).expect("Failed to allocate texture memory")
        };

        unsafe {
            device.bind_image_memory(image, memory, 0).expect("Failed to bind texture memory");
        }

        // Record barriers + copy into the provided command buffer
        let subresource_range = vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            base_mip_level: 0, level_count: 1, base_array_layer: 0, layer_count: 1,
        };

        let barrier_to_dst = vk::ImageMemoryBarrier::default()
            .old_layout(vk::ImageLayout::UNDEFINED)
            .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(image)
            .subresource_range(subresource_range)
            .src_access_mask(vk::AccessFlags::empty())
            .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE);

        let region = vk::BufferImageCopy::default()
            .image_subresource(vk::ImageSubresourceLayers {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                mip_level: 0, base_array_layer: 0, layer_count: 1,
            })
            .image_extent(vk::Extent3D { width, height, depth: 1 });

        let barrier_to_shader = vk::ImageMemoryBarrier::default()
            .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(image)
            .subresource_range(subresource_range)
            .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
            .dst_access_mask(vk::AccessFlags::SHADER_READ);

        unsafe {
            device.cmd_pipeline_barrier(cmd, vk::PipelineStageFlags::TOP_OF_PIPE, vk::PipelineStageFlags::TRANSFER, vk::DependencyFlags::empty(), &[], &[], &[barrier_to_dst]);
            device.cmd_copy_buffer_to_image(cmd, staging_buffer, image, vk::ImageLayout::TRANSFER_DST_OPTIMAL, &[region]);
            device.cmd_pipeline_barrier(cmd, vk::PipelineStageFlags::TRANSFER, vk::PipelineStageFlags::FRAGMENT_SHADER, vk::DependencyFlags::empty(), &[], &[], &[barrier_to_shader]);
        }

        let view_create_info = vk::ImageViewCreateInfo::default()
            .image(image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(vk::Format::R8G8B8A8_UNORM)
            .subresource_range(subresource_range);

        let view = unsafe {
            device.create_image_view(&view_create_info, None).expect("Failed to create image view")
        };

        let texture = VulkanTexture { image, memory, view, width, height };
        (texture, staging_buffer, staging_memory)
    }

    fn create_buffer(
        device: &ash::Device,
        instance: &ash::Instance,
        physical_device: vk::PhysicalDevice,
        size: vk::DeviceSize,
        usage: vk::BufferUsageFlags,
        properties: vk::MemoryPropertyFlags,
    ) -> (vk::Buffer, vk::DeviceMemory) {
        let buffer_info = vk::BufferCreateInfo::default()
            .size(size)
            .usage(usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        let buffer = unsafe {
            device
                .create_buffer(&buffer_info, None)
                .expect("Failed to create buffer")
        };

        let mem_requirements = unsafe { device.get_buffer_memory_requirements(buffer) };
        let memory_type = Self::find_memory_type(
            instance,
            physical_device,
            mem_requirements.memory_type_bits,
            properties,
        );

        let alloc_info = vk::MemoryAllocateInfo::default()
            .allocation_size(mem_requirements.size)
            .memory_type_index(memory_type);

        let memory = unsafe {
            device
                .allocate_memory(&alloc_info, None)
                .expect("Failed to allocate buffer memory")
        };

        unsafe {
            device
                .bind_buffer_memory(buffer, memory, 0)
                .expect("Failed to bind buffer memory");
        }

        (buffer, memory)
    }

    fn find_memory_type(
        instance: &ash::Instance,
        physical_device: vk::PhysicalDevice,
        type_filter: u32,
        properties: vk::MemoryPropertyFlags,
    ) -> u32 {
        let mem_properties =
            unsafe { instance.get_physical_device_memory_properties(physical_device) };

        for i in 0..mem_properties.memory_type_count {
            if (type_filter & (1 << i)) != 0
                && mem_properties.memory_types[i as usize]
                    .property_flags
                    .contains(properties)
            {
                return i;
            }
        }
        panic!("Failed to find suitable memory type!");
    }

    fn transition_image_layout(
        device: &ash::Device,
        command_pool: vk::CommandPool,
        queue: vk::Queue,
        image: vk::Image,
        old_layout: vk::ImageLayout,
        new_layout: vk::ImageLayout,
    ) {
        let command_buffer = Self::begin_single_time_commands(device, command_pool);

        let (src_access_mask, dst_access_mask, src_stage, dst_stage) =
            match (old_layout, new_layout) {
                (vk::ImageLayout::UNDEFINED, vk::ImageLayout::TRANSFER_DST_OPTIMAL) => (
                    vk::AccessFlags::empty(),
                    vk::AccessFlags::TRANSFER_WRITE,
                    vk::PipelineStageFlags::TOP_OF_PIPE,
                    vk::PipelineStageFlags::TRANSFER,
                ),
                (
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                ) => (
                    vk::AccessFlags::TRANSFER_WRITE,
                    vk::AccessFlags::SHADER_READ,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::PipelineStageFlags::FRAGMENT_SHADER,
                ),
                _ => panic!("Unsupported layout transition!"),
            };

        let barrier = vk::ImageMemoryBarrier::default()
            .old_layout(old_layout)
            .new_layout(new_layout)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(image)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            })
            .src_access_mask(src_access_mask)
            .dst_access_mask(dst_access_mask);

        unsafe {
            device.cmd_pipeline_barrier(
                command_buffer,
                src_stage,
                dst_stage,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[barrier],
            );
        }

        Self::end_single_time_commands(device, command_pool, queue, command_buffer);
    }

    fn copy_buffer_to_image(
        device: &ash::Device,
        command_pool: vk::CommandPool,
        queue: vk::Queue,
        buffer: vk::Buffer,
        image: vk::Image,
        width: u32,
        height: u32,
    ) {
        let command_buffer = Self::begin_single_time_commands(device, command_pool);

        let region = vk::BufferImageCopy::default()
            .buffer_offset(0)
            .buffer_row_length(0)
            .buffer_image_height(0)
            .image_subresource(vk::ImageSubresourceLayers {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                mip_level: 0,
                base_array_layer: 0,
                layer_count: 1,
            })
            .image_offset(vk::Offset3D { x: 0, y: 0, z: 0 })
            .image_extent(vk::Extent3D {
                width,
                height,
                depth: 1,
            });

        unsafe {
            device.cmd_copy_buffer_to_image(
                command_buffer,
                buffer,
                image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &[region],
            );
        }

        Self::end_single_time_commands(device, command_pool, queue, command_buffer);
    }

    fn begin_single_time_commands(
        device: &ash::Device,
        command_pool: vk::CommandPool,
    ) -> vk::CommandBuffer {
        let alloc_info = vk::CommandBufferAllocateInfo::default()
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_pool(command_pool)
            .command_buffer_count(1);

        let command_buffer = unsafe {
            device
                .allocate_command_buffers(&alloc_info)
                .expect("Failed to allocate command buffer")[0]
        };

        let begin_info = vk::CommandBufferBeginInfo::default()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);

        unsafe {
            device
                .begin_command_buffer(command_buffer, &begin_info)
                .expect("Failed to begin command buffer");
        }

        command_buffer
    }

    fn end_single_time_commands(
        device: &ash::Device,
        command_pool: vk::CommandPool,
        queue: vk::Queue,
        command_buffer: vk::CommandBuffer,
    ) {
        // Uses queue_wait_idle since this is called during init (placeholder texture).
        // For frame-time uploads, use end_single_time_commands_with_fence.
        unsafe {
            device
                .end_command_buffer(command_buffer)
                .expect("Failed to end command buffer");

            let submit_info =
                vk::SubmitInfo::default().command_buffers(std::slice::from_ref(&command_buffer));

            device
                .queue_submit(queue, &[submit_info], vk::Fence::null())
                .expect("Failed to submit command buffer");
            device
                .queue_wait_idle(queue)
                .expect("Failed to wait for queue");

            device.free_command_buffers(command_pool, &[command_buffer]);
        }
    }

    fn end_single_time_commands_with_fence(
        device: &ash::Device,
        command_pool: vk::CommandPool,
        queue: vk::Queue,
        command_buffer: vk::CommandBuffer,
        fence: vk::Fence,
    ) {
        unsafe {
            device
                .end_command_buffer(command_buffer)
                .expect("Failed to end command buffer");

            device
                .reset_fences(&[fence])
                .expect("Failed to reset upload fence");

            let submit_info =
                vk::SubmitInfo::default().command_buffers(std::slice::from_ref(&command_buffer));

            device
                .queue_submit(queue, &[submit_info], fence)
                .expect("Failed to submit command buffer");
            device
                .wait_for_fences(&[fence], true, u64::MAX)
                .expect("Failed to wait for upload fence");

            device.free_command_buffers(command_pool, &[command_buffer]);
        }
    }

    pub fn destroy(&self, device: &ash::Device) {
        unsafe {
            device.destroy_image_view(self.view, None);
            device.destroy_image(self.image, None);
            device.free_memory(self.memory, None);
        }
    }
}

/// Maximum number of textures in the Vulkan cache before eviction.
const MAX_CACHED_TEXTURES: usize = 2048;

/// Cache of Vulkan textures keyed by Glide address
pub struct TextureCache {
    textures: HashMap<u32, VulkanTexture>,
    sampler: vk::Sampler,
    descriptor_set_layout: vk::DescriptorSetLayout,
    descriptor_pool: vk::DescriptorPool,
    descriptor_sets: HashMap<u32, vk::DescriptorSet>,
    /// Insertion order for LRU eviction (oldest first)
    insertion_order: Vec<u32>,
    /// Placeholder texture (1x1 white) for when no texture is bound
    placeholder: Option<VulkanTexture>,
    placeholder_descriptor: vk::DescriptorSet,
    /// Textures/descriptors pending destruction (deferred until GPU is idle)
    pending_destroy: Vec<(VulkanTexture, Option<vk::DescriptorSet>)>,
    /// Reusable fence for texture upload submissions (avoids create/destroy per upload)
    upload_fence: vk::Fence,
}

impl TextureCache {
    pub fn new(device: &ash::Device) -> Self {
        // Create sampler
        let sampler_info = vk::SamplerCreateInfo::default()
            .mag_filter(vk::Filter::NEAREST)
            .min_filter(vk::Filter::NEAREST)
            .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .anisotropy_enable(false)
            .max_anisotropy(1.0)
            .border_color(vk::BorderColor::INT_OPAQUE_BLACK)
            .unnormalized_coordinates(false)
            .compare_enable(false)
            .compare_op(vk::CompareOp::ALWAYS)
            .mipmap_mode(vk::SamplerMipmapMode::NEAREST)
            .mip_lod_bias(0.0)
            .min_lod(0.0)
            .max_lod(0.0);

        let sampler = unsafe {
            device
                .create_sampler(&sampler_info, None)
                .expect("Failed to create texture sampler")
        };

        // Create descriptor set layout
        let layout_binding = vk::DescriptorSetLayoutBinding::default()
            .binding(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT);

        let layout_info = vk::DescriptorSetLayoutCreateInfo::default()
            .bindings(std::slice::from_ref(&layout_binding));

        let descriptor_set_layout = unsafe {
            device
                .create_descriptor_set_layout(&layout_info, None)
                .expect("Failed to create descriptor set layout")
        };

        // Create descriptor pool (allow up to 1024 textures)
        // Use FREE_DESCRIPTOR_SET_BIT to allow freeing individual descriptor sets
        let pool_size = vk::DescriptorPoolSize::default()
            .ty(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(MAX_CACHED_TEXTURES as u32 + 1);

        let pool_info = vk::DescriptorPoolCreateInfo::default()
            .pool_sizes(std::slice::from_ref(&pool_size))
            .max_sets(MAX_CACHED_TEXTURES as u32 + 1)
            .flags(vk::DescriptorPoolCreateFlags::FREE_DESCRIPTOR_SET);

        let descriptor_pool = unsafe {
            device
                .create_descriptor_pool(&pool_info, None)
                .expect("Failed to create descriptor pool")
        };

        // Allocate a placeholder descriptor set (will be updated when placeholder texture is created)
        let alloc_info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(descriptor_pool)
            .set_layouts(std::slice::from_ref(&descriptor_set_layout));

        let placeholder_descriptor = unsafe {
            device
                .allocate_descriptor_sets(&alloc_info)
                .expect("Failed to allocate placeholder descriptor set")[0]
        };

        let upload_fence = unsafe {
            // Start signaled so first reset_fences succeeds
            let fence_info = vk::FenceCreateInfo::default()
                .flags(vk::FenceCreateFlags::SIGNALED);
            device
                .create_fence(&fence_info, None)
                .expect("Failed to create upload fence")
        };

        TextureCache {
            textures: HashMap::new(),
            sampler,
            descriptor_set_layout,
            descriptor_pool,
            descriptor_sets: HashMap::new(),
            insertion_order: Vec::new(),
            placeholder: None,
            placeholder_descriptor,
            pending_destroy: Vec::new(),
            upload_fence,
        }
    }

    /// Upload a raw RGBA8 texture to the cache
    pub fn upload_rgba8(
        &mut self,
        device: &ash::Device,
        instance: &ash::Instance,
        physical_device: vk::PhysicalDevice,
        command_pool: vk::CommandPool,
        queue: vk::Queue,
        texture_addr: u32,
        width: u32,
        height: u32,
        rgba_data: &[u8],
    ) {
        // Create new Vulkan texture from RGBA data
        let texture = VulkanTexture::new(
            device,
            instance,
            physical_device,
            command_pool,
            queue,
            width,
            height,
            rgba_data,
            None, // init-time upload, no reusable fence
        );

        // Allocate descriptor set
        let alloc_info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(self.descriptor_pool)
            .set_layouts(std::slice::from_ref(&self.descriptor_set_layout));

        let descriptor_set = unsafe {
            device
                .allocate_descriptor_sets(&alloc_info)
                .expect("Failed to allocate descriptor set")[0]
        };

        // Update descriptor set
        let image_info = vk::DescriptorImageInfo::default()
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .image_view(texture.view)
            .sampler(self.sampler);

        let descriptor_write = vk::WriteDescriptorSet::default()
            .dst_set(descriptor_set)
            .dst_binding(0)
            .dst_array_element(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .image_info(std::slice::from_ref(&image_info));

        unsafe {
            device.update_descriptor_sets(&[descriptor_write], &[]);
        }

        // Remove old texture and descriptor set if they exist
        if let Some(old_texture) = self.textures.remove(&texture_addr) {
            old_texture.destroy(device);
            self.insertion_order.retain(|&a| a != texture_addr);
        }
        if let Some(old_descriptor_set) = self.descriptor_sets.remove(&texture_addr) {
            unsafe {
                device
                    .free_descriptor_sets(self.descriptor_pool, &[old_descriptor_set])
                    .unwrap_or(());
            }
        }

        self.textures.insert(texture_addr, texture);
        self.descriptor_sets.insert(texture_addr, descriptor_set);
        self.insertion_order.push(texture_addr);
    }

    /// Create the placeholder texture (1x1 white pixel)
    pub fn create_placeholder(
        &mut self,
        device: &ash::Device,
        instance: &ash::Instance,
        physical_device: vk::PhysicalDevice,
        command_pool: vk::CommandPool,
        graphics_queue: vk::Queue,
    ) {
        // 1x1 white pixel
        let white_pixel: [u8; 4] = [255, 255, 255, 255];

        let texture = VulkanTexture::new(
            device,
            instance,
            physical_device,
            command_pool,
            graphics_queue,
            1,
            1,
            &white_pixel,
            None, // init-time upload
        );

        // Update placeholder descriptor set
        let image_info = vk::DescriptorImageInfo::default()
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .image_view(texture.view)
            .sampler(self.sampler);

        let descriptor_write = vk::WriteDescriptorSet::default()
            .dst_set(self.placeholder_descriptor)
            .dst_binding(0)
            .dst_array_element(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .image_info(std::slice::from_ref(&image_info));

        unsafe {
            device.update_descriptor_sets(&[descriptor_write], &[]);
        }

        self.placeholder = Some(texture);
    }

    /// Upload a texture and create its descriptor set
    pub fn upload_texture(
        &mut self,
        device: &ash::Device,
        instance: &ash::Instance,
        physical_device: vk::PhysicalDevice,
        command_pool: vk::CommandPool,
        graphics_queue: vk::Queue,
        address: u32,
        texture_data: &TextureData,
        palette: Option<&PaletteTable>,
    ) {
        // Convert texture to RGBA8
        let rgba_data = convert_to_rgba8(texture_data, palette);

        // Create Vulkan texture (use reusable fence for frame-time uploads)
        let texture = VulkanTexture::new(
            device,
            instance,
            physical_device,
            command_pool,
            graphics_queue,
            texture_data.width,
            texture_data.height,
            &rgba_data,
            Some(self.upload_fence),
        );

        // Allocate descriptor set
        let alloc_info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(self.descriptor_pool)
            .set_layouts(std::slice::from_ref(&self.descriptor_set_layout));

        let descriptor_set = unsafe {
            device
                .allocate_descriptor_sets(&alloc_info)
                .expect("Failed to allocate descriptor set")[0]
        };

        // Update descriptor set
        let image_info = vk::DescriptorImageInfo::default()
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .image_view(texture.view)
            .sampler(self.sampler);

        let descriptor_write = vk::WriteDescriptorSet::default()
            .dst_set(descriptor_set)
            .dst_binding(0)
            .dst_array_element(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .image_info(std::slice::from_ref(&image_info));

        unsafe {
            device.update_descriptor_sets(&[descriptor_write], &[]);
        }

        // Defer destruction of old texture/descriptor (may still be in-flight)
        let old_descriptor = self.descriptor_sets.remove(&address);
        if let Some(old_texture) = self.textures.remove(&address) {
            self.pending_destroy.push((old_texture, old_descriptor));
            self.insertion_order.retain(|&a| a != address);
        }

        self.textures.insert(address, texture);
        self.descriptor_sets.insert(address, descriptor_set);
        self.insertion_order.push(address);

        log::trace!(
            "Texture cache: {} textures, {} descriptor sets",
            self.textures.len(),
            self.descriptor_sets.len()
        );
    }

    /// Batch-upload multiple textures in a single command buffer submission.
    /// Much faster than individual uploads when loading many textures per frame.
    pub fn batch_upload(
        &mut self,
        device: &ash::Device,
        instance: &ash::Instance,
        physical_device: vk::PhysicalDevice,
        command_pool: vk::CommandPool,
        graphics_queue: vk::Queue,
        textures: Vec<(u32, TextureData, Option<PaletteTable>)>,
    ) {
        if textures.is_empty() {
            return;
        }

        let cmd = VulkanTexture::begin_single_time_commands(device, command_pool);
        let mut staging_resources: Vec<(vk::Buffer, vk::DeviceMemory)> = Vec::new();

        for (address, texture_data, palette) in &textures {
            let rgba_data = convert_to_rgba8(texture_data, palette.as_ref());

            let (texture, staging_buf, staging_mem) = VulkanTexture::new_into_cmd(
                device, instance, physical_device, cmd,
                texture_data.width, texture_data.height, &rgba_data,
            );
            staging_resources.push((staging_buf, staging_mem));

            // Create and update descriptor set
            let alloc_info = vk::DescriptorSetAllocateInfo::default()
                .descriptor_pool(self.descriptor_pool)
                .set_layouts(std::slice::from_ref(&self.descriptor_set_layout));

            let descriptor_set = unsafe {
                device.allocate_descriptor_sets(&alloc_info)
                    .expect("Failed to allocate descriptor set")[0]
            };

            let image_info = vk::DescriptorImageInfo::default()
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .image_view(texture.view)
                .sampler(self.sampler);

            let descriptor_write = vk::WriteDescriptorSet::default()
                .dst_set(descriptor_set)
                .dst_binding(0)
                .dst_array_element(0)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .image_info(std::slice::from_ref(&image_info));

            unsafe {
                device.update_descriptor_sets(&[descriptor_write], &[]);
            }

            // Defer old texture/descriptor destruction
            let old_descriptor = self.descriptor_sets.remove(address);
            if let Some(old_texture) = self.textures.remove(address) {
                self.pending_destroy.push((old_texture, old_descriptor));
                self.insertion_order.retain(|&a| a != *address);
            }

            self.textures.insert(*address, texture);
            self.descriptor_sets.insert(*address, descriptor_set);
            self.insertion_order.push(*address);
        }

        // Single submission for all texture uploads
        VulkanTexture::end_single_time_commands_with_fence(
            device, command_pool, graphics_queue, cmd, self.upload_fence,
        );

        // Cleanup all staging buffers
        unsafe {
            for (buf, mem) in staging_resources {
                device.destroy_buffer(buf, None);
                device.free_memory(mem, None);
            }
        }

        log::debug!("Batch uploaded {} textures", textures.len());
    }

    /// Get descriptor set for a texture address (marks as recently used for LRU)
    pub fn get_descriptor_set(&mut self, address: u32) -> vk::DescriptorSet {
        if self.descriptor_sets.contains_key(&address) {
            // Move to back of LRU order
            if let Some(pos) = self.insertion_order.iter().position(|&a| a == address) {
                self.insertion_order.remove(pos);
                self.insertion_order.push(address);
            }
        }
        self.descriptor_sets
            .get(&address)
            .copied()
            .unwrap_or(self.placeholder_descriptor)
    }

    /// Check if a texture exists at the given address
    pub fn has_texture(&self, address: u32) -> bool {
        self.textures.contains_key(&address)
    }

    /// Get texture dimensions for an address (returns placeholder size if not found)
    pub fn get_texture_size(&self, address: u32) -> (u32, u32) {
        if let Some(tex) = self.textures.get(&address) {
            (tex.width, tex.height)
        } else {
            log::warn!(
                "get_texture_size: texture {:#x} not in cache, using default 256x256",
                address
            );
            (256, 256)
        }
    }

    /// Check if eviction is needed without actually evicting
    pub fn needs_eviction(&self) -> bool {
        self.textures.len() >= MAX_CACHED_TEXTURES
    }

    /// Evict least-recently-used textures, protecting those needed by the current frame.
    /// Must be called only after all in-flight GPU work using these textures has completed.
    pub fn evict_if_needed(
        &mut self,
        device: &ash::Device,
        protected: &std::collections::HashSet<u32>,
    ) {
        if self.textures.len() < MAX_CACHED_TEXTURES {
            return;
        }

        // Evict oldest entries that aren't in the protected set
        let target = MAX_CACHED_TEXTURES / 2;
        let mut evicted = 0;
        let mut new_order = Vec::with_capacity(self.insertion_order.len());

        for addr in self.insertion_order.drain(..) {
            if self.textures.len() - evicted <= target {
                new_order.push(addr);
                continue;
            }
            if protected.contains(&addr) {
                new_order.push(addr);
                continue;
            }
            // Evict this texture
            if let Some(descriptor_set) = self.descriptor_sets.remove(&addr) {
                unsafe {
                    let _ = device.free_descriptor_sets(self.descriptor_pool, &[descriptor_set]);
                }
            }
            if let Some(texture) = self.textures.remove(&addr) {
                texture.destroy(device);
            }
            evicted += 1;
        }
        self.insertion_order = new_order;

        if evicted > 0 {
            log::debug!(
                "Texture cache eviction: removed {}, remaining {}",
                evicted,
                self.textures.len()
            );
        }
    }

    /// Invalidate (remove) a texture from the cache.
    /// Destruction is deferred until flush_pending_destroy() is called after
    /// all in-flight GPU work referencing this texture has completed.
    pub fn invalidate_texture(&mut self, _device: &ash::Device, address: u32) {
        let descriptor = self.descriptor_sets.remove(&address);
        if let Some(texture) = self.textures.remove(&address) {
            self.pending_destroy.push((texture, descriptor));
            self.insertion_order.retain(|&a| a != address);
            log::trace!("Invalidated texture at {:#x} (deferred destroy)", address);
        }
    }

    /// Destroy textures that were previously invalidated.
    /// Must only be called after all in-flight GPU work has completed.
    pub fn flush_pending_destroy(&mut self, device: &ash::Device) {
        for (texture, descriptor) in self.pending_destroy.drain(..) {
            if let Some(ds) = descriptor {
                unsafe {
                    let _ = device.free_descriptor_sets(self.descriptor_pool, &[ds]);
                }
            }
            texture.destroy(device);
        }
    }

    /// Get the number of textures currently in cache
    pub fn texture_count(&self) -> usize {
        self.textures.len()
    }

    /// Get the descriptor set layout for pipeline creation
    pub fn descriptor_set_layout(&self) -> vk::DescriptorSetLayout {
        self.descriptor_set_layout
    }

    pub fn destroy(&self, device: &ash::Device) {
        for texture in self.textures.values() {
            texture.destroy(device);
        }
        if let Some(ref placeholder) = self.placeholder {
            placeholder.destroy(device);
        }
        unsafe {
            device.destroy_sampler(self.sampler, None);
            device.destroy_descriptor_pool(self.descriptor_pool, None);
            device.destroy_descriptor_set_layout(self.descriptor_set_layout, None);
        }
    }
}

/// Convert Glide texture data to RGBA8 format
pub fn convert_to_rgba8(texture: &TextureData, palette: Option<&PaletteTable>) -> Vec<u8> {
    let pixel_count = (texture.width * texture.height) as usize;
    let mut rgba = vec![0u8; pixel_count * 4];

    match texture.format {
        TextureFormat::P8 => {
            // 8-bit paletted texture
            if let Some(pal) = palette {
                for i in 0..pixel_count.min(texture.data.len()) {
                    let index = texture.data[i] as usize;
                    let argb = pal.entries.get(index).copied().unwrap_or(0xFF00FF00); // Magenta for missing
                                                                                      // ARGB -> RGBA
                    rgba[i * 4] = ((argb >> 16) & 0xFF) as u8; // R
                    rgba[i * 4 + 1] = ((argb >> 8) & 0xFF) as u8; // G
                    rgba[i * 4 + 2] = (argb & 0xFF) as u8; // B
                    rgba[i * 4 + 3] = ((argb >> 24) & 0xFF) as u8; // A
                }
            } else {
                // No palette - show grayscale based on index
                for i in 0..pixel_count.min(texture.data.len()) {
                    let gray = texture.data[i];
                    rgba[i * 4] = gray;
                    rgba[i * 4 + 1] = gray;
                    rgba[i * 4 + 2] = gray;
                    rgba[i * 4 + 3] = 255;
                }
            }
        }
        TextureFormat::Rgb565 => {
            // 16-bit RGB565
            for i in 0..pixel_count {
                let offset = i * 2;
                if offset + 1 < texture.data.len() {
                    let pixel =
                        u16::from_le_bytes([texture.data[offset], texture.data[offset + 1]]);
                    let r = ((pixel >> 11) & 0x1F) as u8;
                    let g = ((pixel >> 5) & 0x3F) as u8;
                    let b = (pixel & 0x1F) as u8;
                    // Expand to 8-bit
                    rgba[i * 4] = (r << 3) | (r >> 2);
                    rgba[i * 4 + 1] = (g << 2) | (g >> 4);
                    rgba[i * 4 + 2] = (b << 3) | (b >> 2);
                    rgba[i * 4 + 3] = 255;
                }
            }
        }
        TextureFormat::Argb1555 => {
            // 16-bit ARGB1555
            for i in 0..pixel_count {
                let offset = i * 2;
                if offset + 1 < texture.data.len() {
                    let pixel =
                        u16::from_le_bytes([texture.data[offset], texture.data[offset + 1]]);
                    let a = if (pixel & 0x8000) != 0 { 255u8 } else { 0u8 };
                    let r = ((pixel >> 10) & 0x1F) as u8;
                    let g = ((pixel >> 5) & 0x1F) as u8;
                    let b = (pixel & 0x1F) as u8;
                    rgba[i * 4] = (r << 3) | (r >> 2);
                    rgba[i * 4 + 1] = (g << 3) | (g >> 2);
                    rgba[i * 4 + 2] = (b << 3) | (b >> 2);
                    rgba[i * 4 + 3] = a;
                }
            }
        }
        TextureFormat::Argb4444 => {
            // 16-bit ARGB4444
            for i in 0..pixel_count {
                let offset = i * 2;
                if offset + 1 < texture.data.len() {
                    let pixel =
                        u16::from_le_bytes([texture.data[offset], texture.data[offset + 1]]);
                    let a = ((pixel >> 12) & 0xF) as u8;
                    let r = ((pixel >> 8) & 0xF) as u8;
                    let g = ((pixel >> 4) & 0xF) as u8;
                    let b = (pixel & 0xF) as u8;
                    rgba[i * 4] = (r << 4) | r;
                    rgba[i * 4 + 1] = (g << 4) | g;
                    rgba[i * 4 + 2] = (b << 4) | b;
                    rgba[i * 4 + 3] = (a << 4) | a;
                }
            }
        }
        TextureFormat::Intensity8 | TextureFormat::Alpha8 => {
            // 8-bit intensity or alpha
            for i in 0..pixel_count.min(texture.data.len()) {
                let v = texture.data[i];
                rgba[i * 4] = v;
                rgba[i * 4 + 1] = v;
                rgba[i * 4 + 2] = v;
                rgba[i * 4 + 3] = v;
            }
        }
        TextureFormat::AlphaIntensity44 => {
            // 8-bit alpha + intensity (4 bits each)
            for i in 0..pixel_count.min(texture.data.len()) {
                let v = texture.data[i];
                let a = (v >> 4) & 0xF;
                let intensity = v & 0xF;
                let i8 = (intensity << 4) | intensity;
                let a8 = (a << 4) | a;
                rgba[i * 4] = i8;
                rgba[i * 4 + 1] = i8;
                rgba[i * 4 + 2] = i8;
                rgba[i * 4 + 3] = a8;
            }
        }
        TextureFormat::AlphaIntensity88 => {
            // 16-bit alpha + intensity (8 bits each)
            for i in 0..pixel_count {
                let offset = i * 2;
                if offset + 1 < texture.data.len() {
                    let intensity = texture.data[offset];
                    let alpha = texture.data[offset + 1];
                    rgba[i * 4] = intensity;
                    rgba[i * 4 + 1] = intensity;
                    rgba[i * 4 + 2] = intensity;
                    rgba[i * 4 + 3] = alpha;
                }
            }
        }
        _ => {
            // Unsupported format - fill with magenta for debugging
            log::warn!("Unsupported texture format: {:?}", texture.format);
            for i in 0..pixel_count {
                rgba[i * 4] = 255;
                rgba[i * 4 + 1] = 0;
                rgba[i * 4 + 2] = 255;
                rgba[i * 4 + 3] = 255;
            }
        }
    }

    rgba
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_texture_memory_new() {
        let tm = TextureMemory::new(1024 * 1024);
        assert_eq!(tm.memory_used(), 0);
        assert_eq!(tm.memory_available(), 1024 * 1024);
    }

    #[test]
    fn test_store_and_get_texture() {
        let mut tm = TextureMemory::new(1024 * 1024);
        let texture = TextureData::new(64, 64, TextureFormat::Rgb565, vec![0; 64 * 64 * 2]);

        tm.store_texture(0x1000, texture);

        let retrieved = tm.get_texture(0x1000);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().width, 64);
        assert_eq!(retrieved.unwrap().height, 64);
    }

    #[test]
    fn test_palette_table() {
        let mut tm = TextureMemory::new(1024 * 1024);
        let palette = PaletteTable::new();

        tm.store_palette(0, palette);

        let retrieved = tm.get_palette(0);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().entries.len(), 256);
    }

    #[test]
    fn test_memory_tracking() {
        let mut tm = TextureMemory::new(1024 * 1024);

        let texture1 = TextureData::new(32, 32, TextureFormat::Rgb565, vec![0; 32 * 32 * 2]);
        tm.store_texture(0x1000, texture1);
        assert_eq!(tm.memory_used(), 32 * 32 * 2);

        let texture2 = TextureData::new(64, 64, TextureFormat::Rgb565, vec![0; 64 * 64 * 2]);
        tm.store_texture(0x2000, texture2);
        assert_eq!(tm.memory_used(), 32 * 32 * 2 + 64 * 64 * 2);
    }

    #[test]
    fn test_replace_texture() {
        let mut tm = TextureMemory::new(1024 * 1024);

        let texture1 = TextureData::new(32, 32, TextureFormat::Rgb565, vec![0; 32 * 32 * 2]);
        tm.store_texture(0x1000, texture1);
        assert_eq!(tm.memory_used(), 32 * 32 * 2);

        // Replace with larger texture
        let texture2 = TextureData::new(64, 64, TextureFormat::Rgb565, vec![0; 64 * 64 * 2]);
        tm.store_texture(0x1000, texture2);
        assert_eq!(tm.memory_used(), 64 * 64 * 2);
    }
}
