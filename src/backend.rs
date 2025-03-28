use crate::{
    frontend::SimData,
    utils::{vec2, RenderSpace, ScreenSpace, Vec2},
};
use log::{error, info, trace};
use std::time::Instant;
use wgpu::{CompositeAlphaMode, DeviceDescriptor};
use winit::window::Window;

pub struct Backend<'a> {
    // The window must be declared after the surface so
    // it gets dropped after it as the surface contains
    // unsafe references to the window's resources.
    pub window: &'a Window,
    window_size: Vec2<u32, ScreenSpace>,
    surface: wgpu::Surface<'a>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    texture: wgpu::Texture,
    bind_group_layout: wgpu::BindGroupLayout,
    render_pipeline: wgpu::RenderPipeline,
    gpu_uniforms: GpuUniforms,
    gpu_data_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    sampler: wgpu::Sampler,
}

// Data to pass to gpu, MUST have 16 byte alignment
#[repr(C)]
#[derive(Copy, Clone)]
pub struct GpuUniforms {
    pub padding: [f32; 3],
    pub time: f32,
    pub texture_size: [f32; 2],
    pub window_size: [f32; 2],
}

unsafe impl bytemuck::Zeroable for GpuUniforms {}
unsafe impl bytemuck::Pod for GpuUniforms {}

