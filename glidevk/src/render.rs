//! Ordered submissions to application-owned logical buffers.
use super::*;
use utility::{
    framebuffer::barrier,
    texture::VulkanTexture,
    vertex::{DrawCall, DrawState, GpuVertex, PipelineState, PushConstants},
};

impl Gr {
    pub(super) fn clip_rect(&self) -> vk::Rect2D {
        let s = &self.state;
        let x = s.clipwindowf_xmin.clamp(0.0, s.screen_width as f32) as u32;
        let right = s.clipwindowf_xmax.clamp(x as f32, s.screen_width as f32) as u32;
        let y = s.clipwindowf_ymin.clamp(0.0, s.screen_height as f32) as u32;
        let bottom = s.clipwindowf_ymax.clamp(y as f32, s.screen_height as f32) as u32;
        vk::Rect2D {
            offset: vk::Offset2D {
                x: x as i32,
                y: if s.origin == OriginLocation::LowerLeft {
                    s.screen_height - bottom
                } else {
                    y
                } as i32,
            },
            extent: vk::Extent2D {
                width: right - x,
                height: bottom - y,
            },
        }
    }

    pub(super) fn pipeline_state(&self) -> PipelineState {
        fn blend(factor: AlphaBlendFnc, source: bool) -> i32 {
            match factor {
                AlphaBlendFnc::Zero => vk::BlendFactor::ZERO,
                AlphaBlendFnc::One => vk::BlendFactor::ONE,
                AlphaBlendFnc::SrcAlpha => vk::BlendFactor::SRC_ALPHA,
                AlphaBlendFnc::SrcColor => {
                    if source {
                        vk::BlendFactor::DST_COLOR
                    } else {
                        vk::BlendFactor::SRC_COLOR
                    }
                }
                AlphaBlendFnc::DstAlpha => vk::BlendFactor::DST_ALPHA,
                AlphaBlendFnc::OneMinusSrcAlpha => vk::BlendFactor::ONE_MINUS_SRC_ALPHA,
                AlphaBlendFnc::OneMinusSrcColor => {
                    if source {
                        vk::BlendFactor::ONE_MINUS_DST_COLOR
                    } else {
                        vk::BlendFactor::ONE_MINUS_SRC_COLOR
                    }
                }
                AlphaBlendFnc::OneMinusDstAlpha => vk::BlendFactor::ONE_MINUS_DST_ALPHA,
                AlphaBlendFnc::AlphaSaturate => vk::BlendFactor::SRC_ALPHA_SATURATE,
                _ => vk::BlendFactor::ZERO,
            }
            .as_raw()
        }
        let s = &self.state;
        let depth_test = !matches!(s.depth_buffer_mode, DepthBufferMode::Disable);
        PipelineState {
            blend: [
                blend(s.alpha_blend_rgb_src, true),
                blend(s.alpha_blend_rgb_dst, false),
                blend(s.alpha_blend_alpha_src, true),
                blend(s.alpha_blend_alpha_dst, false),
            ],
            depth_test,
            depth_write: depth_test && s.depth_mask,
            depth_compare: s.depth_buffer_function as i32,
            color_mask: if s.color_mask_rgb { 7 } else { 0 }
                | if s.color_mask_alpha { 8 } else { 0 },
        }
    }

    pub(super) fn push_constants(&self) -> PushConstants {
        let s = &self.state;
        let (r, g, b, _) = s.chromakey_value.to_rgba_f(s.color_format);
        let (fr, fg, fb, _) = s.fog_color.to_rgba_f(s.color_format);
        let mut table = [0; 16];
        for (word, bytes) in table.iter_mut().zip(s.fog_table.chunks_exact(4)) {
            *word = u32::from_le_bytes(bytes.try_into().unwrap());
        }
        PushConstants {
            screen_size: [s.screen_width as f32, s.screen_height as f32],
            chromakey_r: r,
            chromakey_g: g,
            chromakey_b: b,
            fog_mode: s.fog_mode.0,
            fog_color_r: fr,
            fog_color_g: fg,
            fog_color_b: fb,
            gamma_r: 1.0,
            gamma_g: 1.0,
            gamma_b: 1.0,
            fog_table: table,
            tex_combine: std::array::from_fn(|i| {
                (s.tex_combine_function[i].0 as u32 & 31)
                    | ((s.tex_combine_alpha_function[i].0 as u32 & 31) << 5)
                    | ((s.tex_combine_factor[i] as u32 & 15) << 10)
                    | ((s.tex_combine_alpha_factor[i] as u32 & 15) << 14)
                    | ((s.tex_combine_rgb_invert[i] as u32) << 18)
                    | ((s.tex_combine_alpha_invert[i] as u32) << 19)
                    | (((s.tex_filter_mode_min[i] as u32 * 18
                        + s.tex_filter_mode_mag[i] as u32 * 9
                        + s.tex_clamp_mode_s[i] as u32 * 3
                        + s.tex_clamp_mode_t[i] as u32
                        + if matches!(s.tex_mipmap_mode[i], MipMapMode::Disable) {
                            0
                        } else {
                            36
                        })
                        & 127)
                        << 20)
            }),
            alpha_test: s.alpha_test_function as u32 | ((s.alpha_test_reference.0 as u32) << 8),
        }
    }

