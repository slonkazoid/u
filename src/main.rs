#![feature(array_chunks)]
use std::{mem, slice};
use winit::window::WindowBuilder;
use winit::event_loop::EventLoop;
use winit::event::{Event, WindowEvent};
use wgpu::util::DeviceExt;
use log::LevelFilter;
use glam::{Vec2, Vec3};
use obj::{load_obj, Obj};
use shared::Consts;

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

fn main() -> Result {
  env_logger::builder()
    .filter_level(LevelFilter::Info)
    .filter(Some("wgpu_core"), LevelFilter::Warn)
    .filter(Some("wgpu_hal"), LevelFilter::Warn)
    .init();
  std::panic::set_hook(Box::new(|i| log::error!("{}", i)));
  let event_loop = EventLoop::new()?;
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

  let shader = device.create_shader_module(wgpu::include_spirv!(env!("rt.spv")));
  let sampler_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
    entries: &[wgpu::BindGroupLayoutEntry {
      binding: 0,
      visibility: wgpu::ShaderStages::FRAGMENT,
      ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
      count: None,
    }],
    label: None,
  });
  let sampler = device.create_sampler(&wgpu::SamplerDescriptor::default());
  let sampler_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
    layout: &sampler_layout,
    entries: &[wgpu::BindGroupEntry {
      binding: 0,
      resource: wgpu::BindingResource::Sampler(&sampler),
    }],
    label: None,
  });
  let tex_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
    entries: &[wgpu::BindGroupLayoutEntry {
      binding: 0,
      visibility: wgpu::ShaderStages::FRAGMENT,
      ty: wgpu::BindingType::Texture {
        multisampled: false,
        view_dimension: wgpu::TextureViewDimension::D2,
        sample_type: wgpu::TextureSampleType::Float { filterable: false },
      },
      count: None,
    }],
    label: None,
  });
  let buffer_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
    label: None,
    entries: &[wgpu::BindGroupLayoutEntry {
      binding: 0,
      count: None,
      visibility: wgpu::ShaderStages::FRAGMENT,
      ty: wgpu::BindingType::Buffer {
        has_dynamic_offset: false,
        min_binding_size: None,
        ty: wgpu::BufferBindingType::Storage { read_only: false },
      },
    }],
  });

  let obj: Obj = load_obj(std::io::BufReader::new(std::fs::File::open(
    "untitled.obj",
  )?))?;
  let mut min = Vec3::MAX;
  let mut max = Vec3::MIN;
  let vtx_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
    contents: cast_slice(
      &obj
        .indices
        .iter()
        .map(|i| {
          let pos = Vec3::from(obj.vertices[*i as usize].position);
          min = min.min(pos);
          max = max.max(pos);
          pos.extend(1.0)
        })
        .collect::<Vec<_>>(),
    ),
    usage: wgpu::BufferUsages::STORAGE,
    label: None,
  });
  log::info!("{} {}", min, max);
  let vtx_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
    layout: &buffer_layout,
    entries: &[wgpu::BindGroupEntry {
      binding: 0,
      resource: vtx_buf.as_entire_binding(),
    }],
    label: None,
  });

  let rt_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
    label: None,
    bind_group_layouts: &[&sampler_layout, &tex_layout, &tex_layout, &buffer_layout],
    push_constant_ranges: &[wgpu::PushConstantRange {
      stages: wgpu::ShaderStages::FRAGMENT,
      range: 0..mem::size_of::<Consts>() as _,
    }],
  });
  let rt_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
    layout: Some(&rt_pipeline_layout),
    vertex: wgpu::VertexState {
      module: &shader,
      entry_point: "main_v",
      buffers: &[],
    },
    fragment: Some(wgpu::FragmentState {
      module: &shader,
      entry_point: "main_f",
      targets: &[Some(wgpu::ColorTargetState {
        format: wgpu::TextureFormat::Rgba32Float,
        blend: None,
        write_mask: wgpu::ColorWrites::ALL,
      })],
    }),
    primitive: wgpu::PrimitiveState::default(),
    depth_stencil: None,
    multisample: wgpu::MultisampleState::default(),
    multiview: None,
    label: None,
  });

  let quad_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
    label: None,
    bind_group_layouts: &[&sampler_layout, &tex_layout],
    push_constant_ranges: &[wgpu::PushConstantRange {
      stages: wgpu::ShaderStages::FRAGMENT,
      range: 0..mem::size_of::<Consts>() as _,
    }],
  });
  let quad_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
    layout: Some(&quad_pipeline_layout),
    vertex: wgpu::VertexState {
      module: &shader,
      entry_point: "main_v",
      buffers: &[],
    },
    fragment: Some(wgpu::FragmentState {
      module: &shader,
      entry_point: "quad_f",
      targets: &[Some(wgpu::ColorTargetState {
        format: wgpu::TextureFormat::Bgra8Unorm,
        blend: None,
        write_mask: wgpu::ColorWrites::ALL,
      })],
    }),
    primitive: wgpu::PrimitiveState::default(),
    depth_stencil: None,
    multisample: wgpu::MultisampleState::default(),
    multiview: None,
    label: None,
  });

  let size = window.inner_size();
  let mut textures = Textures::new(&device, &tex_layout, size.width, size.height);
  let mut consts = Consts {
    screen_size: Vec2::new(size.width as _, size.height as _),
    rand: rand::random(),
    samples: 0,
  };

  let sky = image::open("alps_field_4k.exr")?.to_rgba32f();
  let sky_tex = device.create_texture_with_data(
    &queue,
    &wgpu::TextureDescriptor {
      size: wgpu::Extent3d {
        width: sky.width(),
        height: sky.height(),
        depth_or_array_layers: 1,
      },
      mip_level_count: 1,
      sample_count: 1,
      dimension: wgpu::TextureDimension::D2,
      format: wgpu::TextureFormat::Rgba32Float,
      usage: wgpu::TextureUsages::TEXTURE_BINDING,
      label: None,
      view_formats: &[],
    },
    cast_slice(&sky.as_raw()),
  );
  let sky_view = sky_tex.create_view(&wgpu::TextureViewDescriptor::default());
  let sky_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
    layout: &tex_layout,
    entries: &[wgpu::BindGroupEntry {
      binding: 0,
      resource: wgpu::BindingResource::TextureView(&sky_view),
    }],
    label: None,
  });

  event_loop.run(move |event, elwt| match event {
    Event::WindowEvent { event, .. } => match event {
      WindowEvent::Resized(size) => {
        surface.configure(
          &device,
          &wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: wgpu::TextureFormat::Bgra8Unorm,
            width: size.width,
            height: size.height,
            present_mode: wgpu::PresentMode::Immediate,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
          },
        );
        textures = Textures::new(&device, &tex_layout, size.width, size.height);
        consts.screen_size = Vec2::new(size.width as _, size.height as _);
        consts.samples = 0;
      }
      WindowEvent::CloseRequested => elwt.exit(),
      WindowEvent::RedrawRequested => {
        let surface = surface.get_current_texture().unwrap();
        let surface_view = surface
          .texture
          .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());

        if consts.samples < 1024 {
          consts.rand = rand::random();
          consts.samples += 1;
          log::info!("{}", consts.samples);

          let mut rt_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
              view: &textures.current_view,
              resolve_target: None,
              ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                store: true,
              },
            })],
            depth_stencil_attachment: None,
            label: None,
          });
          rt_pass.set_pipeline(&rt_pipeline);
          rt_pass.set_bind_group(0, &sampler_bind_group, &[]);
          rt_pass.set_bind_group(1, &textures.prev_bind_group, &[]);
          rt_pass.set_bind_group(2, &sky_bind_group, &[]);
          rt_pass.set_bind_group(3, &vtx_bind_group, &[]);
          rt_pass.set_push_constants(wgpu::ShaderStages::FRAGMENT, 0, cast(&consts));
          rt_pass.draw(0..3, 0..1);
          drop(rt_pass);
          encoder.copy_texture_to_texture(
            wgpu::ImageCopyTexture {
              texture: &textures.current,
              mip_level: 0,
              origin: wgpu::Origin3d::default(),
              aspect: wgpu::TextureAspect::All,
            },
            wgpu::ImageCopyTexture {
              texture: &textures.prev,
              mip_level: 0,
              origin: wgpu::Origin3d::default(),
              aspect: wgpu::TextureAspect::All,
            },
            textures.prev.size(),
          );
        }

        let mut quad_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
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
        quad_pass.set_pipeline(&quad_pipeline);
        quad_pass.set_bind_group(0, &sampler_bind_group, &[]);
        quad_pass.set_bind_group(1, &textures.prev_bind_group, &[]);
        quad_pass.set_push_constants(wgpu::ShaderStages::FRAGMENT, 0, cast(&consts));
        quad_pass.draw(0..3, 0..1);
        drop(quad_pass);

        queue.submit([encoder.finish()]);
        surface.present();
      }
      _ => {}
    },
    Event::AboutToWait => window.request_redraw(),
    _ => {}
  })?;
  Ok(())
}

