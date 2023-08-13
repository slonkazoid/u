#![no_std]
use spirv_std::{spirv, Image, Sampler};
use spirv_std::glam::{Vec2, Vec3, Vec4};
use shared::{Vertex, Consts};

#[spirv(vertex)]
pub fn main_v(
  pos: Vec2,
  uv: Vec2,
  color: Vec3,
  #[spirv(push_constant)] consts: &Consts,
  #[spirv(position)] out_pos: &mut Vec4,
  out_uv: &mut Vec2,
  out_color: &mut Vec3,
) {
  *out_pos = Vec4::new(
    2.0 * pos.x / consts.screen_size.x - 1.0,
    1.0 - 2.0 * pos.y / consts.screen_size.y,
    0.0,
    1.0,
  );
  *out_uv = uv;
  *out_color = color;
}

#[spirv(fragment)]
pub fn main_f(
  uv: Vec2,
  color: Vec3,
  #[spirv(descriptor_set = 0, binding = 0)] tex: &Image!(2D, type = f32, sampled),
  #[spirv(descriptor_set = 0, binding = 1)] sampler: &Sampler,
  out_color: &mut Vec4,
) {
  *out_color = tex.sample(*sampler, uv) * color.extend(1.0);
}
