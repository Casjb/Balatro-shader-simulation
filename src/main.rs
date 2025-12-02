use std::default::Default;
use std::sync::Arc;
use image::{DynamicImage, GenericImageView, RgbaImage};
use std::env;
use std::path::Path;
use std::sync::mpsc::channel;
use std::time::Duration;
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use wgpu::Texture;
use wgpu::util::DeviceExt;
use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::Window;

// write a new texture to the queue
fn write_texture(queue: &wgpu::Queue, texture: &Texture, img_path: &String, height: u32, width: u32) {
    match load_image(&img_path) {
        Ok(img) => {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &img.into_raw(),
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(4 * width),
                    rows_per_image: Some(height),
                },
                wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            );
        },
        Err(e) => eprintln!("Failed to load image: {}", e),
    }
}

// Parse command line arguments to return an image path
fn parse_args() -> String {

    // Parse command line arguments
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Image path required.");
        std::process::exit(1);
    }

    // Get image path from second arg
    args[1].clone()
}

// Open a file dialog using rfd
fn pick_image_file() -> String {
    let file = rfd::FileDialog::new()
        .set_title("Select an image")
        .add_filter("Image", &["png", "jpg", "jpeg", "bmp"])
        .pick_file();

    match file {
        Some(path) => path.to_string_lossy().to_string(),
        None => {
            eprintln!("No file selected. Exiting.");
            std::process::exit(1);
        }
    }
}

fn load_image(img_path: &str) -> Result<RgbaImage, image::ImageError> {
    if !Path::new(img_path).exists() {
        return Err(image::ImageError::IoError(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("File not found: {}", img_path),
        )));
    }

    let img_dynamic = image::open(img_path)?;
    Ok(img_dynamic.to_rgba8())
}