    /// Submit the accumulated draws before resource mutation, readback, or presentation.
    /// Synchronous submissions keep descriptors/resources stable; use fence-owned generations if profiling warrants it.
    pub fn finish(&mut self) {
        if self.vertex_batch.is_empty() || self.vulkan_app.is_none() {
            return;
        }
        self.vertex_batch.finalize();
        let needed = self.frame_textures.clone();
        let mut uploads = Vec::new();
        let app = self.vulkan_app.as_mut().unwrap();
        unsafe {
            app.device
                .device_wait_idle()
                .expect("Wait for texture readers");
        }
        app.texture_cache
            .flush_pending_destroy(&app.device, &app.host_memory);
        app.texture_cache
            .make_room(&app.device, &app.host_memory, &needed);
        for &address in &needed {
            if address == u32::MAX {
                continue;
            }
            let tmu = (address >> 31) as usize;
            if let Some(tex) = self.texture_memory[tmu].get_texture(address & 0x7fffffff) {
                let generation = matches!(
                    tex.format,
                    TextureFormat::P8
                        | TextureFormat::Ap88
                        | TextureFormat::Yiq422
                        | TextureFormat::Ayiq8422
                )
                .then_some(self.state.palette_generation);
                if !app.texture_cache.has_texture(address)
                    || app.texture_cache.is_palette_stale(address, generation)
                {
                    let palette =
                        if matches!(tex.format, TextureFormat::Yiq422 | TextureFormat::Ayiq8422) {
                            Some(
                                self.texture_memory[0]
                                    .ncc(if self.state.tex_table_type == TextureTableType::Ncc1 {
                                        1
                                    } else {
                                        0
                                    })
                                    .clone(),
                            )
                        } else {
                            self.texture_memory[0].get_palette(0).cloned()
                        };
                    uploads.push((address, tex.clone(), palette));
                    app.texture_cache.set_palette_gen(address, generation);
                }
            }
        }
        app.texture_cache.batch_upload(
            &app.device,
            &app.host_memory,
            &self.vulkan_instance.instance,
            app.physical_device,
            app.command_pool,
            app.graphics_queue,
            uploads,
        );
        let mut vertices = self.vertex_batch.vertices.clone();
        for vertex in &mut vertices {
            vertex.texture_index = app.texture_cache.get_texture_slot(vertex.texture_index);
            vertex.texture_index1 = app.texture_cache.get_texture_slot(vertex.texture_index1);
        }
        let draws = self.vertex_batch.draw_calls.clone();
        self.render_draws(&vertices, &draws, None);
        self.vertex_batch.clear();
        self.frame_textures.clear();
    }

    pub(super) fn render_vertices(
        &mut self,
        vertices: &[GpuVertex],
        state: PipelineState,
        push: PushConstants,
        clip: vk::Rect2D,
        clear: Option<(Color, Alpha, u32)>,
    ) {
        let draw = DrawCall {
            start_vertex: 0,
            vertex_count: vertices.len() as u32,
            blend_mode: utility::vertex::BlendMode::Opaque,
            state: Some(DrawState {
                pipeline: state,
                push,
                clip,
            }),
        };
        self.render_draws(vertices, &[draw], clear);
    }

