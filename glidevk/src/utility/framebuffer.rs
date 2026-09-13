use super::{texture::VulkanTexture, vertex::VertexBuffer, vulkan::VulkanApp};
use ash::vk;

/// Logical front/back buffers stay application-owned, including after presentation.
pub struct Framebuffer {
    pub images: [vk::Image; 2],
    memory: [vk::DeviceMemory; 2],
    pub views: [vk::ImageView; 2],
    pub frames: Vec<vk::Framebuffer>,
    pub render_pass: vk::RenderPass,
    depth: vk::Image,
    depth_memory: vk::DeviceMemory,
    depth_view: vk::ImageView,
    pub extent: vk::Extent2D,
}

pub fn color_range() -> vk::ImageSubresourceRange {
    vk::ImageSubresourceRange::default()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .level_count(1)
        .layer_count(1)
}

pub fn color_layers() -> vk::ImageSubresourceLayers {
    vk::ImageSubresourceLayers::default()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .layer_count(1)
}

pub unsafe fn barrier(device: &ash::Device, cmd: vk::CommandBuffer) {
    device.cmd_pipeline_barrier(
        cmd,
        vk::PipelineStageFlags::ALL_COMMANDS,
        vk::PipelineStageFlags::ALL_COMMANDS,
        vk::DependencyFlags::empty(),
        &[vk::MemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::MEMORY_READ | vk::AccessFlags::MEMORY_WRITE)
            .dst_access_mask(vk::AccessFlags::MEMORY_READ | vk::AccessFlags::MEMORY_WRITE)],
        &[],
        &[],
    );
}