struct Textures {
  current: wgpu::Texture,
  current_view: wgpu::TextureView,
  prev: wgpu::Texture,
  prev_bind_group: wgpu::BindGroup,
}

impl Textures {
  fn new(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, width: u32, height: u32) -> Self {
    let size = wgpu::Extent3d {
      width,
      height,
      depth_or_array_layers: 1,
    };
    let current = device.create_texture(&wgpu::TextureDescriptor {
      size,
      mip_level_count: 1,
      sample_count: 1,
      dimension: wgpu::TextureDimension::D2,
      format: wgpu::TextureFormat::Rgba32Float,
      usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
      view_formats: &[],
      label: None,
    });
    let current_view = current.create_view(&wgpu::TextureViewDescriptor::default());
    let prev = device.create_texture(&wgpu::TextureDescriptor {
      size: wgpu::Extent3d {
        width: size.width,
        height: size.height,
        depth_or_array_layers: 1,
      },
      mip_level_count: 1,
      sample_count: 1,
      dimension: wgpu::TextureDimension::D2,
      format: wgpu::TextureFormat::Rgba32Float,
      usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
      view_formats: &[],
      label: None,
    });
    let prev_view = prev.create_view(&wgpu::TextureViewDescriptor::default());
    let prev_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
      layout,
      entries: &[wgpu::BindGroupEntry {
        binding: 0,
        resource: wgpu::BindingResource::TextureView(&prev_view),
      }],
      label: None,
    });
    Self {
      current,
      current_view,
      prev,
      prev_bind_group,
    }
  }
}

fn cast_slice<T>(t: &[T]) -> &[u8] {
  unsafe { slice::from_raw_parts(t.as_ptr() as _, mem::size_of_val(t)) }
}

fn cast<T>(t: &T) -> &[u8] {
  cast_slice(slice::from_ref(t))
}