fn main() {

    // Load and store image
    let img_path = pick_image_file();
    let img = load_image(&img_path).expect("Failed to load image");
    let (width, height) = (img.width(), img.height());

    // create an event loop
    let event_loop = EventLoop::new().expect("Failed to create event loop");

    // create a channel to watch for changes to image file
    let (tx, rx) = channel();

    // create a watcher for the channel
    let mut watcher: RecommendedWatcher =
        Watcher::new(tx, Config::default()).expect("Failed to create watcher");

    // start watching file
    watcher.watch((&img_path).as_ref(), RecursiveMode::NonRecursive)
        .expect("Failed to watch file");

    // build our viewport with the image size in mind
    let window_attributes = Window::default_attributes()
        .with_title("Balatro Shader Simulation")
        .with_inner_size(winit::dpi::LogicalSize::new(width as f64, height as f64));
    
    let window = event_loop.create_window(window_attributes)
        .expect("Failed to create window");
    let window = Arc::new(window);

    // create a gpu instance (this represents the direct connection to the hardware)
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::all(),
        ..Default::default()
    });

    // create a surface (this represents what we are drawing to, and will be the window we defined above)
    let surface = instance.create_surface(window.clone())
        .expect("Failed to create surface");

    // looks for a gpu that's compatible with our needs
    let adapter = pollster::block_on(
        instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        })
    ).expect("Failed to find an appropriate adapter");

    // create a device interface and queue for the selected gpu
    let (device, queue) = pollster::block_on(
        adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: None,
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                experimental_features: Default::default(),
                memory_hints: Default::default(),
                trace: Default::default(),
            },
        )
    ).expect("Failed to create device");

    // select a supported surface format and alpha mode (just pick the first one if there are multiple)
    let caps = surface.get_capabilities(&adapter);
    let surface_format = caps.formats[0];
    let surface_alpha_mode = caps.alpha_modes[0];

    // configure the surface to the chosen device
    let mut config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format: surface_format,
        width,
        height,
        present_mode: wgpu::PresentMode::Fifo,
        desired_maximum_frame_latency: 0,
        alpha_mode: surface_alpha_mode,
        view_formats: Default::default(),
    };
    surface.configure(&device, &config);

    // create our image texture ready to be rendered
    let texture_size = wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        size: texture_size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        label: Some("image_texture"),
        view_formats: Default::default(),
    });

    // write this texture to our device
    write_texture(&queue, &texture, &img_path, height, width);

    // create a sampler to tell the adapter how to handle the texture it's been given
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("image_sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });

    // define params struct
    #[repr(C)]
    #[derive(Copy, Clone, bytemuck::Zeroable, bytemuck::Pod)]
    struct Params {
        time: f32,
        artifact_amplifier: f32,
        crt_amount_adjusted: f32,
        bloom_fac: f32,
    }

    // create a buffer to store our params in
    let params = Params {
        time: 0.0,
        artifact_amplifier: 1.0,
        crt_amount_adjusted: 1.0,
        bloom_fac: 1.0,
    };
    let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Params Buffer"),
        contents: bytemuck::bytes_of(&params),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });

    // describes what resources we want the shader to access by creating bindings
    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("texture_bind_group_layout"),
        entries: &[
            // binding 0: texture
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

            // binding 1: sampler
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },

            // binding 2: uniform buffer (Params)
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(
                        std::mem::size_of::<Params>() as _
                    ),
                },
                count: None,
            }
        ],
    });

    // tie the texture and sampler to the layout's bindings we defined above
    let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("texture_bind_group"),
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
                resource: uniform_buffer.as_entire_binding(),
            },
        ],
    });

    // define vertex data for a quad
    #[repr(C)]
    #[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
    struct Vertex {
        position: [f32; 2],
        uv: [f32; 2],
    }
    let vertices = [
        Vertex { position: [-1.0, -1.0], uv: [0.0, 1.0] },
        Vertex { position: [ 1.0, -1.0], uv: [1.0, 1.0] },
        Vertex { position: [ 1.0,  1.0], uv: [1.0, 0.0] },
        Vertex { position: [-1.0,  1.0], uv: [0.0, 0.0] },
    ];
    let indices: &[u16] = &[0, 1, 2, 2, 3, 0];
    let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Vertex Buffer"),
        contents: bytemuck::cast_slice(&vertices),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Index Buffer"),
        contents: bytemuck::cast_slice(indices),
        usage: wgpu::BufferUsages::INDEX,
    });

    let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/shaders.wgsl").into()),
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Pipeline Layout"),
        bind_group_layouts: &[&bind_group_layout],
        push_constant_ranges: &[],
    });

    let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Render Pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader_module,
            entry_point: Option::from("vs_main"),
            compilation_options: Default::default(),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &[
                    wgpu::VertexAttribute {
                        offset: 0,
                        shader_location: 0,
                        format: wgpu::VertexFormat::Float32x2,
                    },
                    wgpu::VertexAttribute {
                        offset: 8,
                        shader_location: 1,
                        format: wgpu::VertexFormat::Float32x2,
                    },
                ],
            }],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader_module,
            entry_point: Option::from("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: surface_format,
                blend: Some(wgpu::BlendState::REPLACE),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
        cache: None,
    });

    // main loop
    event_loop.run(move |event, event_loop_window_target| {
        event_loop_window_target.set_control_flow(ControlFlow::Poll);

        match event {
            Event::WindowEvent { event, window_id } if window_id == window.id() => {

                // receive file change event from watcher
                if let Ok(msg) = rx.try_recv() {
                    write_texture(&queue, &texture, &img_path, height, width);
                    window.request_redraw();
                    println!("File change received: {:?}", msg);
                }

                match event {
                    WindowEvent::Resized(physical_size) => {
                        let width = physical_size.width.max(1);
                        let height = physical_size.height.max(1);
                        
                        config.width = width;
                        config.height = height;
                        surface.configure(&device, &config);
                        
                        window.request_redraw();
                    }
                    WindowEvent::CloseRequested => {
                        event_loop_window_target.exit();
                    }
                    WindowEvent::RedrawRequested => {
                        // Get the current surface texture
                        let frame = surface
                            .get_current_texture()
                            .expect("Failed to acquire next swap chain texture");
                        let view = frame
                            .texture
                            .create_view(&wgpu::TextureViewDescriptor::default());
                        
                        // Create a command encoder
                        let mut encoder = device.create_command_encoder(
                            &wgpu::CommandEncoderDescriptor {
                                label: Some("Render Encoder"),
                            }
                        );
                        
                        // Begin render pass
                        {
                            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                                label: Some("Render Pass"),
                                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                    view: &view,
                                    depth_slice: None,
                                    resolve_target: None,
                                    ops: wgpu::Operations {
                                        load: wgpu::LoadOp::Clear(wgpu::Color {
                                            r: 0.1,
                                            g: 0.2,
                                            b: 0.3,
                                            a: 1.0,
                                        }),
                                        store: wgpu::StoreOp::Store,
                                    },
                                })],
                                depth_stencil_attachment: None,
                                timestamp_writes: None,
                                occlusion_query_set: None,
                            });
                            
                            render_pass.set_pipeline(&render_pipeline);
                            render_pass.set_bind_group(0, &bind_group, &[]);
                            render_pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                            render_pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint16);
                            render_pass.draw_indexed(0..6, 0, 0..1);
                        }
                        
                        // Submit command buffer
                        queue.submit(std::iter::once(encoder.finish()));
                        frame.present();
                    }
                    _ => {}
                }
            }
            Event::AboutToWait => {
                // Request redraw each frame
                window.request_redraw();
            }
            _ => {}
        }
    }).expect("Event loop error");
}