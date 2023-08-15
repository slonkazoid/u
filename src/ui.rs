use std::hash::{Hash, Hasher};
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use fontdue::{Font, FontSettings};
use fontdue::layout::{Layout, LayoutSettings, CoordinateSystem, TextStyle};
use guillotiere::{AtlasAllocator, Size, Point};
use glam::{Vec3, Vec2};
use shared::Vertex;
use crate::Result;

pub struct Context {
  fonts: FontAtlas,
  style: Style,
  input: InputState,
  active_id: Option<u64>,
  render_state: FrameOutput,
}

impl Context {
  pub fn new() -> Self {
    Self {
      fonts: FontAtlas::new(),
      style: Style::default(),
      input: InputState::default(),
      active_id: None,
      render_state: FrameOutput::default(),
    }
  }

  pub fn fonts(&mut self) -> &mut FontAtlas {
    &mut self.fonts
  }

  pub fn style(&mut self) -> &mut Style {
    &mut self.style
  }

  pub fn input(&mut self) -> &mut InputState {
    &mut self.input
  }

  pub fn begin_frame(&mut self) -> Ui {
    self.render_state = FrameOutput::default();
    Ui::new(self)
  }

  pub fn end_frame(&mut self) -> FrameOutput {
    if !self.input.mouse_buttons[0] {
      self.active_id = None;
    }
    self.render_state.clone()
  }
}

pub struct FontAtlas {
  fonts: Vec<(Font, f32, HashMap<char, Glyph>)>,
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
    let mut glyphs: HashMap<_, _> = HashMap::new();
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
          self.packer.grow(self.packer.size() * 2);
          match self.packer.allocate(size) {
            Some(a) => a,
            None => panic!("couldnt allocate glyph {:?}", c),
          }
        }
      };
      glyphs.insert(*c, (i.get(), a.rectangle.min, metrics));
    }
    let width = self.packer.size().width as f32;
    let glyphs = glyphs
      .into_iter()
      .map(|(c, (id, pos, metrics))| {
        (
          c,
          Glyph {
            id,
            pos,
            uv_min: (pos.to_f32() / width).to_array().into(),
            uv_max: ((pos + Size::new(metrics.width as _, metrics.height as _)).to_f32() / width)
              .to_array()
              .into(),
          },
        )
      })
      .collect();
    for (_, _, glyphs) in &mut self.fonts {
      for g in glyphs.values_mut() {
        g.uv_min /= uv_fac;
        g.uv_max /= uv_fac;
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
      for (_, g) in glyphs.iter() {
        let (metrics, raster) = font.rasterize_indexed(g.id, *scale);
        for y in 0..metrics.height {
          for x in 0..metrics.width {
            let px = raster[y * metrics.width + x];
            tex[(y + g.pos.y as usize) * width + x + g.pos.x as usize] = [px; 4];
          }
        }
      }
    }
    tex
  }
}

#[derive(Copy, Clone)]
struct Glyph {
  id: u16,
  pos: Point,
  uv_min: Vec2,
  uv_max: Vec2,
}

struct Text {
  glyphs: Vec<(Vec2, Vec2, Glyph)>,
  bounds: Vec2,
}

impl Text {
  fn new(ctx: &Context, bounds: Vec2, text: &str, size: f32) -> Self {
    let mut layout = Layout::new(CoordinateSystem::PositiveYDown);
    layout.reset(&LayoutSettings {
      max_width: Some(bounds.x),
      max_height: Some(bounds.y),
      ..Default::default()
    });
    let font = &ctx.fonts.fonts[0];
    layout.append(&[&font.0], &TextStyle::new(text, size, 0));

    let mut glyphs = vec![];
    for g in layout.glyphs() {
      if g.parent.is_whitespace() {
        continue;
      }
      let (glyph, width, height) = match font.2.get(&g.parent) {
        Some(glyph) => (glyph, g.width, g.height),
        None => match font.2.get(&'\u{fffd}') {
          Some(glyph) => {
            let metrics = font.0.metrics_indexed(glyph.id, size);
            (glyph, metrics.width, metrics.height)
          }
          None => continue,
        },
      };
      glyphs.push((
        Vec2::new(g.x, g.y),
        Vec2::new(width as _, height as _),
        *glyph,
      ));
    }
    let last = layout.glyphs().last().unwrap();
    Self {
      glyphs,
      bounds: Vec2::new(last.x + last.width as f32, layout.height()),
    }
  }

