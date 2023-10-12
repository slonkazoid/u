#![no_std]
use core::f32::consts::PI;
use spirv_std::{spirv, Sampler};
use spirv_std::image::Image2d;
use spirv_std::glam::{Vec2, Vec3, Vec4};
use spirv_std::num_traits::Float;
use shared::Consts;

#[spirv(vertex)]
pub fn main_v(
  #[spirv(vertex_index)] idx: u32,
  out_uv: &mut Vec2,
  #[spirv(position)] out_pos: &mut Vec4,
) {
  *out_uv = Vec2::new(((idx << 1) & 2) as f32, (idx & 2) as f32);
  *out_pos = (2.0 * *out_uv - Vec2::ONE).extend(0.0).extend(1.0);
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
    if distance > min && distance < max {
      Hit {
        distance,
        pos: ray.at(distance),
        normal: ab.cross(ac).normalize(),
        front_face: det > 0.0,
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

#[repr(u32)]
#[derive(Copy, Clone)]
enum Material {
  Lambertian,
  Metal,
  Emissive,
  Dielectric,
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
}

impl Camera {
  fn new(pos: Vec3, coord: Vec2, size: Vec2) -> Self {
    Self { pos, coord, size }
  }

  fn ray(&mut self, rng: &mut Rng) -> Ray {
    let relative =
      Vec2::new(self.coord.x + rng.gen(), self.coord.y + rng.gen()) * 2.0 / self.size - Vec2::ONE;
    Ray::new(
      self.pos,
      -(relative * Vec2::new(self.size.x / self.size.y, 1.0) * 0.6f32.tan()).extend(1.0),
    )
  }
}

const MAX_BOUNCES: usize = 32;

#[spirv(fragment)]
pub fn main_f(
  uv: Vec2,
  #[spirv(frag_coord)] frag_coord: Vec4,
  #[spirv(push_constant)] consts: &Consts,
  #[spirv(descriptor_set = 0, binding = 0)] sampler: &Sampler,
  #[spirv(descriptor_set = 1, binding = 0)] prev: &Image2d,
  #[spirv(descriptor_set = 2, binding = 0)] sky: &Image2d,
  #[spirv(storage_buffer, descriptor_set = 3, binding = 0)] vtx_buf: &mut [Vec4],
  out_color: &mut Vec4,
) {
  let coord = Vec2::new(frag_coord.x, frag_coord.y);
  let mut rng = Rng(uv * consts.rand);
  let mut cam = Camera::new(Vec3::new(0.0, 1.5, 0.0), coord, consts.screen_size);
  let materials = [
    (Vec3::splat(0.8), Material::Lambertian),
    (Vec3::X, Material::Lambertian),
    (Vec3::Y, Material::Lambertian),
    (Vec3::Z, Material::Lambertian),
    (Vec3::new(1.0, 0.0, 1.0), Material::Lambertian),
    (Vec3::ONE, Material::Dielectric),
    (Vec3::splat(0.8), Material::Metal),
  ];
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
  ];
  let meshes = [Mesh {
    start: 0,
    end: 2903,
    aabb: AABB(
      Vec3::new(-1.040056, 0.026624, -6.060498),
      Vec3::new(1.442725, 1.795877, -4.065464),
    ),
    mat: 0,
  }];

  *out_color = prev.sample_by_lod(*sampler, Vec2::new(uv.x, 1.0 - uv.y), 1.0);
  let mut attenuation = Vec3::ONE;
  let mut ray = cam.ray(&mut rng);
  for _ in 0..MAX_BOUNCES {
    let mut closest = Hit::default();
    closest.distance = f32::MAX;
    let mut obj = (0, 0);
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
      let (color, mat) = materials[closest.mat];
      ray = match mat {
        Material::Lambertian => Ray::new(closest.pos, closest.normal + rng.gen_in_sphere()),
        Material::Metal => Ray::new(closest.pos, reflect(ray.dir, closest.normal)),
        Material::Emissive => {
          *out_color += (color * attenuation).extend(1.0);
          break;
        }
        Material::Dielectric => {
          let ir = if closest.front_face { 1.0 / 1.5 } else { 1.5 };
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

fn to_equirect(mut dir: Vec3) -> Vec2 {
  dir = Vec3::new(dir.x, dir.z, dir.y);
  let mut longlat = Vec2::new(dir.y.atan2(dir.x), dir.z.acos());
  longlat.x += PI;
  return longlat / Vec2::new(2.0 * PI, PI);
}

fn unreal(x: Vec3) -> Vec3 {
  x / (x + 0.155) * 1.019
}

#[spirv(fragment)]
pub fn quad_f(
  uv: Vec2,
  #[spirv(push_constant)] consts: &Consts,
  #[spirv(descriptor_set = 0, binding = 0)] sampler: &Sampler,
  #[spirv(descriptor_set = 1, binding = 0)] tex: &Image2d,
  out_color: &mut Vec4,
) {
  *out_color =
    unreal(tex.sample(*sampler, Vec2::new(uv.x, 1.0 - uv.y)).truncate() / consts.samples as f32)
      .extend(1.0);
}

struct Rng(Vec2);

impl Rng {
  fn gen(&mut self) -> f32 {
    let res = (self.0.dot(Vec2::new(12.9898, 78.233)).sin() * 43758.5453).fract();
    self.0 = Vec2::new(
      (self.0.x + res + 17.825) % 3718.0,
      (self.0.y + res + 72.7859) % 1739.0,
    );
    res
  }

  fn gen_pos(&mut self) -> f32 {
    (self.gen() + 1.0) / 2.0
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
