#![no_std]
#![feature(unchecked_math)]
use core::mem;
use core::f32::consts::PI;
use spirv_std::{spirv, Sampler};
use spirv_std::image::Image2d;
use spirv_std::glam::{Vec2, Vec3, Vec4};
use spirv_std::num_traits::Float;
use shared::{Consts, Material};

#[spirv(vertex)]
pub fn quad_v(
  #[spirv(vertex_index)] idx: u32,
  #[spirv(uniform, descriptor_set = 0, binding = 0)] consts: &Consts,
  out_uv: &mut Vec2,
  #[spirv(position)] out_pos: &mut Vec4,
) {
  *out_uv = Vec2::new(((idx << 1) & 2) as f32, (idx & 2) as f32);
  *out_pos = (2.0 * *out_uv - Vec2::ONE).extend(consts.zero).extend(1.0);
}

struct Ray {
  origin: Vec3,
  dir: Vec3,
}

impl Ray {
  fn new(origin: Vec3, dir: Vec3) -> Self {
    Self {
      origin,
      dir: dir.normalize(),
    }
  }

  fn at(&self, t: f32) -> Vec3 {
    self.origin + t * self.dir
  }
}

struct Sphere {
  pos: Vec3,
  radius: f32,
  mat: usize,
}

impl Sphere {
  fn hit(&self, ray: &Ray, min: f32, max: f32) -> Hit {
    let oc = ray.origin - self.pos;
    let a = ray.dir.length_squared();
    let b = oc.dot(ray.dir);
    let c = oc.length_squared() - self.radius * self.radius;
    let disc = b * b - a * c;

    if disc < 0.0 {
      return Hit::default();
    }
    let sqrtd = disc.sqrt();
    let mut distance = (-b - sqrtd) / a;
    if distance < min || distance > max {
      distance = (-b + sqrtd) / a;
      if distance < min || distance > max {
        return Hit::default();
      }
    }
    let pos = ray.at(distance);
    let normal = (pos - self.pos) / self.radius;
    let front_face = ray.dir.dot(normal) < 0.0;
    Hit {
      distance,
      pos,
      normal: if front_face { normal } else { -normal },
      front_face,
      mat: self.mat,
    }
  }
}

struct Mesh {
  start: usize,
  end: usize,
  aabb: AABB,
  mat: usize,
}

#[derive(Copy, Clone, Default)]
pub struct Tri(Vec3, Vec3, Vec3, usize);

impl Tri {
  fn hit(&self, ray: &Ray, min: f32, max: f32) -> Hit {
    let ab = self.1 - self.0;
    let ac = self.2 - self.0;
    let ao = ray.origin - self.0;
    let u_vec = ray.dir.cross(ac);
    let det = ab.dot(u_vec);
    let inv_det = 1.0 / det;
    let u = ao.dot(u_vec) * inv_det;
    if u < 0.0 || u > 1.0 {
      return Hit::default();
    }
    let v_vec = ao.cross(ab);
    let v = ray.dir.dot(v_vec) * inv_det;
    if v < 0.0 || u + v > 1.0 {
      return Hit::default();
    }
    let distance = ac.dot(v_vec) * inv_det;
    let normal = ab.cross(ac).normalize();
    let front_face = ray.dir.dot(normal) < 0.0;
    if distance > min && distance < max {
      Hit {
        distance,
        pos: ray.at(distance),
        normal: if front_face { normal } else { -normal },
        front_face,
        mat: self.3,
      }
    } else {
      Hit::default()
    }
  }
}

pub struct AABB(Vec3, Vec3);

impl AABB {
  fn hit(&self, ray: &Ray) -> bool {
    let min = (self.0 - ray.origin) / ray.dir;
    let tmax = (self.1 - ray.origin) / ray.dir;
    let t1 = min.min(tmax);
    let t2 = min.max(tmax);
    let near = t1.x.max(t1.y).max(t1.z);
    let far = t2.x.min(t2.y).min(t2.z);
    near < far
  }
}

#[derive(Default)]
struct Hit {
  distance: f32,
  pos: Vec3,
  normal: Vec3,
  front_face: bool,
  mat: usize,
}

struct Camera {
  pos: Vec3,
  coord: Vec2,
  size: Vec2,
  fov: f32,
  defocus: f32,
  focal_length: f32,
}

impl Camera {
  fn new(pos: Vec3, coord: Vec2, size: Vec2) -> Self {
    Self {
      pos,
      coord,
      size,
      fov: 0.6,
      defocus: 0.05,
      focal_length: 5.0,
    }
  }

  fn ray(&mut self, rng: &mut Rng) -> Ray {
    let relative =
      Vec2::new(self.coord.x + rng.gen(), self.coord.y + rng.gen()) * 2.0 / self.size - Vec2::ONE;
    let dir = -(relative * Vec2::new(self.size.x / self.size.y, 1.0) * self.fov.tan()).extend(1.0);
    let start = self.pos + (self.defocus * rng.gen_in_circle()).extend(0.0);
    let target = self.pos + dir * self.focal_length;
    Ray::new(start, target - start)
  }
}

