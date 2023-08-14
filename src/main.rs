#![feature(type_name_of_val)]
use std::{mem, slice};
use std::collections::HashMap;
use winit::window::WindowBuilder;
use winit::event_loop::{EventLoop, ControlFlow};
use winit::event::{Event, WindowEvent};
use winit::dpi::PhysicalSize;
use wgpu::util::DeviceExt;
use log::LevelFilter;
use glam::{Vec2, Vec3};
use fontdue::{Font, FontSettings};
use fontdue::layout::{Layout, CoordinateSystem, TextStyle};
use etagere::{BucketedAtlasAllocator, Size};
use shared::{Vertex, Consts};

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8UnormSrgb;

fn main() -> Result {
  env_logger::builder().filter_level(LevelFilter::Info).init();
  let event_loop = EventLoop::new();
  let window = WindowBuilder::new().build(&event_loop)?;

  let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::default());
  let surface = unsafe { instance.create_surface(&window)? };
  let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
    power_preference: wgpu::PowerPreference::HighPerformance,
    compatible_surface: Some(&surface),
    force_fallback_adapter: false,
  }))
  .unwrap();
  let (device, queue) = pollster::block_on(adapter.request_device(
    &wgpu::DeviceDescriptor {
      features: wgpu::Features::PUSH_CONSTANTS,
      limits: wgpu::Limits {
        max_push_constant_size: 128,
        ..Default::default()
      },
      label: None,
    },
    None,
  ))?;
  resize(&surface, &device, window.inner_size());

  let shader = device.create_shader_module(wgpu::include_spirv!(env!("shaders.spv")));
  let tex_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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
    label: None,
  });
  let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
    label: None,
    bind_group_layouts: &[&tex_layout],
    push_constant_ranges: &[wgpu::PushConstantRange {
      stages: wgpu::ShaderStages::VERTEX,
      range: 0..mem::size_of::<Consts>() as _,
    }],
  });
  let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
    layout: Some(&pipeline_layout),
    vertex: wgpu::VertexState {
      module: &shader,
      entry_point: "main_v",
      buffers: &[wgpu::VertexBufferLayout {
        array_stride: mem::size_of::<Vertex>() as _,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x3],
      }],
    },
    fragment: Some(wgpu::FragmentState {
      module: &shader,
      entry_point: "main_f",
      targets: &[Some(wgpu::ColorTargetState {
        format: FORMAT,
        blend: Some(wgpu::BlendState::REPLACE),
        write_mask: wgpu::ColorWrites::ALL,
      })],
    }),
    primitive: wgpu::PrimitiveState::default(),
    depth_stencil: None,
    multisample: wgpu::MultisampleState::default(),
    multiview: None,
    label: None,
  });
  let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
    address_mode_u: wgpu::AddressMode::ClampToEdge,
    address_mode_v: wgpu::AddressMode::ClampToEdge,
    address_mode_w: wgpu::AddressMode::ClampToEdge,
    mag_filter: wgpu::FilterMode::Linear,
    min_filter: wgpu::FilterMode::Nearest,
    mipmap_filter: wgpu::FilterMode::Nearest,
    ..Default::default()
  });

  let font_size = 48.0;
  let font = Font::from_bytes(include_bytes!("roboto.ttf") as &_, FontSettings::default())?;
  let mut packer = BucketedAtlasAllocator::new(Size::new(256, 256));
  let mut tex_data = vec![[0; 4]; packer.size().area() as _];
  packer.allocate(Size::new(2, 2));
  tex_data[0] = [255; 4];
  let glyphs: HashMap<_, (Vec2, Vec2)> = font
    .chars()
    .iter()
    .filter_map(|(c, i)| {
      let metrics = font.metrics_indexed(i.get(), font_size);
      let size = Size::new(metrics.width as _, metrics.height as _);
      if size.is_empty() {
        None
      } else {
        Some(match packer.allocate(size) {
          Some(a) => (c, (i, a)),
          None => {
            tex_data.extend(vec![[0; 4]; (packer.size().area() * 3) as _]);
            packer.grow(packer.size() * 2);
            match packer.allocate(size) {
              Some(a) => (c, (i, a)),
              None => panic!("its over"),
            }
          }
        })
      }
    })
    .collect::<HashMap<_, _>>()
    .into_iter()
    .map(|(c, (i, a))| {
      let (metrics, raster) = font.rasterize_indexed(i.get(), font_size);
      let pos = a.rectangle.min;
      let width = packer.size().width as usize;
      for y in 0..metrics.height {
        for x in 0..metrics.width {
          let px = raster[y * metrics.width + x];
          tex_data[(y + pos.y as usize) * width + x + pos.x as usize] = [px, px, px, 255];
        }
      }
      let min = pos.to_f32() / width as f32;
      let max = (pos + Size::new(metrics.width as _, metrics.height as _)).to_f32() / width as f32;
      (c, (min.to_array().into(), max.to_array().into()))
    })
    .collect();
  packer.dump_svg(&mut std::fs::File::create("test.svg")?);
  let size = wgpu::Extent3d {
    width: packer.size().width as _,
    height: packer.size().height as _,
    depth_or_array_layers: 1,
  };
  let tex = device.create_texture(&wgpu::TextureDescriptor {
    size,
    mip_level_count: 1,
    sample_count: 1,
    dimension: wgpu::TextureDimension::D2,
    format: wgpu::TextureFormat::Rgba8Unorm,
    usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
    view_formats: &[],
    label: None,
  });
  let tex_view = tex.create_view(&wgpu::TextureViewDescriptor::default());
  queue.write_texture(
    wgpu::ImageCopyTexture {
      texture: &tex,
      mip_level: 0,
      origin: wgpu::Origin3d::ZERO,
      aspect: wgpu::TextureAspect::All,
    },
    cast_slice(&tex_data),
    wgpu::ImageDataLayout {
      offset: 0,
      bytes_per_row: Some(4 * size.width),
      rows_per_image: Some(size.height),
    },
    size,
  );
  let tex_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
    layout: &tex_layout,
    entries: &[
      wgpu::BindGroupEntry {
        binding: 0,
        resource: wgpu::BindingResource::TextureView(&tex_view),
      },
      wgpu::BindGroupEntry {
        binding: 1,
        resource: wgpu::BindingResource::Sampler(&sampler),
      },
    ],
    label: None,
  });

  let mut layout = Layout::new(CoordinateSystem::PositiveYDown);
  layout.append(
    &[&font],
    &TextStyle::new("the quick brown floppa", font_size, 0),
  );

  let mut verts = vec![];
  let mut indices = vec![];

  for g in layout.glyphs() {
    if let Some((min, max)) = glyphs.get(&g.parent) {
      indices.extend([0, 1, 2, 1, 3, 2].map(|l| l + verts.len() as u32));
      verts.extend([
        Vertex {
          pos: Vec2::new(g.x, g.y),
          uv: Vec2::new(min.x, min.y),
          color: Vec3::ONE,
        },
        Vertex {
          pos: Vec2::new(g.x + g.width as f32, g.y),
          uv: Vec2::new(max.x, min.y),
          color: Vec3::ONE,
        },
        Vertex {
          pos: Vec2::new(g.x, g.y + g.height as f32),
          uv: Vec2::new(min.x, max.y),
          color: Vec3::ONE,
        },
        Vertex {
          pos: Vec2::new(g.x + g.width as f32, g.y + g.height as f32),
          uv: Vec2::new(max.x, max.y),
          color: Vec3::ONE,
        },
      ]);
    }
  }

  indices.extend([0, 1, 2].map(|l| l + verts.len() as u32));
  verts.extend([
    Vertex {
      pos: Vec2::new(100.0, 100.0),
      uv: Vec2::new(0.0, 0.0),
      color: Vec3::X,
    },
    Vertex {
      pos: Vec2::new(100.0, 200.0),
      uv: Vec2::new(0.0, 0.0),
      color: Vec3::Y,
    },
    Vertex {
      pos: Vec2::new(200.0, 200.0),
      uv: Vec2::new(0.0, 0.0),
      color: Vec3::Z,
    },
  ]);

  let vert_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
    contents: cast_slice(&verts),
    usage: wgpu::BufferUsages::VERTEX,
    label: None,
  });
  let idx_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
    contents: cast_slice(&indices),
    usage: wgpu::BufferUsages::INDEX,
    label: None,
  });

  event_loop.run(move |event, _, control_flow| match event {
    Event::WindowEvent { event, .. } => match event {
      WindowEvent::Resized(size) => resize(&surface, &device, size),
      WindowEvent::CloseRequested => *control_flow = ControlFlow::Exit,
      _ => {}
    },
    Event::RedrawRequested(..) => {
      let surface = surface.get_current_texture().unwrap();
      let surface_view = surface
        .texture
        .create_view(&wgpu::TextureViewDescriptor::default());
      let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());

      let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
          view: &surface_view,
          resolve_target: None,
          ops: wgpu::Operations {
            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
            store: true,
          },
        })],
        depth_stencil_attachment: None,
        label: None,
      });
      render_pass.set_pipeline(&pipeline);
      render_pass.set_push_constants(
        wgpu::ShaderStages::VERTEX,
        0,
        cast(&Consts {
          screen_size: Vec2::new(surface.texture.width() as _, surface.texture.height() as _),
        }),
      );
      render_pass.set_bind_group(0, &tex_bind_group, &[]);
      render_pass.set_vertex_buffer(0, vert_buf.slice(..));
      render_pass.set_index_buffer(idx_buf.slice(..), wgpu::IndexFormat::Uint32);
      render_pass.draw_indexed(0..indices.len() as _, 0, 0..1);
      drop(render_pass);

      queue.submit([encoder.finish()]);
      surface.present();
    }
    Event::MainEventsCleared => window.request_redraw(),
    _ => {}
  });
}

fn resize(surface: &wgpu::Surface, device: &wgpu::Device, size: PhysicalSize<u32>) {
  surface.configure(
    device,
    &wgpu::SurfaceConfiguration {
      usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
      format: FORMAT,
      width: size.width,
      height: size.height,
      present_mode: wgpu::PresentMode::Fifo,
      alpha_mode: wgpu::CompositeAlphaMode::Auto,
      view_formats: vec![],
    },
  );
}

fn cast_slice<T>(t: &[T]) -> &[u8] {
  unsafe { slice::from_raw_parts(t.as_ptr() as _, mem::size_of_val(t)) }
}

fn cast<T>(t: &T) -> &[u8] {
  cast_slice(slice::from_ref(t))
}
