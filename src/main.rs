mod ui;

use std::{mem, slice};
use std::io::BufReader;
use std::fs::File;
use winit::window::WindowBuilder;
use winit::event_loop::EventLoop;
use winit::event::{Event, WindowEvent, MouseButton, ElementState};
use wgpu::util::DeviceExt;
use log::LevelFilter;
use glam::{Vec2, Vec3};
use obj::{load_obj, Obj};
use shared::{Consts, Vertex, Material};
use crate::ui::Context;

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

const SAMPLES: u32 = 4096;

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
      features: wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES,
      limits: wgpu::Limits {
        max_bind_groups: 8,
        ..Default::default()
      },
      label: None,
    },
    None,
  ))?;

  let shader = device.create_shader_module(wgpu::include_spirv!(env!("shaders.spv")));
  let rt_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
    layout: None,
    vertex: wgpu::VertexState {
      module: &shader,
      entry_point: "quad_v",
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
  let quad_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
    layout: None,
    vertex: wgpu::VertexState {
      module: &shader,
      entry_point: "quad_v",
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
  let ui_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
    layout: None,
    vertex: wgpu::VertexState {
      module: &shader,
      entry_point: "ui_v",
      buffers: &[wgpu::VertexBufferLayout {
        array_stride: mem::size_of::<Vertex>() as _,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x3],
      }],
    },
    fragment: Some(wgpu::FragmentState {
      module: &shader,
      entry_point: "ui_f",
      targets: &[Some(wgpu::ColorTargetState {
        format: wgpu::TextureFormat::Bgra8Unorm,
        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
        write_mask: wgpu::ColorWrites::ALL,
      })],
    }),
    primitive: wgpu::PrimitiveState::default(),
    depth_stencil: None,
    multisample: wgpu::MultisampleState::default(),
    multiview: None,
    label: None,
  });
  let tex_layout = rt_pipeline.get_bind_group_layout(2);

  let uniform_buf = device.create_buffer(&wgpu::BufferDescriptor {
    size: mem::size_of::<Consts>() as _,
    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    mapped_at_creation: false,
    label: None,
  });
  let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
    layout: &rt_pipeline.get_bind_group_layout(0),
    entries: &[wgpu::BindGroupEntry {
      binding: 0,
      resource: uniform_buf.as_entire_binding(),
    }],
    label: None,
  });

  let sampler = device.create_sampler(&wgpu::SamplerDescriptor::default());
  let sampler_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
    layout: &rt_pipeline.get_bind_group_layout(1),
    entries: &[wgpu::BindGroupEntry {
      binding: 0,
      resource: wgpu::BindingResource::Sampler(&sampler),
    }],
    label: None,
  });

  let obj: Obj = load_obj(BufReader::new(File::open("untitled.obj")?))?;
  let mut min = Vec3::MAX;
  let mut max = Vec3::MIN;
  let verts = obj
    .indices
    .iter()
    .map(|i| {
      let pos = Vec3::from(obj.vertices[*i as usize].position);
      min = min.min(pos);
      max = max.max(pos);
      pos.extend(1.0)
    })
    .collect::<Vec<_>>();
  let vtx_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
    contents: cast_slice(&verts),
    usage: wgpu::BufferUsages::STORAGE,
    label: None,
  });
  log::info!("{} {} {}", min, max, verts.len());
  let materials = [
    (Vec3::splat(0.8), Material::Lambertian),
    (Vec3::X, Material::Lambertian),
    (Vec3::Y, Material::Lambertian),
    (Vec3::Z, Material::Lambertian),
    (Vec3::new(1.0, 0.0, 1.0), Material::Lambertian),
    (Vec3::ONE, Material::Dielectric),
    (Vec3::splat(0.8), Material::Metal),
    (Vec3::splat(5.0), Material::Emissive),
  ];
  let material_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
    contents: cast_slice(&materials),
    usage: wgpu::BufferUsages::STORAGE,
    label: None,
  });
  let scene_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
    layout: &rt_pipeline.get_bind_group_layout(4),
    entries: &[
      wgpu::BindGroupEntry {
        binding: 0,
        resource: vtx_buf.as_entire_binding(),
      },
      wgpu::BindGroupEntry {
        binding: 1,
        resource: material_buf.as_entire_binding(),
      },
    ],
    label: None,
  });

  let sky = image::open("alps_field_4k.exr")?.to_rgba32f();
  let sky_tex = device.create_texture_with_data(
    // let sky_tex = device.create_texture(
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

  let mut ctx = Context::new();
  ctx.fonts().add_font(include_bytes!("roboto.ttf"), 40.0)?;
  let font_tex = device.create_texture_with_data(
    &queue,
    &wgpu::TextureDescriptor {
      size: wgpu::Extent3d {
        width: ctx.fonts().size().0,
        height: ctx.fonts().size().1,
        depth_or_array_layers: 1,
      },
      mip_level_count: 1,
      sample_count: 1,
      dimension: wgpu::TextureDimension::D2,
      format: wgpu::TextureFormat::Rgba8Unorm,
      usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
      view_formats: &[],
      label: None,
    },
    cast_slice(&ctx.fonts().build_tex()),
  );
  let font_view = font_tex.create_view(&wgpu::TextureViewDescriptor::default());
  let font_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
    layout: &tex_layout,
    entries: &[wgpu::BindGroupEntry {
      binding: 0,
      resource: wgpu::BindingResource::TextureView(&font_view),
    }],
    label: None,
  });

  let size = window.inner_size();
  let mut textures = Textures::new(&device, &tex_layout, size.width, size.height);
  let mut consts = Consts {
    size: Vec2::new(size.width as _, size.height as _),
    rand: rand::random(),
    samples: 1,
    zero: 0.0,
  };

  event_loop.run(move |event, elwt| {
    handle_ui_event(&mut ctx, &event);
    match event {
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
          consts.size = Vec2::new(size.width as _, size.height as _);
          consts.samples = 1;
        }
        WindowEvent::CloseRequested => elwt.exit(),
        WindowEvent::RedrawRequested => {
          let surface = surface.get_current_texture().unwrap();
          let surface_view = surface
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
          let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());

          queue.write_buffer(&uniform_buf, 0, cast(&consts));

          if consts.samples <= SAMPLES {
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
            rt_pass.set_bind_group(0, &uniform_bind_group, &[]);
            rt_pass.set_bind_group(1, &sampler_bind_group, &[]);
            rt_pass.set_bind_group(2, &textures.prev_bind_group, &[]);
            rt_pass.set_bind_group(3, &sky_bind_group, &[]);
            rt_pass.set_bind_group(4, &scene_bind_group, &[]);
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
            consts.rand = rand::random();
            consts.samples += 1;
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
          quad_pass.set_bind_group(0, &uniform_bind_group, &[]);
          quad_pass.set_bind_group(1, &sampler_bind_group, &[]);
          quad_pass.set_bind_group(2, &textures.prev_bind_group, &[]);
          quad_pass.draw(0..3, 0..1);
          drop(quad_pass);

          let mut ui = ctx.begin_frame();
          ui.text(&format!("{}/{}", consts.samples - 1, SAMPLES));
          let out = ctx.end_frame();
          let vtx_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            contents: cast_slice(&out.vtx_buf),
            usage: wgpu::BufferUsages::VERTEX,
            label: None,
          });
          let idx_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            contents: cast_slice(&out.idx_buf),
            usage: wgpu::BufferUsages::INDEX,
            label: None,
          });
          let mut ui_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
              view: &surface_view,
              resolve_target: None,
              ops: wgpu::Operations {
                load: wgpu::LoadOp::Load,
                store: true,
              },
            })],
            depth_stencil_attachment: None,
            label: None,
          });
          ui_pass.set_pipeline(&ui_pipeline);
          ui_pass.set_bind_group(0, &uniform_bind_group, &[]);
          ui_pass.set_bind_group(1, &sampler_bind_group, &[]);
          ui_pass.set_bind_group(2, &font_bind_group, &[]);
          ui_pass.set_vertex_buffer(0, vtx_buf.slice(..));
          ui_pass.set_index_buffer(idx_buf.slice(..), wgpu::IndexFormat::Uint32);
          ui_pass.draw_indexed(0..out.idx_buf.len() as _, 0, 0..1);
          drop(ui_pass);

          queue.submit([encoder.finish()]);
          surface.present();
          instance.poll_all(true);
        }
        _ => {}
      },
      Event::AboutToWait => window.request_redraw(),

      _ => {}
    }
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

fn handle_ui_event<T>(ctx: &mut Context, event: &Event<T>) {
  let input = ctx.input();
  match event {
    Event::WindowEvent { event, .. } => match event {
      WindowEvent::CursorMoved { position, .. } => {
        input.cursor_pos = Vec2::new(position.x as _, position.y as _);
      }
      WindowEvent::MouseInput { button, state, .. } => {
        input.mouse_buttons[match button {
          MouseButton::Left => 0,
          MouseButton::Middle => 2,
          MouseButton::Right => 3,
          _ => return,
        }] = *state == ElementState::Pressed;
      }
      _ => {}
    },
    _ => {}
  }
}