impl<'a> Backend<'a> {
    pub fn render(&mut self, sim_data: &SimData, start: Instant) {
        optick::event!("Backend::render");

        let frame = match self.surface.get_current_texture() {
            Ok(frame) => frame,
            // can't gracefully exit in oom states
            Err(wgpu::SurfaceError::OutOfMemory) => std::process::exit(0),
            Err(wgpu::SurfaceError::Lost) => {
                self.resize(self.window_size, sim_data);
                // TODO(TOM): logging the error, but not handling it.
                error!("SurfaceError::Lost, cannot resize simulation in this scope. fix this tom!");
                return;
            }
            Err(e) => {
                error!("{e:#?}");
                return;
            }
        };

        // Creates necessary metadata of the texture for the render pass.
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        // Creates the GPU commands. Most graphics frameworks expect commands
        // to be stored in a command buffer before being sent to the GPU.
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
        });
        render_pass.set_pipeline(&self.render_pipeline);
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        trace!("Bound items to render pass");

        {
            optick::event!("Update gpu uniforms");
            // Writing new time value to a GPU buffer, for shader code to access!
            self.gpu_uniforms.time = start.elapsed().as_millis_f32();
            self.queue.write_buffer(
                &self.gpu_data_buffer,
                0, // the entire uniform buffer is updated.
                bytemuck::cast_slice(&[self.gpu_uniforms]),
            );
        }

        {
            optick::event!("Update texture && draw");
            Self::update_texture(&self.queue, &self.texture, sim_data);

            // Takes 6 vertices (2 triangles = 1 square) and the vertex & fragment shader
            render_pass.draw(0..6, 0..1);
        }
        // Drop render_pass' mutable reference to encoder, crashes otherwise.
        drop(render_pass);

        {
            optick::event!("Submitted render pass");
            self.queue.submit(std::iter::once(encoder.finish()));
            frame.present();
        }
    }

    pub fn resize(&mut self, window_size: Vec2<u32, ScreenSpace>, sim_data: &SimData) {
        optick::event!("Backend::resize");

        trace!("Attempting window & texture resize to {:?}", sim_data.size);

        self.window_size = window_size;
        self.config.width = self.window_size.x;
        self.config.height = self.window_size.y;
        self.surface.configure(&self.device, &self.config);

        self.resize_texture(sim_data);
    }

    pub fn resize_texture(&mut self, sim_data: &SimData) {
        // create new texture
        self.texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("RGBA Texture"),
            size: wgpu::Extent3d {
                width: sim_data.size.x,
                height: sim_data.size.y,
                depth_or_array_layers: 1, // set to 1 for 2D textures
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.config.format, // SRGB (3 bpp)
            // TEXTURE_BINDING tells wgpu that we want to use this texture in shaders
            // COPY_DST means that we want to copy data to this texture
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            // This specifies other texture formats that can be used to create TextureViews.
            // not supported on the WebGL2 backend.
            view_formats: &[],
        });

        // update gpu data, explicity naming every field to throw compile errors on new fields
        self.gpu_uniforms = GpuUniforms {
            padding: self.gpu_uniforms.padding,
            time: self.gpu_uniforms.time,
            texture_size: sim_data.size.cast().to_array(),
            window_size: self.window_size.cast().to_array(),
        };

        // update binding group
        let texture_view = self
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        self.bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bind_group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.gpu_data_buffer.as_entire_binding(),
                },
            ],
        });

        Self::update_texture(&self.queue, &self.texture, sim_data);
    }

    fn update_texture(queue: &wgpu::Queue, texture: &wgpu::Texture, sim_data: &SimData) {
        let tex_size = texture.size();
        let computed_data_len = (4 * sim_data.size.x * sim_data.size.y) as usize;

        assert_eq!(tex_size.width, sim_data.size.x);
        assert_eq!(tex_size.height, sim_data.size.y);
        assert_eq!(sim_data.buf.len(), computed_data_len, "{sim_data:#?}");

        queue.write_texture(
            wgpu::ImageCopyTexture {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            sim_data.buf,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(4 * sim_data.size.x),
                rows_per_image: Some(sim_data.size.y),
            },
            wgpu::Extent3d {
                width: sim_data.size.x,
                height: sim_data.size.y,
                depth_or_array_layers: 1,
            },
        );
    }

    async fn create_surface(
        instance: &wgpu::Instance,
        window: &'a Window,
    ) -> (
        wgpu::Surface<'a>,
        wgpu::Device,
        wgpu::Queue,
        wgpu::SurfaceConfiguration,
        Vec2<u32, ScreenSpace>,
    ) {
        let surface = instance.create_surface(window).unwrap();
        info!("Surface created");

        // >> Requesting Adapter (gpu abstraction) <<
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .unwrap();
        info!("Adapter created");

        // >> Creating Device and Queue <<
        let (device, queue) = adapter
            .request_device(&DeviceDescriptor::default(), None)
            .await
            .unwrap();
        info!("Device and Queue created");

        // >> Creating Surface Config <<
        let window_size = window.inner_size();
        let window_size = vec2(window_size.width, window_size.height);

        let capabilities = surface.get_capabilities(&adapter);
        let surface_format = capabilities
            .formats
            .iter()
            .find(|x| **x == wgpu::TextureFormat::Rgba8Unorm)
            .copied()
            .unwrap_or(capabilities.formats[0]);
        assert_eq!(surface_format, wgpu::TextureFormat::Rgba8Unorm);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: window_size.x,
            height: window_size.y,
            present_mode: wgpu::PresentMode::Immediate, // Immediate = no vsync, Fifo = vsync
            desired_maximum_frame_latency: 0,
            alpha_mode: CompositeAlphaMode::default(),
            view_formats: Vec::new(),
        };
        surface.configure(&device, &config);
        info!("Surface configured with format '{surface_format:?}', {window_size:?}");

        (surface, device, queue, config, window_size)
    }

    fn create_texture(
        sim_data: &SimData,
        queue: &wgpu::Queue,
        device: &wgpu::Device,
        config: &wgpu::SurfaceConfiguration,
    ) -> wgpu::Texture {
        // Loading an image.
        // let bytes = include_bytes!("patSilhouette.png");
        // let image = image::load_from_memory(bytes).unwrap();
        // let image_size = image::GenericImageView::dimensions(&image);
        // let texture_data = image.to_rgba8().into_raw();
        // info!("Image loaded with size {image_size:?}");

        // >> Creating Texture <<
        let texture_size = wgpu::Extent3d {
            width: sim_data.size.x,
            height: sim_data.size.y,
            depth_or_array_layers: 1, // set to 1 for 2D textures
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("RGBA Texture"),
            size: texture_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: config.format, // SRGB (3 bpp)
            // TEXTURE_BINDING tells wgpu that we want to use this texture in shaders
            // COPY_DST means that we want to copy data to this texture
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            // This specifies other texture formats that can be used to create TextureViews.
            // not supported on the WebGL2 backend.
            view_formats: &[],
        });
        Self::update_texture(queue, &texture, sim_data);
        info!("Texture created, size: {:?}", texture.size());

        texture
    }

    fn create_render_pipeline(
        device: &wgpu::Device,
        config: &wgpu::SurfaceConfiguration,
    ) -> (wgpu::RenderPipeline, wgpu::BindGroupLayout) {
        // >> Creating bind group layout <<
        let shader = device.create_shader_module(wgpu::include_wgsl!("shader.wgsl"));
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bind_group_layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    // This should match the filterable field of the
                    // corresponding Texture entry above.
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        // >> Creating Render Pipeline <<
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList, // 1.
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw, // 2.
                cull_mode: Some(wgpu::Face::Back),
                // Setting this to anything other than Fill requires Features::NON_FILL_POLYGON_MODE
                polygon_mode: wgpu::PolygonMode::Fill,
                // Requires Features::DEPTH_CLIP_CONTROL
                unclipped_depth: false,
                // Requires Features::CONSERVATIVE_RASTERIZATION
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });
        info!("Render Pipeline created");

        (render_pipeline, bind_group_layout)
    }

    fn create_gpu_uniforms(
        device: &wgpu::Device,
        texture_size: Vec2<u32, RenderSpace>,
        window_size: Vec2<u32, ScreenSpace>,
    ) -> (GpuUniforms, wgpu::Buffer) {
        // Create a GPU buffer to hold time values, for shader code!
        let gpu_uniforms = GpuUniforms {
            padding: [0.0; 3],
            time: 0.0,
            texture_size: texture_size.cast().to_array(),
            window_size: window_size.cast().to_array(),
        };
        let gpu_data_buffer = wgpu::util::DeviceExt::create_buffer_init(
            device,
            &wgpu::util::BufferInitDescriptor {
                label: Some("Uniform Buffer"),
                contents: bytemuck::cast_slice(&[gpu_uniforms]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            },
        );
        info!("Uniform Buffer created");

        (gpu_uniforms, gpu_data_buffer)
    }

    fn create_bind_group(
        device: &wgpu::Device,
        bind_group_layout: &wgpu::BindGroupLayout,
        texture: &wgpu::Texture,
        gpu_data_buffer: &wgpu::Buffer,
    ) -> (wgpu::BindGroup, wgpu::Sampler) {
        // >> Creating Bind Group <<
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bind_group"),
            layout: bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: gpu_data_buffer.as_entire_binding(),
                },
            ],
        });
        info!("Bind Group created");

        (bind_group, sampler)
    }

    pub async fn new(window: &'a Window, sim_data: SimData<'_>) -> Self {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            // TODO(TOM): if wasm, use GL.
            ..Default::default()
        });
        info!("Instance created");

        let (surface, device, queue, config, window_size) =
            Self::create_surface(&instance, window).await;

        let texture = Self::create_texture(&sim_data, &queue, &device, &config);

        let (render_pipeline, bind_group_layout) = Self::create_render_pipeline(&device, &config);

        let (gpu_uniforms, gpu_data_buffer) =
            Self::create_gpu_uniforms(&device, sim_data.size, window_size);

        let (bind_group, sampler) =
            Self::create_bind_group(&device, &bind_group_layout, &texture, &gpu_data_buffer);

        Self {
            window,
            window_size,
            surface,
            device,
            queue,
            config,
            texture,
            bind_group_layout,
            render_pipeline,
            gpu_uniforms,
            gpu_data_buffer,
            bind_group,
            sampler,
        }
    }
}
