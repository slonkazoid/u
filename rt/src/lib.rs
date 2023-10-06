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

#[derive(Copy, Clone)]
struct Sphere {
  pos: Vec3,
  radius: f32,
  color: Vec3,
  mat: Material,
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
    }
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
  #[spirv(descriptor_set = 0, binding = 0)] tex: &Image2d,
  #[spirv(descriptor_set = 0, binding = 1)] sampler: &Sampler,
  out_color: &mut Vec4,
) {
  let coord = Vec2::new(frag_coord.x, frag_coord.y);
  let mut rng = Rng(uv * consts.rand);
  let mut cam = Camera::new(Vec3::new(0.0, 1.5, 0.0), coord, consts.screen_size);
  let objects = [
    Sphere {
      pos: Vec3::new(0.0, -200.0, 0.0),
      radius: 200.0,
      color: Vec3::splat(0.8),
      mat: Material::Lambertian,
    },
    Sphere {
      pos: Vec3::new(-3.0, 1.5, -7.5),
      radius: 1.5,
      color: Vec3::X,
      mat: Material::Lambertian,
    },
    Sphere {
      pos: Vec3::new(0.0, 1.5, -10.0),
      radius: 1.5,
      color: Vec3::Y,
      mat: Material::Lambertian,
    },
    Sphere {
      pos: Vec3::new(3.0, 1.5, -7.5),
      radius: 1.5,
      color: Vec3::Z,
      mat: Material::Lambertian,
    },
    Sphere {
      pos: Vec3::new(0.0, 1.5, 2.5),
      radius: 1.5,
      color: Vec3::new(1.0, 0.0, 1.0),
      mat: Material::Lambertian,
    },
    Sphere {
      pos: Vec3::new(1.0, 1.0, -3.0),
      radius: 0.75,
      color: Vec3::ONE,
      mat: Material::Dielectric,
    },
    Sphere {
      pos: Vec3::new(-1.0, 1.0, -3.0),
      radius: 0.75,
      color: Vec3::splat(0.8),
      mat: Material::Metal,
    },
    Sphere {
      pos: Vec3::new(8.0, 8.0, -8.0),
      radius: 4.0,
      color: Vec3::splat(5.0),
      mat: Material::Emissive,
    },
  ];

  *out_color = tex.sample(*sampler, Vec2::new(uv.x, 1.0 - uv.y));
  let mut attenuation = Vec3::ONE;
  let mut ray = cam.ray(&mut rng);
  for _ in 0..MAX_BOUNCES {
    let mut closest = Hit::default();
    let mut obj = 0;
    for i in 0..objects.len() {
      let hit = objects[i].hit(
        &ray,
        0.001,
        if closest.distance == 0.0 {
          f32::MAX
        } else {
          closest.distance
        },
      );
      if hit.distance > 0.0 {
        closest = hit;
        obj = i;
      }
    }
    if closest.distance > 0.0 {
      ray = match objects[obj].mat {
        Material::Lambertian => Ray::new(closest.pos, closest.normal + rng.gen_in_sphere()),
        Material::Metal => Ray::new(closest.pos, reflect(ray.dir, closest.normal)),
        Material::Emissive => {
          *out_color += (objects[obj].color * attenuation).extend(1.0);
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
      attenuation *= objects[obj].color;
    } else {
      // let sky = Vec3::new(0.25, 0.35, 0.5);
      let sky = Vec3::splat(0.01);
      *out_color += (sky * attenuation).extend(1.0);
      break;
    }
  }
}

fn unreal(x: Vec3) -> Vec3 {
  x / (x + 0.155) * 1.019
}

#[spirv(fragment)]
pub fn quad_f(
  uv: Vec2,
  #[spirv(push_constant)] consts: &Consts,
  #[spirv(descriptor_set = 0, binding = 0)] tex: &Image2d,
  #[spirv(descriptor_set = 0, binding = 1)] sampler: &Sampler,
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