    fn render_draws(
        &mut self,
        vertices: &[GpuVertex],
        draws: &[DrawCall],
        clear: Option<(Color, Alpha, u32)>,
    ) {
        let target = if self.state.render_buffer == Buffer::Front {
            0
        } else {
            1
        };
        let color_format = self.state.color_format;
        let depth_mask = self.state.depth_mask;
        let app = self.vulkan_app.as_mut().unwrap();
        let fb = app.framebuffer.as_ref().unwrap();
        let device = &app.device;
        unsafe {
            device
                .device_wait_idle()
                .expect("Wait for prior framebuffer readers");
            for draw in draws {
                let state = draw.state.unwrap().pipeline;
                app.state_pipelines.entry(state).or_insert_with(|| {
                    utility::create_state_pipeline(
                        device,
                        fb.render_pass,
                        fb.extent,
                        app.texture_cache.descriptor_set_layout(),
                        state,
                        Some(app.pipeline_layout),
                    )
                    .0
                });
            }
            let bytes = std::slice::from_raw_parts(
                vertices.as_ptr() as *const u8,
                std::mem::size_of_val(vertices),
            );
            if bytes.len() as u64 > app.vertex_buffers[0].size {
                app.vertex_buffers[0].destroy(device, &app.host_memory);
                app.vertex_buffers[0] = utility::vertex::VertexBuffer::new(
                    device,
                    &app.host_memory,
                    &self.vulkan_instance.instance,
                    app.physical_device,
                    (bytes.len() as u64).next_power_of_two(),
                    vk::BufferUsageFlags::VERTEX_BUFFER,
                    vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
                );
            }
            if !bytes.is_empty() {
                utility::vertex::upload_to_buffer(device, &app.vertex_buffers[0], bytes);
            }
            let cmd = VulkanTexture::begin_single_time_commands(device, app.command_pool);
            barrier(device, cmd);
            device.cmd_begin_render_pass(
                cmd,
                &vk::RenderPassBeginInfo::default()
                    .render_pass(fb.render_pass)
                    .framebuffer(fb.frames[target])
                    .render_area(vk::Rect2D::default().extent(fb.extent)),
                vk::SubpassContents::INLINE,
            );
            if let Some((color, alpha, depth)) = clear {
                let state = draws[0].state.unwrap().pipeline;
                let clip = draws[0].state.unwrap().clip;
                let (r, g, b, _) = color.to_rgba_f(color_format);
                let mut attachments = Vec::new();
                if state.color_mask == 15 {
                    attachments.push(
                        vk::ClearAttachment::default()
                            .aspect_mask(vk::ImageAspectFlags::COLOR)
                            .color_attachment(0)
                            .clear_value(vk::ClearValue {
                                color: vk::ClearColorValue {
                                    float32: [r, g, b, alpha.to_f32()],
                                },
                            }),
                    );
                }
                if depth_mask {
                    attachments.push(
                        vk::ClearAttachment::default()
                            .aspect_mask(vk::ImageAspectFlags::DEPTH)
                            .clear_value(vk::ClearValue {
                                depth_stencil: vk::ClearDepthStencilValue {
                                    depth: depth as f32 / 65535.0,
                                    stencil: 0,
                                },
                            }),
                    );
                }
                if !attachments.is_empty() && clip.extent.width > 0 && clip.extent.height > 0 {
                    device.cmd_clear_attachments(
                        cmd,
                        &attachments,
                        &[vk::ClearRect::default().rect(clip).layer_count(1)],
                    );
                }
            }
            if !vertices.is_empty() {
                device.cmd_set_viewport(
                    cmd,
                    0,
                    &[vk::Viewport::default()
                        .width(fb.extent.width as f32)
                        .height(fb.extent.height as f32)
                        .max_depth(1.0)],
                );
                device.cmd_bind_vertex_buffers(cmd, 0, &[app.vertex_buffers[0].buffer], &[0]);
                device.cmd_bind_descriptor_sets(
                    cmd,
                    vk::PipelineBindPoint::GRAPHICS,
                    app.pipeline_layout,
                    0,
                    &[app.texture_cache.get_descriptor_set()],
                    &[],
                );
                for draw in draws {
                    let DrawState {
                        pipeline,
                        push,
                        clip,
                    } = draw.state.unwrap();
                    if clip.extent.width == 0 || clip.extent.height == 0 {
                        continue;
                    }
                    device.cmd_bind_pipeline(
                        cmd,
                        vk::PipelineBindPoint::GRAPHICS,
                        app.state_pipelines[&pipeline],
                    );
                    device.cmd_set_scissor(cmd, 0, &[clip]);
                    let bytes = std::slice::from_raw_parts(
                        &push as *const _ as *const u8,
                        std::mem::size_of_val(&push),
                    );
                    device.cmd_push_constants(
                        cmd,
                        app.pipeline_layout,
                        vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                        0,
                        bytes,
                    );
                    device.cmd_draw(cmd, draw.vertex_count, 1, draw.start_vertex, 0);
                }
            }
            device.cmd_end_render_pass(cmd);
            barrier(device, cmd);
            VulkanTexture::end_single_time_commands(
                device,
                app.command_pool,
                app.graphics_queue,
                cmd,
            );
        }
    }

