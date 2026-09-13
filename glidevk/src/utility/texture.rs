use std::collections::HashMap;

use ash::vk;

use crate::glidesys::{AspectRatio, TextureFormat, LOD};

/// Texture data stored in CPU memory
#[derive(Debug, Clone)]
pub struct TextureData {
    pub width: u32,
    pub height: u32,
    pub format: TextureFormat,
    pub data: Vec<u8>,
    pub mipmaps: Vec<TextureData>,
}

impl TextureData {
    pub fn new(width: u32, height: u32, format: TextureFormat, data: Vec<u8>) -> Self {
        TextureData {
            width,
            height,
            format,
            data,
            mipmaps: Vec::new(),
        }
    }
}

impl TextureData {
    pub fn level_mut(&mut self, width: u32, height: u32) -> &mut TextureData {
        if (width, height) == (self.width, self.height) {
            return self;
        }
        if let Some(index) = self
            .mipmaps
            .iter()
            .position(|level| (level.width, level.height) == (width, height))
        {
            return &mut self.mipmaps[index];
        }
        self.mipmaps.push(TextureData::new(
            width,
            height,
            self.format,
            vec![0; width as usize * height as usize * self.format.bytes_per_pixel()],
        ));
        self.mipmaps.last_mut().unwrap()
    }
    pub fn byte_len(&self) -> usize {
        self.data.len() + self.mipmaps.iter().map(Self::byte_len).sum::<usize>()
    }
}

/// Palette table for indexed color textures
#[derive(Debug, Clone, PartialEq)]
pub struct PaletteTable {
    pub entries: Vec<u32>, // ARGB colors
    pub extended_alpha: bool,
}