impl Framebuffer {
    pub fn new(app: &VulkanApp, instance: &ash::Instance, width: u32, height: u32) -> Self {
        let device = &app.device;
        let extent = vk::Extent2D { width, height };
        let mut images = [vk::Image::null(); 2];
        let mut memory = [vk::DeviceMemory::null(); 2];
        let mut views = [vk::ImageView::null(); 2];
        unsafe {
            for i in 0..2 {
                images[i] = device
                    .create_image(
                        &vk::ImageCreateInfo::default()
                            .image_type(vk::ImageType::TYPE_2D)
                            .format(vk::Format::B8G8R8A8_UNORM)
                            .extent(vk::Extent3D {
                                width,
                                height,
                                depth: 1,
                            })
                            .mip_levels(1)
                            .array_layers(1)
                            .samples(vk::SampleCountFlags::TYPE_1)
                            .tiling(vk::ImageTiling::OPTIMAL)
                            .usage(
                                vk::ImageUsageFlags::COLOR_ATTACHMENT
                                    | vk::ImageUsageFlags::TRANSFER_SRC
                                    | vk::ImageUsageFlags::TRANSFER_DST
                                    | vk::ImageUsageFlags::SAMPLED,
                            )
                            .sharing_mode(vk::SharingMode::EXCLUSIVE),
                        None,
                    )
                    .expect("Create logical framebuffer");
                let requirements = device.get_image_memory_requirements(images[i]);
                memory[i] = app.host_memory.allocate_image(
                    device,
                    instance,
                    app.physical_device,
                    requirements,
                    vk::MemoryPropertyFlags::DEVICE_LOCAL,
                );
                device
                    .bind_image_memory(images[i], memory[i], 0)
                    .expect("Bind framebuffer");
                views[i] = super::create_image_view(
                    device,
                    images[i],
                    vk::Format::B8G8R8A8_UNORM,
                    vk::ImageAspectFlags::COLOR,
                    1,
                );
            }
            let (depth, depth_memory, depth_view) = super::create_depth_resources(
                device,
                &app.host_memory,
                instance,
                app.physical_device,
                extent,
                app.depth_format,
            );
            let render_pass = super::create_scene_render_pass(
                device,
                vk::Format::B8G8R8A8_UNORM,
                app.depth_format,
            );
            let frames = super::create_framebuffers(
                device,
                render_pass,
                &views.to_vec(),
                depth_view,
                &extent,
            );
            let cmd = VulkanTexture::begin_single_time_commands(device, app.command_pool);
            for image in images {
                let transition = vk::ImageMemoryBarrier::default()
                    .image(image)
                    .subresource_range(color_range())
                    .old_layout(vk::ImageLayout::UNDEFINED)
                    .new_layout(vk::ImageLayout::GENERAL)
                    .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE);
                device.cmd_pipeline_barrier(
                    cmd,
                    vk::PipelineStageFlags::TOP_OF_PIPE,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &[transition],
                );
                device.cmd_clear_color_image(
                    cmd,
                    image,
                    vk::ImageLayout::GENERAL,
                    &vk::ClearColorValue {
                        float32: [0.0, 0.0, 0.0, 1.0],
                    },
                    &[color_range()],
                );
            }
            // Depth is initialized once, then preserved across ordered submissions.
            let transition = vk::ImageMemoryBarrier::default()
                .image(depth)
                .subresource_range(
                    vk::ImageSubresourceRange::default()
                        .aspect_mask(vk::ImageAspectFlags::DEPTH)
                        .level_count(1)
                        .layer_count(1),
                )
                .old_layout(vk::ImageLayout::UNDEFINED)
                .new_layout(vk::ImageLayout::GENERAL)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE);
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::TOP_OF_PIPE,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[transition],
            );
            device.cmd_clear_depth_stencil_image(
                cmd,
                depth,
                vk::ImageLayout::GENERAL,
                &vk::ClearDepthStencilValue {
                    depth: 1.0,
                    stencil: 0,
                },
                &[transition.subresource_range],
            );
            barrier(device, cmd);
            VulkanTexture::end_single_time_commands(
                device,
                app.command_pool,
                app.graphics_queue,
                cmd,
            );
            Self {
                images,
                memory,
                views,
                frames,
                render_pass,
                depth,
                depth_memory,
                depth_view,
                extent,
            }
        }
    }

    // Allocate staging per LFB operation; reuse a buffer if readback profiling warrants it.
    pub fn read(&self, app: &VulkanApp, instance: &ash::Instance, buffer: usize) -> Vec<u8> {
        let size = self.extent.width as u64 * self.extent.height as u64 * 4;
        let staging = VertexBuffer::new(
            &app.device,
            &app.host_memory,
            instance,
            app.physical_device,
            size,
            vk::BufferUsageFlags::TRANSFER_DST,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        );
        unsafe {
            let cmd = VulkanTexture::begin_single_time_commands(&app.device, app.command_pool);
            barrier(&app.device, cmd);
            let region = vk::BufferImageCopy::default()
                .image_subresource(color_layers())
                .image_extent(vk::Extent3D {
                    width: self.extent.width,
                    height: self.extent.height,
                    depth: 1,
                });
            app.device.cmd_copy_image_to_buffer(
                cmd,
                self.images[buffer],
                vk::ImageLayout::GENERAL,
                staging.buffer,
                &[region],
            );
            barrier(&app.device, cmd);
            VulkanTexture::end_single_time_commands(
                &app.device,
                app.command_pool,
                app.graphics_queue,
                cmd,
            );
            let ptr = app
                .device
                .map_memory(staging.memory, 0, size, vk::MemoryMapFlags::empty())
                .expect("Read framebuffer");
            let pixels = std::slice::from_raw_parts(ptr as *const u8, size as usize).to_vec();
            app.device.unmap_memory(staging.memory);
            staging.destroy(&app.device, &app.host_memory);
            pixels
        }
    }

    pub fn write(&self, app: &VulkanApp, instance: &ash::Instance, buffer: usize, pixels: &[u8]) {
        self.write_rect(
            app,
            instance,
            buffer,
            pixels,
            0,
            0,
            self.extent.width,
            self.extent.height,
        );
    }

    pub fn write_rect(
        &self,
        app: &VulkanApp,
        instance: &ash::Instance,
        buffer: usize,
        pixels: &[u8],
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    ) {
        let staging = VertexBuffer::new(
            &app.device,
            &app.host_memory,
            instance,
            app.physical_device,
            pixels.len() as u64,
            vk::BufferUsageFlags::TRANSFER_SRC,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        );
        super::vertex::upload_to_buffer(&app.device, &staging, &pixels);
        unsafe {
            let cmd = VulkanTexture::begin_single_time_commands(&app.device, app.command_pool);
            barrier(&app.device, cmd);
            let region = vk::BufferImageCopy::default()
                .image_subresource(color_layers())
                .image_offset(vk::Offset3D {
                    x: x as i32,
                    y: y as i32,
                    z: 0,
                })
                .image_extent(vk::Extent3D {
                    width,
                    height,
                    depth: 1,
                });
            app.device.cmd_copy_buffer_to_image(
                cmd,
                staging.buffer,
                self.images[buffer],
                vk::ImageLayout::GENERAL,
                &[region],
            );
            barrier(&app.device, cmd);
            VulkanTexture::end_single_time_commands(
                &app.device,
                app.command_pool,
                app.graphics_queue,
                cmd,
            );
            staging.destroy(&app.device, &app.host_memory);
        }
    }

    pub fn copy_front(&self, app: &VulkanApp) {
        unsafe {
            let cmd = VulkanTexture::begin_single_time_commands(&app.device, app.command_pool);
            barrier(&app.device, cmd);
            let region = vk::ImageCopy::default()
                .src_subresource(color_layers())
                .dst_subresource(color_layers())
                .extent(vk::Extent3D {
                    width: self.extent.width,
                    height: self.extent.height,
                    depth: 1,
                });
            app.device.cmd_copy_image(
                cmd,
                self.images[1],
                vk::ImageLayout::GENERAL,
                self.images[0],
                vk::ImageLayout::GENERAL,
                &[region],
            );
            barrier(&app.device, cmd);
            VulkanTexture::end_single_time_commands(
                &app.device,
                app.command_pool,
                app.graphics_queue,
                cmd,
            );
        }
    }

    pub fn destroy(&self, device: &ash::Device, host_memory: &super::hostmem::HostMemory) {
        unsafe {
            for &frame in &self.frames {
                device.destroy_framebuffer(frame, None);
            }
            device.destroy_render_pass(self.render_pass, None);
            device.destroy_image_view(self.depth_view, None);
            device.destroy_image(self.depth, None);
            host_memory.free(device, self.depth_memory);
            for i in 0..2 {
                device.destroy_image_view(self.views[i], None);
                device.destroy_image(self.images[i], None);
                host_memory.free(device, self.memory[i]);
            }
        }
    }
}