const MAX_BOUNCES: usize = 32;

fn hash(key: u32) -> u32 {
  let mut h = 0;
  for i in 0..4 {
    h += (key >> (i * 8)) & 0xFF;
    h += h << 10;
    h ^= h >> 6;
  }
  h += h << 3;
  h ^= h >> 11;
  h += h << 15;
  h
}

#[spirv(fragment)]
pub fn main_f(
  uv: Vec2,
  #[spirv(frag_coord)] frag_coord: Vec4,
  #[spirv(uniform, descriptor_set = 0, binding = 0)] consts: &Consts,
  #[spirv(descriptor_set = 1, binding = 0)] sampler: &Sampler,
  #[spirv(descriptor_set = 2, binding = 0)] prev: &Image2d,
  #[spirv(descriptor_set = 3, binding = 0)] sky: &Image2d,
  #[spirv(storage_buffer, descriptor_set = 4, binding = 0)] vtx_buf: &mut [Vec4],
  #[spirv(storage_buffer, descriptor_set = 4, binding = 1)] materials: &mut [Vec4],
  out_color: &mut Vec4,
) {
  let coord = Vec2::new(frag_coord.x, frag_coord.y);
  let mut rng = Rng(consts.rand ^ hash((coord.x + consts.size.y * coord.y) as _));
  let mut cam = Camera::new(Vec3::new(0.0, 1.5, 0.0), coord, consts.size);
  let spheres = [
    Sphere {
      pos: Vec3::new(0.0, -200.0, 0.0),
      radius: 200.0,
      mat: 0,
    },
    Sphere {
      pos: Vec3::new(-3.0, 1.5, -7.5),
      radius: 1.5,
      mat: 1,
    },
    Sphere {
      pos: Vec3::new(0.0, 1.5, -10.0),
      radius: 1.5,
      mat: 2,
    },
    Sphere {
      pos: Vec3::new(3.0, 1.5, -7.5),
      radius: 1.5,
      mat: 3,
    },
    Sphere {
      pos: Vec3::new(0.0, 1.5, 2.5),
      radius: 1.5,
      mat: 4,
    },
    Sphere {
      pos: Vec3::new(1.5, 1.0, -3.0),
      radius: 0.75,
      mat: 5,
    },
    Sphere {
      pos: Vec3::new(-1.5, 1.0, -3.0),
      radius: 0.75,
      mat: 6,
    },
    // Sphere {
    //   pos: Vec3::new(6.0, 6.0, 6.0),
    //   radius: 4.0,
    //   mat: 7,
    // },
  ];
  let meshes = [Mesh {
    start: 0,
    end: 2901,
    aabb: AABB(
      Vec3::new(-1.040056, 0.026624, -6.060498),
      Vec3::new(1.442725, 1.795877, -4.065464),
    ),
    mat: 0,
  }];

  *out_color = prev.sample_by_lod(*sampler, Vec2::new(uv.x, 1.0 - uv.y), 1.0);

  let wavelength = rng.gen_pos() * 370.0 + 380.0;
  let mut attenuation = match wavelength {
    380.0..=440.0 => {
      let at = 0.3 + 0.7 * (wavelength - 380.0) / (440.0 - 380.0);
      Vec3::new((-(wavelength - 440.0) / (440.0 - 380.0)) * at, 0.0, at)
    }
    440.0..=490.0 => Vec3::new(0.0, (wavelength - 440.0) / (490.0 - 440.0), 1.0),
    510.0..=580.0 => Vec3::new((wavelength - 510.0) / (580.0 - 510.0), 1.0, 0.0),
    580.0..=645.0 => Vec3::new(1.0, -(wavelength - 645.0) / (645.0 - 580.0), 0.0),
    645.0..=750.0 => {
      let at = 0.3 + 0.7 * (750.0 - wavelength) / (750.0 - 645.0);
      Vec3::new(at, 0.0, 0.0)
    }
    _ => Vec3::ZERO,
  };
  attenuation *= Vec3::new(2.74738275, 2.97417918, 3.33566826); //?

  let mut ray = cam.ray(&mut rng);
  for _ in 0..MAX_BOUNCES {
    let mut closest = Hit::default();
    closest.distance = f32::MAX;
    for i in 0..spheres.len() {
      let hit = spheres[i].hit(&ray, 0.001, closest.distance);
      if hit.distance > 0.0 {
        closest = hit;
      }
    }
    for i in 0..meshes.len() {
      if meshes[i].aabb.hit(&ray) {
        for f in meshes[i].start..meshes[i].end / 3 {
          let hit = Tri(
            vtx_buf[3 * f].truncate(),
            vtx_buf[3 * f + 1].truncate(),
            vtx_buf[3 * f + 2].truncate(),
            meshes[i].mat,
          )
          .hit(&ray, 0.001, closest.distance);
          if hit.distance > 0.0 {
            closest = hit;
          }
        }
      }
    }

    if closest.distance != f32::MAX {
      let mat = materials[closest.mat];
      let color = mat.truncate();
      ray = match mat.w.into() {
        Material::Lambertian => Ray::new(closest.pos, closest.normal + rng.gen_in_sphere()),
        Material::Metal => Ray::new(closest.pos, reflect(ray.dir, closest.normal)),
        Material::Emissive => {
          *out_color += (color * attenuation).extend(1.0);
          break;
        }
        Material::Dielectric => {
          let ir = 1.5 + (wavelength - 150.0) * 0.0005;
          let ir = if closest.front_face { 1.0 / ir } else { ir };
          let cos_theta = (-ray.dir).dot(closest.normal).min(1.0);
          let sin_theta = (1.0 - cos_theta * cos_theta).sqrt();

          let cannot_refract = ir * sin_theta > 1.0;
          let will_reflect = rng.gen_pos() < schlick(cos_theta, ir);
          let dir = if cannot_refract || will_reflect {
            reflect(ray.dir, closest.normal)
          } else {
            refract(ray.dir, closest.normal, ir)
          };

          Ray::new(closest.pos, dir)
        }
      };
      attenuation *= color;
    } else {
      *out_color +=
        sky.sample_by_lod(*sampler, to_equirect(ray.dir), 1.0) * attenuation.extend(1.0);
      break;
    }
  }
}