impl PaletteTable {
    pub fn new() -> Self {
        PaletteTable {
            entries: vec![0; 256],
            extended_alpha: false,
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
        PaletteTable {
            entries,
            extended_alpha: false,
        }
    }

    /// Build a palette using the Glide PALETTE_6666 extension semantics.
    ///
    /// The application still supplies ordinary ARGB8888 entries. Hardware
    /// truncates each channel to six bits, so reproduce that quantization and
    /// expand the result back to ARGB8888 for Vulkan sampling.
    pub fn from_argb6666_data(data: &[u8]) -> Self {
        let mut palette = Self::from_data(data);
        palette.extended_alpha = true;
        for color in &mut palette.entries {
            let [b, g, r, a] = color.to_le_bytes();
            let quantize = |channel: u8| {
                let six_bits = channel >> 2;
                (six_bits << 2) | (six_bits >> 4)
            };
            *color = u32::from_le_bytes([quantize(b), quantize(g), quantize(r), quantize(a)]);
        }
        palette
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
    ncc: [PaletteTable; 2],
    palettes: [PaletteTable; 2], // NCC tables or color palettes per TMU
    total_memory: usize,
    max_memory: usize,
}

impl TextureMemory {
    pub fn new(max_memory_per_tmu: usize) -> Self {
        TextureMemory {
            textures: HashMap::new(),
            palettes: [PaletteTable::new(), PaletteTable::new()],
            ncc: [PaletteTable::new(), PaletteTable::new()],
            total_memory: 0,
            max_memory: max_memory_per_tmu,
        }
    }

    /// Store a texture at the given Glide address
    pub fn store_texture(&mut self, address: u32, texture: TextureData) {
        let size = texture.byte_len();

        // Remove old texture at this address if it exists
        if let Some(old) = self.textures.remove(&address) {
            self.total_memory = self.total_memory.saturating_sub(old.byte_len());
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

    pub fn store_ncc(&mut self, index: usize, data: &[u8]) {
        if index >= 2 || data.len() < 64 {
            return;
        }
        let signed = |offset| i16::from_le_bytes([data[offset], data[offset + 1]]) as i32;
        let entries = (0..256)
            .map(|value| {
                let y = data[value >> 4] as i32;
                let rgb: Vec<u32> = (0..3)
                    .map(|channel| {
                        (y + signed(16 + ((value >> 2) & 3) * 6 + channel * 2)
                            + signed(40 + (value & 3) * 6 + channel * 2))
                        .clamp(0, 255) as u32
                    })
                    .collect();
                0xff000000 | (rgb[0] << 16) | (rgb[1] << 8) | rgb[2]
            })
            .collect();
        self.ncc[index] = PaletteTable {
            entries,
            extended_alpha: false,
        };
    }
    pub fn ncc(&self, index: usize) -> &PaletteTable {
        &self.ncc[index.min(1)]
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
        host_memory: &super::hostmem::HostMemory,
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
        let staging = super::vertex::VertexBuffer::new(
            device,
            host_memory,
            instance,
            physical_device,
            image_size,
            vk::BufferUsageFlags::TRANSFER_SRC,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        );
        let (staging_buffer, staging_memory) = (staging.buffer, staging.memory);

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
        let memory = unsafe {
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
                    device,
                    command_pool,
                    graphics_queue,
                    cmd,
                    fence,
                );
            } else {
                Self::end_single_time_commands(device, command_pool, graphics_queue, cmd);
            }
        }

        // Cleanup staging buffer
        staging.destroy(device, host_memory);

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

    /// Compatibility entrypoint for a single RGBA level.
    pub fn new_into_cmd(
        device: &ash::Device,
        host_memory: &super::hostmem::HostMemory,
        instance: &ash::Instance,
        physical_device: vk::PhysicalDevice,
        cmd: vk::CommandBuffer,
        width: u32,
        height: u32,
        rgba_data: &[u8],
    ) -> (Self, vk::Buffer, vk::DeviceMemory) {
        Self::new_mips_into_cmd(
            device,
            host_memory,
            instance,
            physical_device,
            cmd,
            width,
            height,
            &[rgba_data.to_vec()],
        )
    }

    /// Create a texture and record upload commands into an external command buffer.
    /// Returns the texture and staging resources to be cleaned up after the cmd buffer completes.
    pub fn new_mips_into_cmd(
        device: &ash::Device,
        host_memory: &super::hostmem::HostMemory,
        instance: &ash::Instance,
        physical_device: vk::PhysicalDevice,
        cmd: vk::CommandBuffer,
        width: u32,
        height: u32,
        levels: &[Vec<u8>],
    ) -> (Self, vk::Buffer, vk::DeviceMemory) {
        let rgba_data: Vec<u8> = levels.iter().flatten().copied().collect();
        let image_size = rgba_data.len() as vk::DeviceSize;

        let staging = super::vertex::VertexBuffer::new(
            device,
            host_memory,
            instance,
            physical_device,
            image_size,
            vk::BufferUsageFlags::TRANSFER_SRC,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        );
        let (staging_buffer, staging_memory) = (staging.buffer, staging.memory);

        unsafe {
            let data_ptr = device
                .map_memory(staging_memory, 0, image_size, vk::MemoryMapFlags::empty())
                .expect("Failed to map staging buffer");
            std::ptr::copy_nonoverlapping(rgba_data.as_ptr(), data_ptr as *mut u8, rgba_data.len());
            device.unmap_memory(staging_memory);
        }

        let image_create_info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .extent(vk::Extent3D {
                width,
                height,
                depth: 1,
            })
            .mip_levels(levels.len() as u32)
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

        let mem_requirements = unsafe { device.get_image_memory_requirements(image) };
        let memory = unsafe {
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
                .bind_image_memory(image, memory, 0)
                .expect("Failed to bind texture memory");
        }

        // Record barriers + copy into the provided command buffer
        let subresource_range = vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            base_mip_level: 0,
            level_count: levels.len() as u32,
            base_array_layer: 0,
            layer_count: 1,
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

        let mut offset = 0;
        let regions: Vec<_> = levels
            .iter()
            .enumerate()
            .map(|(level, bytes)| {
                let region = vk::BufferImageCopy::default()
                    .buffer_offset(offset)
                    .image_subresource(
                        vk::ImageSubresourceLayers::default()
                            .aspect_mask(vk::ImageAspectFlags::COLOR)
                            .mip_level(level as u32)
                            .layer_count(1),
                    )
                    .image_extent(vk::Extent3D {
                        width: (width >> level).max(1),
                        height: (height >> level).max(1),
                        depth: 1,
                    });
                offset += bytes.len() as u64;
                region
            })
            .collect();

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
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::TOP_OF_PIPE,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[barrier_to_dst],
            );
            device.cmd_copy_buffer_to_image(
                cmd,
                staging_buffer,
                image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &regions,
            );
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

        let view_create_info = vk::ImageViewCreateInfo::default()
            .image(image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(vk::Format::R8G8B8A8_UNORM)
            .subresource_range(subresource_range);

        let view = unsafe {
            device
                .create_image_view(&view_create_info, None)
                .expect("Failed to create image view")
        };

        let texture = VulkanTexture {
            image,
            memory,
            view,
            width,
            height,
        };
        (texture, staging_buffer, staging_memory)
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

    pub(crate) fn begin_single_time_commands(
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

    pub(crate) fn end_single_time_commands(
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

    pub fn destroy(&self, device: &ash::Device, host_memory: &super::hostmem::HostMemory) {
        unsafe {
            device.destroy_image_view(self.view, None);
            device.destroy_image(self.image, None);
            host_memory.free(device, self.memory);
        }
    }
}

/// Maximum number of textures in the Vulkan cache before eviction.
/// Also the size of the descriptor array in the shader.
pub const MAX_CACHED_TEXTURES: usize = 2048;

/// Cache of Vulkan textures keyed by Glide address.
/// Uses descriptor indexing: a single descriptor set with an array of
/// MAX_CACHED_TEXTURES combined image samplers at binding 0. Each texture
/// is assigned a slot index; vertices reference textures by slot index.
pub const DISPLAY_SLOT: u32 = MAX_CACHED_TEXTURES as u32 - 1;

pub struct TextureCache {
    samplers: Vec<vk::Sampler>,
    textures: HashMap<u32, VulkanTexture>,
    descriptor_set_layout: vk::DescriptorSetLayout,
    descriptor_pool: vk::DescriptorPool,
    /// Single shared descriptor set with array of samplers
    descriptor_set: vk::DescriptorSet,
    /// Maps Glide texture address to array slot index
    slot_map: HashMap<u32, u32>,
    /// Available slot indices for reuse
    free_slots: Vec<u32>,
    /// Insertion order for LRU eviction (oldest first)
    insertion_order: Vec<u32>,
    /// Placeholder texture (1x1 white) at slot 0
    placeholder: Option<VulkanTexture>,
    /// Textures pending destruction (deferred until GPU is idle)
    pending_destroy: Vec<VulkanTexture>,
    /// Reusable fence for texture upload submissions (avoids create/destroy per upload)
    upload_fence: vk::Fence,
    /// Palette generation at which each P8 texture was uploaded.
    uploaded_palette_gen: HashMap<u32, u64>,
}

impl TextureCache {
    pub fn new(device: &ash::Device) -> Self {
        // Create descriptor set layout with descriptor indexing flags
        let layout_binding = vk::DescriptorSetLayoutBinding::default()
            .binding(0)
            .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
            .descriptor_count(MAX_CACHED_TEXTURES as u32)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT);

        let binding_flags = [
            vk::DescriptorBindingFlags::PARTIALLY_BOUND
                | vk::DescriptorBindingFlags::UPDATE_AFTER_BIND,
            vk::DescriptorBindingFlags::UPDATE_AFTER_BIND,
        ];
        let bindings = [
            layout_binding,
            vk::DescriptorSetLayoutBinding::default()
                .binding(1)
                .descriptor_type(vk::DescriptorType::SAMPLER)
                .descriptor_count(72)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
        ];
        let mut binding_flags_info =
            vk::DescriptorSetLayoutBindingFlagsCreateInfo::default().binding_flags(&binding_flags);

        let layout_info = vk::DescriptorSetLayoutCreateInfo::default()
            .bindings(&bindings)
            .flags(vk::DescriptorSetLayoutCreateFlags::UPDATE_AFTER_BIND_POOL)
            .push_next(&mut binding_flags_info);

        let descriptor_set_layout = unsafe {
            device
                .create_descriptor_set_layout(&layout_info, None)
                .expect("Failed to create descriptor set layout")
        };

        // Create descriptor pool with UPDATE_AFTER_BIND flag
        let pool_size = vk::DescriptorPoolSize::default()
            .ty(vk::DescriptorType::SAMPLED_IMAGE)
            .descriptor_count(MAX_CACHED_TEXTURES as u32);

        let pool_sizes = [
            pool_size,
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::SAMPLER)
                .descriptor_count(72),
        ];
        let pool_info = vk::DescriptorPoolCreateInfo::default()
            .pool_sizes(&pool_sizes)
            .max_sets(1)
            .flags(vk::DescriptorPoolCreateFlags::UPDATE_AFTER_BIND);

        let descriptor_pool = unsafe {
            device
                .create_descriptor_pool(&pool_info, None)
                .expect("Failed to create descriptor pool")
        };

        // Allocate the single shared descriptor set
        let alloc_info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(descriptor_pool)
            .set_layouts(std::slice::from_ref(&descriptor_set_layout));

        let descriptor_set = unsafe {
            device
                .allocate_descriptor_sets(&alloc_info)
                .expect("Failed to allocate descriptor set")[0]
        };

        let upload_fence = unsafe {
            // Start signaled so first reset_fences succeeds
            let fence_info = vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED);
            device
                .create_fence(&fence_info, None)
                .expect("Failed to create upload fence")
        };

        // Slot 0 is reserved for the placeholder texture.
        // Free slots start from 1.
        let free_slots: Vec<u32> = (1..DISPLAY_SLOT).rev().collect();

        // Nearest mip sampling only; add trilinear variants and keyed LOD bias when needed.
        let samplers: Vec<_> = (0..72)
            .map(|key| {
                let wrap = |mode| match mode {
                    1 => vk::SamplerAddressMode::CLAMP_TO_EDGE,
                    2 => vk::SamplerAddressMode::MIRRORED_REPEAT,
                    _ => vk::SamplerAddressMode::REPEAT,
                };
                unsafe {
                    device
                        .create_sampler(
                            &vk::SamplerCreateInfo::default()
                                .min_filter(vk::Filter::from_raw(key / 18 % 2))
                                .mag_filter(vk::Filter::from_raw(key / 9 % 2))
                                .address_mode_u(wrap(key / 3 % 3))
                                .address_mode_v(wrap(key % 3))
                                .mipmap_mode(vk::SamplerMipmapMode::NEAREST)
                                .max_lod(if key < 36 { 0.0 } else { vk::LOD_CLAMP_NONE }),
                            None,
                        )
                        .expect("Create sampler")
                }
            })
            .collect();
        let infos: Vec<_> = samplers
            .iter()
            .map(|&sampler| vk::DescriptorImageInfo::default().sampler(sampler))
            .collect();
        unsafe {
            device.update_descriptor_sets(
                &[vk::WriteDescriptorSet::default()
                    .dst_set(descriptor_set)
                    .dst_binding(1)
                    .descriptor_type(vk::DescriptorType::SAMPLER)
                    .image_info(&infos)],
                &[],
            );
        }
        TextureCache {
            samplers,
            textures: HashMap::new(),
            descriptor_set_layout,
            descriptor_pool,
            descriptor_set,
            slot_map: HashMap::new(),
            free_slots,
            insertion_order: Vec::new(),
            placeholder: None,
            pending_destroy: Vec::new(),
            upload_fence,
            uploaded_palette_gen: HashMap::new(),
        }
    }

    /// Create the placeholder texture (1x1 white pixel) at slot 0
    pub fn create_placeholder(
        &mut self,
        device: &ash::Device,
        host_memory: &super::hostmem::HostMemory,
        instance: &ash::Instance,
        physical_device: vk::PhysicalDevice,
        command_pool: vk::CommandPool,
        graphics_queue: vk::Queue,
    ) {
        // 1x1 white pixel
        let white_pixel: [u8; 4] = [255, 255, 255, 255];

        let texture = VulkanTexture::new(
            device,
            host_memory,
            instance,
            physical_device,
            command_pool,
            graphics_queue,
            1,
            1,
            &white_pixel,
            None, // init-time upload
        );

        // Update descriptor at slot 0 (reserved for placeholder)
        let image_info = vk::DescriptorImageInfo::default()
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .image_view(texture.view);

        let descriptor_write = vk::WriteDescriptorSet::default()
            .dst_set(self.descriptor_set)
            .dst_binding(0)
            .dst_array_element(0)
            .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
            .image_info(std::slice::from_ref(&image_info));

        unsafe {
            device.update_descriptor_sets(&[descriptor_write], &[]);
        }

        self.placeholder = Some(texture);
    }

    /// Compatibility entrypoints share the synchronized batch uploader.
    pub fn upload_rgba8(
        &mut self,
        device: &ash::Device,
        host_memory: &super::hostmem::HostMemory,
        instance: &ash::Instance,
        physical_device: vk::PhysicalDevice,
        command_pool: vk::CommandPool,
        queue: vk::Queue,
        address: u32,
        width: u32,
        height: u32,
        rgba_data: &[u8],
    ) {
        self.batch_upload_rgba(
            device,
            host_memory,
            instance,
            physical_device,
            command_pool,
            queue,
            vec![(address, width, height, vec![rgba_data.to_vec()])],
        );
    }

    pub fn upload_texture(
        &mut self,
        device: &ash::Device,
        host_memory: &super::hostmem::HostMemory,
        instance: &ash::Instance,
        physical_device: vk::PhysicalDevice,
        command_pool: vk::CommandPool,
        queue: vk::Queue,
        address: u32,
        texture: &TextureData,
        palette: Option<&PaletteTable>,
    ) {
        self.batch_upload(
            device,
            host_memory,
            instance,
            physical_device,
            command_pool,
            queue,
            vec![(address, texture.clone(), palette.cloned())],
        );
    }

    pub fn needs_eviction(&self) -> bool {
        self.free_slots.is_empty()
    }

    /// Caller must have completed all readers before reclaiming cached resources.
    pub fn evict_if_needed(
        &mut self,
        device: &ash::Device,
        host_memory: &super::hostmem::HostMemory,
        protected: &std::collections::HashSet<u32>,
    ) {
        if self.needs_eviction() {
            if let Some(address) = self
                .insertion_order
                .iter()
                .copied()
                .find(|a| !protected.contains(a))
            {
                self.invalidate_texture(device, address);
                self.flush_pending_destroy(device, host_memory);
            }
        }
        self.make_room(device, host_memory, protected);
    }

    /// Batch-upload multiple textures in a single command buffer submission.
    /// Much faster than individual uploads when loading many textures per frame.
    pub fn batch_upload(
        &mut self,
        device: &ash::Device,
        host_memory: &super::hostmem::HostMemory,
        instance: &ash::Instance,
        physical_device: vk::PhysicalDevice,
        command_pool: vk::CommandPool,
        graphics_queue: vk::Queue,
        textures: Vec<(u32, TextureData, Option<PaletteTable>)>,
    ) {
        let textures = textures
            .into_iter()
            .map(|(address, texture_data, palette)| {
                let max_level = texture_data
                    .mipmaps
                    .iter()
                    .map(|m| {
                        texture_data.width.max(texture_data.height).ilog2()
                            - m.width.max(m.height).ilog2()
                    })
                    .max()
                    .unwrap_or(0);
                let mut levels = vec![convert_to_rgba8(&texture_data, palette.as_ref())];
                // Clamp incomplete chains to their last contiguous level.
                for level in 1..=max_level {
                    let (w, h) = (
                        (texture_data.width >> level).max(1),
                        (texture_data.height >> level).max(1),
                    );
                    let Some(mip) = texture_data
                        .mipmaps
                        .iter()
                        .find(|m| (m.width, m.height) == (w, h))
                    else {
                        break;
                    };
                    levels.push(convert_to_rgba8(mip, palette.as_ref()));
                }
                (address, texture_data.width, texture_data.height, levels)
            })
            .collect();
        self.batch_upload_rgba(
            device,
            host_memory,
            instance,
            physical_device,
            command_pool,
            graphics_queue,
            textures,
        );
    }

    fn batch_upload_rgba(
        &mut self,
        device: &ash::Device,
        host_memory: &super::hostmem::HostMemory,
        instance: &ash::Instance,
        physical_device: vk::PhysicalDevice,
        command_pool: vk::CommandPool,
        graphics_queue: vk::Queue,
        textures: Vec<(u32, u32, u32, Vec<Vec<u8>>)>,
    ) {
        if textures.is_empty() {
            return;
        }
        unsafe {
            device
                .device_wait_idle()
                .expect("Wait before descriptor replacement");
        }
        let protected = textures.iter().map(|(address, _, _, _)| *address).collect();
        self.make_room(device, host_memory, &protected);
        let cmd = VulkanTexture::begin_single_time_commands(device, command_pool);
        let mut staging_resources: Vec<(vk::Buffer, vk::DeviceMemory)> = Vec::new();

        for (address, width, height, levels) in &textures {
            // Reuse existing slot or allocate a new one
            let slot = if let Some(old_slot) = self.slot_map.remove(address) {
                if let Some(old_texture) = self.textures.remove(address) {
                    self.pending_destroy.push(old_texture);
                    self.insertion_order.retain(|&a| a != *address);
                }
                old_slot
            } else {
                match self.free_slots.pop() {
                    Some(slot) => slot,
                    None => {
                        log::error!("Texture working set exceeds descriptor capacity; using placeholder for {address:#x}");
                        continue;
                    }
                }
            };

            let (texture, staging_buf, staging_mem) = VulkanTexture::new_mips_into_cmd(
                device,
                host_memory,
                instance,
                physical_device,
                cmd,
                *width,
                *height,
                levels,
            );
            staging_resources.push((staging_buf, staging_mem));

            // Update descriptor at slot index
            let image_info = vk::DescriptorImageInfo::default()
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .image_view(texture.view);

            let descriptor_write = vk::WriteDescriptorSet::default()
                .dst_set(self.descriptor_set)
                .dst_binding(0)
                .dst_array_element(slot)
                .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                .image_info(std::slice::from_ref(&image_info));

            unsafe {
                device.update_descriptor_sets(&[descriptor_write], &[]);
            }

            self.textures.insert(*address, texture);
            self.slot_map.insert(*address, slot);
            self.insertion_order.push(*address);
        }

        // Single submission for all texture uploads
        VulkanTexture::end_single_time_commands_with_fence(
            device,
            command_pool,
            graphics_queue,
            cmd,
            self.upload_fence,
        );

        // Cleanup all staging buffers
        unsafe {
            for (buf, mem) in staging_resources {
                device.destroy_buffer(buf, None);
                host_memory.free(device, mem);
            }
        }

        log::debug!("Batch uploaded {} textures", textures.len());
    }

    /// Get the single shared descriptor set (bound once per frame)
    pub fn get_descriptor_set(&self) -> vk::DescriptorSet {
        self.descriptor_set
    }

    /// Get the array slot index for a texture address (0 = placeholder)
    pub fn get_texture_slot(&self, addr: u32) -> u32 {
        self.slot_map.get(&addr).copied().unwrap_or(0)
    }

    /// Check if a texture exists at the given address
    pub fn has_texture(&self, address: u32) -> bool {
        self.textures.contains_key(&address)
    }

    /// Check if a cached P8 texture is stale due to a palette change.
    /// Non-palettized textures pass `None` and never become palette-stale.
    pub fn is_palette_stale(&self, address: u32, current_gen: Option<u64>) -> bool {
        current_gen.is_some_and(|generation| {
            self.uploaded_palette_gen.get(&address).copied() != Some(generation)
        })
    }

    /// Record the palette generation used by a P8 texture. Passing `None`
    /// explicitly marks the texture as palette-independent.
    pub fn set_palette_gen(&mut self, address: u32, generation: Option<u64>) {
        if let Some(generation) = generation {
            self.uploaded_palette_gen.insert(address, generation);
        } else {
            self.uploaded_palette_gen.remove(&address);
        }
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

    /// Reclaim space before allocation; the caller has completed previous readers.
    pub fn make_room(
        &mut self,
        device: &ash::Device,
        host_memory: &super::hostmem::HostMemory,
        protected: &std::collections::HashSet<u32>,
    ) {
        let required = protected
            .iter()
            .filter(|a| !self.slot_map.contains_key(a))
            .count();
        let candidates = self.insertion_order.clone();
        for address in candidates {
            if self.free_slots.len() >= required {
                break;
            }
            if !protected.contains(&address) {
                self.invalidate_texture(device, address);
            }
        }
        self.flush_pending_destroy(device, host_memory);
    }

    pub fn set_display_image(&self, device: &ash::Device, view: vk::ImageView) {
        let info = vk::DescriptorImageInfo::default()
            .image_view(view)
            .image_layout(vk::ImageLayout::GENERAL);
        unsafe {
            device.update_descriptor_sets(
                &[vk::WriteDescriptorSet::default()
                    .dst_set(self.descriptor_set)
                    .dst_binding(0)
                    .dst_array_element(DISPLAY_SLOT)
                    .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                    .image_info(std::slice::from_ref(&info))],
                &[],
            );
        }
    }

    /// Invalidate (remove) a texture from the cache.
    /// Destruction is deferred until flush_pending_destroy() is called after
    /// all in-flight GPU work referencing this texture has completed.
    pub fn invalidate_texture(&mut self, _device: &ash::Device, address: u32) {
        if let Some(slot) = self.slot_map.remove(&address) {
            self.free_slots.push(slot);
        }
        if let Some(texture) = self.textures.remove(&address) {
            self.pending_destroy.push(texture);
            self.insertion_order.retain(|&a| a != address);
            self.uploaded_palette_gen.remove(&address);
            log::trace!("Invalidated texture at {:#x} (deferred destroy)", address);
        }
    }

    /// Destroy textures that were previously invalidated.
    /// Must only be called after all in-flight GPU work has completed.
    pub fn flush_pending_destroy(
        &mut self,
        device: &ash::Device,
        host_memory: &super::hostmem::HostMemory,
    ) {
        for texture in self.pending_destroy.drain(..) {
            texture.destroy(device, host_memory);
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

    pub fn destroy(&self, device: &ash::Device, host_memory: &super::hostmem::HostMemory) {
        for texture in self.textures.values() {
            texture.destroy(device, host_memory);
        }
        for texture in &self.pending_destroy {
            texture.destroy(device, host_memory);
        }
        if let Some(ref placeholder) = self.placeholder {
            placeholder.destroy(device, host_memory);
        }
        unsafe {
            device.destroy_fence(self.upload_fence, None);
            for sampler in &self.samplers {
                device.destroy_sampler(*sampler, None);
            }
            device.destroy_descriptor_pool(self.descriptor_pool, None);
            device.destroy_descriptor_set_layout(self.descriptor_set_layout, None);
        }
    }
}

/// Convert Glide texture data to RGBA8 format
pub fn convert_to_rgba8(texture: &TextureData, palette: Option<&PaletteTable>) -> Vec<u8> {
    let pixel_count = (texture.width * texture.height) as usize;
    let mut rgba = vec![0u8; pixel_count * 4];

    let expand = |value: u16, bits: u32| -> u8 {
        let v = value as u32;
        match bits {
            2 => (v * 85) as u8,
            3 => ((v << 5) | (v << 2) | (v >> 1)) as u8,
            4 => (v * 17) as u8,
            5 => ((v << 3) | (v >> 2)) as u8,
            6 => ((v << 2) | (v >> 4)) as u8,
            _ => v as u8,
        }
    };
    let bpp = texture.format.bytes_per_pixel();
    for (i, bytes) in texture.data.chunks_exact(bpp).take(pixel_count).enumerate() {
        let p = if bpp == 2 {
            u16::from_le_bytes([bytes[0], bytes[1]])
        } else {
            bytes[0] as u16
        };
        let indexed = |alpha: Option<u8>| {
            let color = palette
                .and_then(|p| p.entries.get(bytes[0] as usize))
                .copied()
                .unwrap_or(0xff000000 | (bytes[0] as u32 * 0x010101));
            [
                (color >> 16) as u8,
                (color >> 8) as u8,
                color as u8,
                alpha.unwrap_or_else(|| {
                    if palette.is_some_and(|p| p.extended_alpha) {
                        (color >> 24) as u8
                    } else {
                        255
                    }
                }),
            ]
        };
        let color = match texture.format {
            TextureFormat::P8 | TextureFormat::Yiq422 => indexed(None),
            TextureFormat::Ap88 | TextureFormat::Ayiq8422 => indexed(Some(bytes[1])),
            TextureFormat::Rgb332 | TextureFormat::Argb8332 => [
                expand((p >> 5) & 7, 3),
                expand((p >> 2) & 7, 3),
                expand(p & 3, 2),
                if bpp == 2 { bytes[1] } else { 255 },
            ],
            TextureFormat::Rgb565 => [
                expand((p >> 11) & 31, 5),
                expand((p >> 5) & 63, 6),
                expand(p & 31, 5),
                255,
            ],
            TextureFormat::Argb1555 => [
                expand((p >> 10) & 31, 5),
                expand((p >> 5) & 31, 5),
                expand(p & 31, 5),
                if p & 0x8000 != 0 { 255 } else { 0 },
            ],
            TextureFormat::Argb4444 => [
                expand((p >> 8) & 15, 4),
                expand((p >> 4) & 15, 4),
                expand(p & 15, 4),
                expand((p >> 12) & 15, 4),
            ],
            TextureFormat::Intensity8 => [bytes[0], bytes[0], bytes[0], 255],
            TextureFormat::Alpha8 => [bytes[0]; 4],
            TextureFormat::AlphaIntensity44 => [
                expand(p & 15, 4),
                expand(p & 15, 4),
                expand(p & 15, 4),
                expand(p >> 4, 4),
            ],
            TextureFormat::AlphaIntensity88 => [bytes[0], bytes[0], bytes[0], bytes[1]],
            _ => [0, 0, 0, 255],
        };
        rgba[i * 4..i * 4 + 4].copy_from_slice(&color);
    }

    rgba
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepted_formats_and_palette_alpha() {
        let mut palette = PaletteTable::new();
        palette.entries[7] = 0x00123456;
        for (format, bytes, expected) in [
            (TextureFormat::Rgb332, vec![0xe3], [255, 0, 255, 255]),
            (TextureFormat::Argb8332, vec![0x1c, 128], [0, 255, 0, 128]),
            (TextureFormat::Ap88, vec![7, 99], [0x12, 0x34, 0x56, 99]),
            (TextureFormat::P8, vec![7], [0x12, 0x34, 0x56, 255]),
            (TextureFormat::Intensity8, vec![50], [50, 50, 50, 255]),
            (TextureFormat::Alpha8, vec![50], [50, 50, 50, 50]),
        ] {
            assert_eq!(
                convert_to_rgba8(&TextureData::new(1, 1, format, bytes), Some(&palette)),
                expected
            );
        }
        palette.extended_alpha = true;
        assert_eq!(
            convert_to_rgba8(
                &TextureData::new(1, 1, TextureFormat::P8, vec![7]),
                Some(&palette)
            )[3],
            0
        );
        let mut memory = TextureMemory::new(1024);
        let mut ncc = [0u8; 64];
        ncc[15] = 100;
        ncc[16..18].copy_from_slice(&50i16.to_le_bytes());
        ncc[42..44].copy_from_slice(&(-25i16).to_le_bytes());
        memory.store_ncc(0, &ncc);
        assert_eq!(
            convert_to_rgba8(
                &TextureData::new(1, 1, TextureFormat::Yiq422, vec![0xf0]),
                Some(memory.ncc(0))
            ),
            [150, 75, 100, 255]
        );
    }

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
    fn test_palette_6666_quantizes_argb_channels() {
        let input = 0xFF80_4020u32.to_le_bytes();
        let palette = PaletteTable::from_argb6666_data(&input);

        // FF,80,40,20 quantized to six bits and expanded becomes FF,82,41,20.
        assert_eq!(palette.entries[0], 0xFF82_4120);
        assert_eq!(palette.entries.len(), 256);
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
