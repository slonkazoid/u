use std::collections::HashMap;
use fontdue::{Font, FontSettings};
use fontdue::layout::{Layout, CoordinateSystem, TextStyle};
use guillotiere::{AtlasAllocator, Size, Point};
use glam::{Vec3, Vec2};
use shared::Vertex;
use crate::Result;

pub struct Context {
  fonts: FontAtlas,
  render_state: FrameOutput,
}

impl Context {
  pub fn new() -> Self {
    Self {
      fonts: FontAtlas::new(),
      render_state: FrameOutput::default(),
    }
  }

  pub fn fonts(&mut self) -> &mut FontAtlas {
    &mut self.fonts
  }

  pub fn begin_frame(&mut self) {
    self.render_state = FrameOutput::default();
    let mut layout = Layout::new(CoordinateSystem::PositiveYDown);
    let font = &self.fonts.fonts[0];
    let s: String = "the quick brown floppa 日本"
      .chars()
      .map(|c| {
        if c.is_whitespace() || font.2.contains_key(&c) {
          c
        } else {
          '\u{fffd}'
        }
      })
      .collect();
    layout.append(&[&self.fonts.fonts[0].0], &TextStyle::new(&s, 40.0, 0));

    for g in layout.glyphs() {
      if let Some((_, _, min, max)) = self.fonts.fonts[0].2.get(&g.parent) {
        self.render_state.push_rect(
          Vec2::new(g.x, g.y),
          Vec2::new(g.x + g.width as f32, g.y + g.height as f32),
          *min,
          *max,
          Vec3::ONE,
        );
      }
    }
  }

  pub fn end_frame(&mut self) -> FrameOutput {
    self.render_state.clone()
  }
}

pub struct FontAtlas {
  fonts: Vec<(Font, f32, HashMap<char, (Point, u16, Vec2, Vec2)>)>,
  packer: AtlasAllocator,
}

impl FontAtlas {
  fn new() -> Self {
    let mut packer = AtlasAllocator::new(Size::splat(256));
    packer.allocate(Size::splat(1));
    Self {
      fonts: vec![],
      packer,
    }
  }

  pub fn add_font(&mut self, data: &[u8], scale: f32) -> Result<usize> {
    let font = Font::from_bytes(
      data,
      FontSettings {
        collection_index: 0,
        scale,
      },
    )?;
    let mut uv_fac = 1.0;
    let mut glyphs: HashMap<char, (Point, u16, Vec2, Vec2)> = HashMap::new();
    for (c, i) in font.chars().iter() {
      let metrics = font.metrics_indexed(i.get(), scale);
      let size = Size::new(metrics.width as _, metrics.height as _);
      if size.is_empty() {
        continue;
      }
      let a = match self.packer.allocate(size) {
        Some(a) => a,
        None => {
          uv_fac *= 2.0;
          for glyph in glyphs.iter_mut() {
            glyph.1 .2 /= 2.0;
            glyph.1 .3 /= 2.0;
          }
          self.packer.grow(self.packer.size() * 2);
          match self.packer.allocate(size) {
            Some(a) => a,
            None => panic!("couldnt allocate glyph {:?}", c),
          }
        }
      };
      let pos = a.rectangle.min;
      let width = self.packer.size().width as usize;
      let min = pos.to_f32() / width as f32;
      let max = (pos + Size::new(metrics.width as _, metrics.height as _)).to_f32() / width as f32;
      glyphs.insert(
        *c,
        (pos, i.get(), min.to_array().into(), max.to_array().into()),
      );
    }
    for (_, _, glyphs) in &mut self.fonts {
      for glyph in glyphs {
        glyph.1 .2 /= uv_fac;
        glyph.1 .3 /= uv_fac;
      }
    }
    self.fonts.push((font, scale, glyphs));
    Ok(self.fonts.len() - 1)
  }

  pub fn size(&self) -> (u32, u32) {
    self.packer.size().to_u32().to_tuple()
  }

  pub fn build_tex(&self) -> Vec<[u8; 4]> {
    let width = self.packer.size().width as usize;
    let mut tex = vec![[0; 4]; width * width];
    tex[0] = [255; 4];
    for (font, scale, glyphs) in &self.fonts {
      for (_, (pos, i, _, _)) in glyphs.iter() {
        let (metrics, raster) = font.rasterize_indexed(*i, *scale);
        for y in 0..metrics.height {
          for x in 0..metrics.width {
            let px = raster[y * metrics.width + x];
            tex[(y + pos.y as usize) * width + x + pos.x as usize] = [px, px, px, 255];
          }
        }
      }
    }
    tex
  }
}

#[derive(Clone, Default)]
pub struct FrameOutput {
  pub vtx_buf: Vec<Vertex>,
  pub idx_buf: Vec<u32>,
}

impl FrameOutput {
  fn push_indices<const N: usize>(&mut self, indices: [u32; N]) {
    self
      .idx_buf
      .extend(indices.map(|l| l + self.vtx_buf.len() as u32));
  }

  fn push_rect(&mut self, min: Vec2, max: Vec2, min_uv: Vec2, max_uv: Vec2, color: Vec3) {
    self.push_indices([0, 1, 2, 1, 3, 2]);
    self.vtx_buf.extend([
      Vertex {
        pos: min,
        uv: min_uv,
        color,
      },
      Vertex {
        pos: Vec2::new(max.x, min.y),
        uv: Vec2::new(max_uv.x, min_uv.y),
        color,
      },
      Vertex {
        pos: Vec2::new(min.x, max.y),
        uv: Vec2::new(min_uv.x, max_uv.y),
        color,
      },
      Vertex {
        pos: max,
        uv: max_uv,
        color,
      },
    ]);
  }
}
