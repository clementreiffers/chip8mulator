use std::sync::Arc;

use chip8_engine::{DISPLAY_HEIGHT, DISPLAY_WIDTH};
use wgpu::util::DeviceExt;
use winit::{dpi::PhysicalSize, window::Window};

const SHADER: &str = r#"
struct Viewport { size: vec2<f32>, _padding: vec2<f32> };
@group(0) @binding(0) var pixels: texture_2d<f32>;
@group(0) @binding(1) var pixel_sampler: sampler;
@group(0) @binding(2) var<uniform> viewport: Viewport;

struct VertexOutput { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32> };

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> VertexOutput {
    var positions = array<vec2<f32>, 3>(vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0));
    var output: VertexOutput;
    output.position = vec4(positions[index], 0.0, 1.0);
    output.uv = positions[index] * vec2(0.5, -0.5) + vec2(0.5);
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    var uv = input.uv;
    let aspect = viewport.size.x / viewport.size.y;
    let chip8_aspect = 2.0;
    if (aspect > chip8_aspect) {
        let used_width = chip8_aspect / aspect;
        uv.x = (uv.x - (1.0 - used_width) * 0.5) / used_width;
    } else {
        let used_height = aspect / chip8_aspect;
        uv.y = (uv.y - (1.0 - used_height) * 0.5) / used_height;
    }
    if (any(uv < vec2(0.0)) || any(uv > vec2(1.0))) { return vec4(0.006, 0.009, 0.014, 1.0); }
    let lit = textureSample(pixels, pixel_sampler, uv).r;
    let off = vec3(0.008, 0.014, 0.020);
    let on = vec3(0.20, 1.0, 0.73);
    return vec4(mix(off, on, lit), 1.0);
}
"#;

pub struct Renderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    size: PhysicalSize<u32>,
    pixel_texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
    pipeline: wgpu::RenderPipeline,
    viewport_buffer: wgpu::Buffer,
}

impl Renderer {
    pub async fn new(window: Arc<Window>) -> Result<Self, Box<dyn std::error::Error>> {
        let size = window.inner_size();
        let instance = wgpu::Instance::default();
        let surface = instance.create_surface(window)?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .ok_or("no compatible GPU adapter found")?;
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("CHIP-8 GPU device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                },
                None,
            )
            .await?;
        let limits = device.limits();
        let capabilities = surface.get_capabilities(&adapter);
        let format = capabilities
            .formats
            .iter()
            .copied()
            .find(wgpu::TextureFormat::is_srgb)
            .unwrap_or(capabilities.formats[0]);
        let present_mode = capabilities
            .present_modes
            .iter()
            .copied()
            .find(|mode| *mode == wgpu::PresentMode::Mailbox)
            .unwrap_or(wgpu::PresentMode::Fifo);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1).min(limits.max_texture_dimension_2d),
            height: size.height.max(1).min(limits.max_texture_dimension_2d),
            present_mode,
            alpha_mode: capabilities.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let pixel_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("CHIP-8 framebuffer"),
            size: wgpu::Extent3d {
                width: DISPLAY_WIDTH as u32,
                height: DISPLAY_HEIGHT as u32,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let texture_view = pixel_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("nearest-neighbor CHIP-8 sampler"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let viewport_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("viewport uniform"),
            contents: &viewport_bytes(size),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("CHIP-8 bind group layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
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
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("CHIP-8 bind group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: viewport_buffer.as_entire_binding(),
                },
            ],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("CHIP-8 shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("CHIP-8 pipeline layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("CHIP-8 render pipeline"),
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
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });
        Ok(Self {
            surface,
            device,
            queue,
            config,
            size,
            pixel_texture,
            bind_group,
            pipeline,
            viewport_buffer,
        })
    }

    pub fn resize(&mut self, size: PhysicalSize<u32>) {
        if size.width == 0 || size.height == 0 {
            return;
        }
        let limits = self.device.limits();
        self.size = size;
        self.config.width = size.width.min(limits.max_texture_dimension_2d);
        self.config.height = size.height.min(limits.max_texture_dimension_2d);
        self.reconfigure();
        self.queue
            .write_buffer(&self.viewport_buffer, 0, &viewport_bytes(size));
    }

    pub fn reconfigure(&self) {
        self.surface.configure(&self.device, &self.config);
    }

    pub fn render(
        &mut self,
        framebuffer: &[u8],
        upload_frame: bool,
    ) -> Result<(), wgpu::SurfaceError> {
        if upload_frame {
            self.queue.write_texture(
                wgpu::ImageCopyTexture {
                    texture: &self.pixel_texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                framebuffer,
                wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(DISPLAY_WIDTH as u32),
                    rows_per_image: Some(DISPLAY_HEIGHT as u32),
                },
                wgpu::Extent3d {
                    width: DISPLAY_WIDTH as u32,
                    height: DISPLAY_HEIGHT as u32,
                    depth_or_array_layers: 1,
                },
            );
        }
        let frame = self.surface.get_current_texture()?;
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("CHIP-8 render encoder"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("CHIP-8 render pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
        self.queue.submit(Some(encoder.finish()));
        frame.present();
        Ok(())
    }
}

fn viewport_bytes(size: PhysicalSize<u32>) -> [u8; 16] {
    let values = [size.width as f32, size.height as f32, 0.0, 0.0];
    bytemuck::cast(values)
}
