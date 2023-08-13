#![no_std]
use glam::{Vec2, Vec3};

#[repr(C)]
#[cfg_attr(not(target_arch = "spirv"), derive(Copy, Clone, Debug))]
pub struct Vertex {
  pub pos: Vec2,
  pub uv: Vec2,
  pub color: Vec3,
}

#[repr(C)]
#[cfg_attr(not(target_arch = "spirv"), derive(Copy, Clone, Debug))]
pub struct Consts {
  pub screen_size: Vec2,
}
