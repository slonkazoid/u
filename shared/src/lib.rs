#![no_std]
use core::mem;
use glam::{Vec2, Vec3};

#[repr(C, align(16))]
#[cfg_attr(not(target_arch = "spirv"), derive(Copy, Clone, Debug))]
pub struct Consts {
  pub size: Vec2,
  pub rand: u32,
  pub samples: u32,
  pub zero: f32,
}

#[repr(C)]
#[cfg_attr(not(target_arch = "spirv"), derive(Copy, Clone, Debug))]
pub struct Vertex {
  pub pos: Vec2,
  pub uv: Vec2,
  pub color: Vec3,
}

#[repr(u32)]
#[derive(Copy, Clone)]
pub enum Material {
  Lambertian,
  Metal,
  Emissive,
  Dielectric,
}

impl From<f32> for Material {
  fn from(f: f32) -> Self {
    unsafe { mem::transmute(f) }
  }
}
