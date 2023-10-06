#![no_std]
use glam::Vec2;

#[repr(C)]
#[cfg_attr(not(target_arch = "spirv"), derive(Copy, Clone, Debug))]
pub struct Consts {
  pub screen_size: Vec2,
  pub rand: f32,
  pub samples: u32,
}