  fn render(&self, ctx: &mut Context, pos: Vec2) {
    for (glyph_pos, size, glyph) in &self.glyphs {
      ctx.render_state.push_rect_uv(
        pos + *glyph_pos,
        pos + *glyph_pos + *size,
        glyph.uv_min,
        glyph.uv_max,
        Vec3::ONE,
      );
    }
  }
}

pub struct Style {
  pub font_size: f32,
  pub button: Vec3,
  pub button_hovered: Vec3,
}

impl Style {
  fn default() -> Self {
    Self {
      font_size: 18.0,
      button: Vec3::splat(0.02),
      button_hovered: Vec3::splat(0.05),
    }
  }
}

#[derive(Default)]
pub struct InputState {
  pub cursor_pos: Vec2,
  pub mouse_buttons: [bool; 3],
}

impl InputState {
  fn cursor_in(&self, min: Vec2, max: Vec2) -> bool {
    self.cursor_pos.cmpge(min).all() && self.cursor_pos.cmple(max).all()
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

  fn push_rect_border(
    &mut self,
    min: Vec2,
    max: Vec2,
    thickness: f32,
    color: Vec3,
    border_color: Vec3,
  ) {
    if thickness > 0.0 {
      self.push_rect(min, max, border_color);
    }
    let thickness = Vec2::splat(thickness);
    self.push_rect(min + thickness, max - thickness, color);
  }

  fn push_rect(&mut self, min: Vec2, max: Vec2, color: Vec3) {
    self.push_rect_uv(min, max, Vec2::ZERO, Vec2::ZERO, color);
  }

  fn push_rect_uv(&mut self, min: Vec2, max: Vec2, uv_min: Vec2, uv_max: Vec2, color: Vec3) {
    self.push_indices([0, 1, 2, 1, 3, 2]);
    self.vtx_buf.extend([
      Vertex {
        pos: min,
        uv: uv_min,
        color,
      },
      Vertex {
        pos: Vec2::new(max.x, min.y),
        uv: Vec2::new(uv_max.x, uv_min.y),
        color,
      },
      Vertex {
        pos: Vec2::new(min.x, max.y),
        uv: Vec2::new(uv_min.x, uv_max.y),
        color,
      },
      Vertex {
        pos: max,
        uv: uv_max,
        color,
      },
    ]);
  }
}

pub struct Ui<'c> {
  ctx: &'c mut Context,
  origin: Vec2,
  cursor: Vec2,
  bounds: Vec2,
  last_height: f32,
  same_line: bool,
}

impl<'c> Ui<'c> {
  fn new(ctx: &'c mut Context) -> Self {
    Self {
      ctx,
      origin: Vec2::new(20.0, 20.0),
      bounds: Vec2::new(256.0, 640.0),
      cursor: Vec2::ZERO,
      last_height: 0.0,
      same_line: false,
    }
  }

  fn pre(&mut self) {
    if !self.same_line {
      self.cursor.x = self.origin.x;
      self.cursor.y += self.last_height;
    }
    self.same_line = false;
  }

  pub fn text(&mut self, text: &str) {
    self.pre();
    let text = Text::new(
      self.ctx,
      self.bounds - self.cursor,
      text,
      self.ctx.style.font_size,
    );
    text.render(self.ctx, self.origin + self.cursor);
    let bounds = text.bounds;
    self.cursor.x += bounds.x;
    self.last_height = bounds.y;
  }

  pub fn button(&mut self, label: &str) -> bool {
    self.pre();
    let id = hash_id(label);
    let text = Text::new(
      self.ctx,
      self.bounds - self.cursor,
      label,
      self.ctx.style.font_size,
    );
    let min = self.origin + self.cursor;
    let max = min + Vec2::new(self.bounds.x, text.bounds.y);
    let hovered = self.ctx.input.cursor_in(min, max);
    let active = Some(id) == self.ctx.active_id;
    self.ctx.render_state.push_rect_border(
      min,
      max,
      if active { 1.0 } else { 0.0 },
      if hovered {
        self.ctx.style.button_hovered
      } else {
        self.ctx.style.button
      },
      Vec3::ONE,
    );
    text.render(
      self.ctx,
      min + Vec2::new((self.bounds.x - text.bounds.x) / 2.0, 0.0),
    );
    if hovered {
      if self.ctx.input.mouse_buttons[0] {
        if self.ctx.active_id.is_none() {
          self.ctx.active_id = Some(id);
        }
      } else if active {
        return true;
      }
    }
    false
  }

  pub fn same_line(&mut self) {
    self.same_line = true;
  }
}

fn hash_id(s: &str) -> u64 {
  let mut h = DefaultHasher::new();
  s.hash(&mut h);
  h.finish()
}