    pub(super) fn draw_fps_overlay(&mut self) {
        if self.config.render.fps == config::FpsPosition::Off || self.vulkan_app.is_none() {
            return;
        }
        self.fps_counter.frame_count += 1;
        let elapsed = self.fps_counter.last_time.elapsed().as_secs_f32();
        if elapsed >= 1.0 {
            self.fps_counter.display_fps = (self.fps_counter.frame_count as f32 / elapsed) as u32;
            self.fps_counter.frame_count = 0;
            self.fps_counter.last_time = std::time::Instant::now();
        }
        let digits = fps_digits(self.fps_counter.display_fps);
        let width = digits.len() as f32 * 8.0 + 4.0;
        let x = if matches!(
            self.config.render.fps,
            config::FpsPosition::TopRight | config::FpsPosition::BottomRight
        ) {
            self.state.screen_width as f32 - width - 2.0
        } else {
            2.0
        };
        let y = if matches!(
            self.config.render.fps,
            config::FpsPosition::BottomLeft | config::FpsPosition::BottomRight
        ) {
            self.state.screen_height as f32 - 16.0
        } else {
            2.0
        };
        let mut vertices = Vec::new();
        push_quad(
            &mut vertices,
            x,
            y,
            x + width,
            y + 14.0,
            [0.0, 0.0, 0.0, 0.6],
        );
        for (i, digit) in digits.into_iter().enumerate() {
            draw_7seg_digit(
                &mut vertices,
                x + 2.0 + i as f32 * 8.0,
                y + 2.0,
                6.0,
                10.0,
                digit,
                [0.0, 1.0, 0.0, 1.0],
            );
        }
        let state = PipelineState {
            blend: [
                vk::BlendFactor::SRC_ALPHA.as_raw(),
                vk::BlendFactor::ONE_MINUS_SRC_ALPHA.as_raw(),
                1,
                0,
            ],
            ..Default::default()
        };
        let mut push = self.push_constants();
        push.fog_mode = 0;
        push.alpha_test = CmpFnc::Always as u32;
        let old_buffer = self.state.render_buffer;
        self.state.render_buffer = Buffer::Back;
        self.render_vertices(
            &vertices,
            state,
            push,
            vk::Rect2D::default().extent(vk::Extent2D {
                width: self.state.screen_width,
                height: self.state.screen_height,
            }),
            None,
        );
        self.state.render_buffer = old_buffer;
    }

