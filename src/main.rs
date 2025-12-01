use std::default::Default;
use image::{DynamicImage, GenericImageView, RgbaImage};
use std::env;
use std::path::Path;
use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::Window;

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

// Loads an image from the filesystem and returns it as a RGBA8 image
fn load_image(img_path: &String) -> RgbaImage {

    // Check if image exists in filesystem
    if !Path::new(img_path).exists() {
        eprintln!("Image not found: {}", img_path);
        std::process::exit(1);
    }

    // Load the image
    let img_dynamic = image::open(img_path).expect("Failed to load image");

    // Convert to RGBA8 (we need this for wgpu)
    img_dynamic.to_rgba8()
}

fn main() {

    // Load and store image
    let img = load_image(&parse_args());
    let (width, height) = (img.width(), img.height());

    // create an event loop
    let event_loop = EventLoop::new().expect("Failed to create event loop");

    // build our viewport with the image size in mind
    let window_attributes = Window::default_attributes()
        .with_title("Balatro Shader Simulation")
        .with_inner_size(winit::dpi::LogicalSize::new(width as f64, height as f64));
    
    let window = event_loop.create_window(window_attributes)
        .expect("Failed to create window");

    // create a gpu instance (this represents the direct connection to the hardware)
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::all(),
        ..Default::default()
    });

    // create a surface (this represents what we are drawing to, and will be the window we defined above)
    let surface = instance.create_surface(&window)
        .expect("Failed to create surface");

    // looks for a gpu that's compatible with our needs
    let adapter = pollster::block_on(
        instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        })
    ).expect("Failed to find an appropriate adapter");

    // create a device interface with the selected gpu
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
    let config = wgpu::SurfaceConfiguration {
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
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
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
        texture_size,
    );

    // create a sampler to tell the adapter how to handle the texture it's been given
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("image_sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });

    // describes what resources we want the shader to access by creating bindings (texture + sampler)
    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("texture_bind_group_layout"),
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
        ],
    });

    // tie the texture and sampler to the layout's bindings we defined above
    let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
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
        ],
        label: Some("texture_bind_group"),
    });


}