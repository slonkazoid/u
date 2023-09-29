#![no_std]
use spirv_std::spirv;
use spirv_std::glam::{Vec2, Vec3, Vec4};
use spirv_std::num_traits::Float;
use shared::Consts;

#[spirv(vertex)]
pub fn main_v(#[spirv(vertex_index)] idx: u32, #[spirv(position)] out_pos: &mut Vec4) {
  let uv = Vec2::new(((idx << 1) & 2) as f32, (idx & 2) as f32);
  *out_pos = (2.0 * uv - Vec2::ONE).extend(0.0).extend(1.0);
}

struct Ray {
  origin: Vec3,
  dir: Vec3,
}

impl Ray {
  fn new(origin: Vec3, dir: Vec3) -> Self {
    Self { origin, dir }
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
  fn hit(&self, ray: &Ray, max: f32) -> Hit {
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
    if distance > max {
      distance = (-b + sqrtd) / a;
      if distance > max {
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

#[derive(Copy, Clone)]
enum Material {
  Lambertian,
  Metal,
}

impl Material {
  fn scatter(&self, ray: Ray, hit: Hit, rng: &mut Rng) -> Ray {
    match self {
      Self::Lambertian => Ray::new(hit.pos, hit.normal + rng.gen_in_sphere()),
      Self::Metal => Ray::new(hit.pos, reflect(ray.dir, hit.normal).normalize()),
    }
  }
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
      -(relative * Vec2::new(self.size.x / self.size.y, 1.0) * 0.7f32.tan())
        .extend(1.0)
        .normalize(),
    )
  }
}

const SAMPLES: usize = 48;
const MAX_BOUNCES: usize = 4;

#[spirv(fragment)]
pub fn main_f(
  #[spirv(frag_coord)] frag_coord: Vec4,
  #[spirv(push_constant)] consts: &Consts,
  out_color: &mut Vec4,
) {
  let coord = Vec2::new(frag_coord.x, frag_coord.y);
  let mut rng = Rng(coord);
  let mut cam = Camera::new(Vec3::ZERO, coord, consts.screen_size);
  let objects = [
    Sphere {
      pos: Vec3::new(0.0, -100.5, -1.0),
      radius: 100.0,
      color: Vec3::splat(0.8),
      mat: Material::Lambertian,
    },
    Sphere {
      pos: Vec3::new(2.0, 0.0, -2.5),
      radius: 1.5,
      color: Vec3::X,
      mat: Material::Lambertian,
    },
    Sphere {
      pos: Vec3::new(-2.0, 0.0, -2.5),
      radius: 1.5,
      color: Vec3::Z,
      mat: Material::Lambertian,
    },
    Sphere {
      pos: Vec3::new(0.0, 0.0, 2.0),
      radius: 1.5,
      color: Vec3::Y,
      mat: Material::Lambertian,
    },
    Sphere {
      pos: Vec3::new(0.0, 0.5, -2.5),
      radius: 0.4,
      color: Vec3::splat(0.8),
      mat: Material::Metal,
    },
  ];

  for _ in 0..SAMPLES {
    *out_color += color(cam.ray(&mut rng), objects.clone(), &mut rng).extend(1.0);
  }
  *out_color /= Vec4::splat(SAMPLES as _);
}

fn color(mut ray: Ray, objects: [Sphere; 5], rng: &mut Rng) -> Vec3 {
  let mut attenuation = Vec3::ONE;

  for _ in 0..MAX_BOUNCES {
    let mut closest = Hit::default();
    let mut obj = 0;
    for i in 0..objects.len() {
      let hit = objects[i].hit(
        &ray,
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
      ray = objects[obj].mat.scatter(ray, closest, rng);
      attenuation *= objects[obj].color;
    } else {
      return Vec3::new(0.5, 0.7, 1.0) * attenuation;
    }
  }
  Vec3::ZERO
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

  fn gen_in_sphere(&mut self) -> Vec3 {
    loop {
      let v = Vec3::new(self.gen(), self.gen(), self.gen());
      if v.length() < 1.0 {
        return v;
      }
    }
  }
}

pub fn reflect(v: Vec3, n: Vec3) -> Vec3 {
  v - 2.0 * v.dot(n) * n
}