    pub(super) fn present_front(&mut self) -> Option<()> {
        #[cfg(target_os = "windows")]
        self.refresh_native_window_size();
        let mut push = self.push_constants();
        let gamma = api::current_gamma_rgb();
        push.gamma_r = gamma[0];
        push.gamma_g = gamma[1];
        push.gamma_b = gamma[2];
        let app = self.vulkan_app.as_mut()?;
        // The display branch uses this otherwise-unused word to mark an sRGB target.
        push.alpha_test = matches!(
            app.swapchain_format,
            vk::Format::R8G8B8A8_SRGB | vk::Format::B8G8R8A8_SRGB
        ) as u32;
        unsafe {
            app.device
                .device_wait_idle()
                .expect("Wait before presentation");
            let image = match app.swapchain_loader.acquire_next_image(
                app.swapchain,
                u64::MAX,
                app.image_available_semaphores[0],
                vk::Fence::null(),
            ) {
                Ok((image, suboptimal)) => {
                    self.is_framebuffer_resized |= suboptimal;
                    image
                }
                Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                    self.is_framebuffer_resized = true;
                    return Some(());
                }
                Err(error) => {
                    log::error!("Acquire failed: {error:?}");
                    return None;
                }
            };
            app.texture_cache
                .set_display_image(&app.device, app.framebuffer.as_ref()?.views[0]);
            let mut vertices = Vec::new();
            push_quad(
                &mut vertices,
                0.0,
                0.0,
                self.state.screen_width as f32,
                self.state.screen_height as f32,
                [1.0; 4],
            );
            for v in &mut vertices {
                v.combine_flags = 1 << 31;
                v.texture_index = utility::texture::DISPLAY_SLOT;
                v.tex_coord = [
                    v.position[0] / push.screen_size[0],
                    v.position[1] / push.screen_size[1],
                ];
            }
            let data = std::slice::from_raw_parts(
                vertices.as_ptr() as *const u8,
                std::mem::size_of_val(vertices.as_slice()),
            );
            utility::vertex::upload_to_buffer(&app.device, &app.vertex_buffers[0], data);
            let cmd = app.command_buffers[image as usize];
            app.device
                .reset_command_buffer(cmd, vk::CommandBufferResetFlags::empty())
                .expect("Reset present commands");
            app.device
                .begin_command_buffer(cmd, &vk::CommandBufferBeginInfo::default())
                .expect("Begin present commands");
            barrier(&app.device, cmd);
            let clears = [
                vk::ClearValue {
                    color: vk::ClearColorValue {
                        float32: [0.0, 0.0, 0.0, 1.0],
                    },
                },
                vk::ClearValue {
                    depth_stencil: vk::ClearDepthStencilValue {
                        depth: 1.0,
                        stencil: 0,
                    },
                },
            ];
            app.device.cmd_begin_render_pass(
                cmd,
                &vk::RenderPassBeginInfo::default()
                    .render_pass(app.render_pass)
                    .framebuffer(app.swapchain_framebuffers[image as usize])
                    .render_area(vk::Rect2D::default().extent(app.swapchain_extent))
                    .clear_values(&clears),
                vk::SubpassContents::INLINE,
            );
            app.device.cmd_bind_pipeline(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                app.graphics_pipelines[&utility::vertex::BlendMode::Opaque],
            );
            let s = &self.state;
            app.device.cmd_set_viewport(
                cmd,
                0,
                &[vk::Viewport::default()
                    .x(s.viewport_x as f32)
                    .y(s.viewport_y as f32)
                    .width(s.viewport_width as f32)
                    .height(s.viewport_height as f32)
                    .max_depth(1.0)],
            );
            app.device.cmd_set_scissor(
                cmd,
                0,
                &[vk::Rect2D::default().extent(app.swapchain_extent)],
            );
            app.device
                .cmd_bind_vertex_buffers(cmd, 0, &[app.vertex_buffers[0].buffer], &[0]);
            app.device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                app.pipeline_layout,
                0,
                &[app.texture_cache.get_descriptor_set()],
                &[],
            );
            let bytes = std::slice::from_raw_parts(
                &push as *const _ as *const u8,
                std::mem::size_of_val(&push),
            );
            app.device.cmd_push_constants(
                cmd,
                app.pipeline_layout,
                vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                0,
                bytes,
            );
            app.device.cmd_draw(cmd, 6, 1, 0, 0);
            app.device.cmd_end_render_pass(cmd);
            app.device
                .end_command_buffer(cmd)
                .expect("End presentation");
            let waits = [app.image_available_semaphores[0]];
            let stages = [vk::PipelineStageFlags::ALL_COMMANDS];
            let signals = [app.render_finished_semaphores[image as usize]];
            let commands = [cmd];
            app.device
                .queue_submit(
                    app.graphics_queue,
                    &[vk::SubmitInfo::default()
                        .wait_semaphores(&waits)
                        .wait_dst_stage_mask(&stages)
                        .command_buffers(&commands)
                        .signal_semaphores(&signals)],
                    vk::Fence::null(),
                )
                .expect("Submit presentation");
            let swapchains = [app.swapchain];
            let images = [image];
            match app.swapchain_loader.queue_present(
                app.present_queue,
                &vk::PresentInfoKHR::default()
                    .wait_semaphores(&signals)
                    .swapchains(&swapchains)
                    .image_indices(&images),
            ) {
                Ok(suboptimal) => self.is_framebuffer_resized |= suboptimal,
                Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => self.is_framebuffer_resized = true,
                Err(error) => {
                    log::error!("Presentation failed: {error:?}");
                    return None;
                }
            }
            app.last_presented_image_index = image;
        }
        Some(())
    }
}