fn to_equirect(dir: Vec3) -> Vec2 {
  Vec2::new(dir.z.atan2(dir.x) + PI, dir.y.acos()) / Vec2::new(2.0 * PI, PI)
}

fn unreal(x: Vec3) -> Vec3 {
  x / (x + 0.155) * 1.019
}

#[spirv(fragment)]
pub fn quad_f(
  uv: Vec2,
  #[spirv(uniform, descriptor_set = 0, binding = 0)] consts: &Consts,
  #[spirv(descriptor_set = 1, binding = 0)] sampler: &Sampler,
  #[spirv(descriptor_set = 2, binding = 0)] tex: &Image2d,
  out_color: &mut Vec4,
) {
  *out_color =
    unreal(tex.sample(*sampler, Vec2::new(uv.x, 1.0 - uv.y)).truncate() / consts.samples as f32)
      .extend(1.0);
}

struct Rng(u32);

impl Rng {
  fn gen(&mut self) -> f32 {
    self.0 = unsafe { self.0.unchecked_mul(0xadb4a92d) } + 1;
    let m = (self.0 >> 9) | 0x40000000;
    unsafe { mem::transmute::<_, f32>(m) - 3.0 }
  }

  fn gen_pos(&mut self) -> f32 {
    (self.gen() + 1.0) / 2.0
  }

  fn gen_in_circle(&mut self) -> Vec2 {
    let t = PI * self.gen();
    self.gen_pos().sqrt() * Vec2::new(t.cos(), t.sin())
  }

  fn gen_in_sphere(&mut self) -> Vec3 {
    let u = self.gen();
    let v = self.gen();
    let theta = u * 2.0 * PI;
    let phi = (2.0 * v - 1.0).acos();
    self.gen().cbrt() * Vec3::new(phi.sin() * theta.cos(), phi.sin() * theta.sin(), phi.cos())
  }
}

fn reflect(v: Vec3, n: Vec3) -> Vec3 {
  v - 2.0 * v.dot(n) * n
}

fn refract(v: Vec3, n: Vec3, ir: f32) -> Vec3 {
  let cos_theta = (-v).dot(n).min(1.0);
  let perp = ir * (v + cos_theta * n);
  let parallel = (1.0 - perp.length_squared()).abs().sqrt() * n;
  perp - parallel
}

fn schlick(cos: f32, ir: f32) -> f32 {
  let r0 = ((1.0 - ir) / (1.0 + ir)).powf(2.0);
  r0 + (1.0 - r0) * (1.0 - cos).powf(5.0)
}

#[spirv(vertex)]
pub fn ui_v(
  pos: Vec2,
  uv: Vec2,
  color: Vec3,
  #[spirv(uniform, descriptor_set = 0, binding = 0)] consts: &Consts,
  #[spirv(position)] out_pos: &mut Vec4,
  out_uv: &mut Vec2,
  out_color: &mut Vec3,
) {
  *out_pos = Vec4::new(
    2.0 * pos.x / consts.size.x - 1.0,
    1.0 - 2.0 * pos.y / consts.size.y,
    0.0,
    1.0,
  );
  *out_uv = uv;
  *out_color = color;
}

#[spirv(fragment)]
pub fn ui_f(
  uv: Vec2,
  color: Vec3,
  #[spirv(uniform, descriptor_set = 0, binding = 0)] consts: &Consts,
  #[spirv(descriptor_set = 1, binding = 0)] sampler: &Sampler,
  #[spirv(descriptor_set = 2, binding = 0)] tex: &Image2d,
  out_color: &mut Vec4,
) {
  // maybe needs gamma correction?
  *out_color = tex.sample(*sampler, uv) * color.extend(consts.zero + 1.0);
}
