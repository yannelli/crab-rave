use std::fmt::Write as _;
use std::ops::Range;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Rgb(pub u8, pub u8, pub u8);

pub const BLACK: Rgb = Rgb(0, 0, 0);
pub const WHITE: Rgb = Rgb(255, 255, 255);

impl Rgb {
    pub fn mix(self, other: Rgb, amount: f32) -> Rgb {
        let amount = amount.clamp(0.0, 1.0);
        let lerp =
            |from: u8, to: u8| (from as f32 + (to as f32 - from as f32) * amount).round() as u8;
        Rgb(
            lerp(self.0, other.0),
            lerp(self.1, other.1),
            lerp(self.2, other.2),
        )
    }

    /// Adds light on top of the color, like stage lighting.
    pub fn lighten(self, light: Rgb, amount: f32) -> Rgb {
        let amount = amount.max(0.0);
        let add = |base: u8, extra: u8| (base as f32 + extra as f32 * amount).min(255.0) as u8;
        Rgb(
            add(self.0, light.0),
            add(self.1, light.1),
            add(self.2, light.2),
        )
    }

    pub fn scale(self, factor: f32) -> Rgb {
        BLACK.mix(self, factor)
    }

    pub fn invert(self) -> Rgb {
        Rgb(255 - self.0, 255 - self.1, 255 - self.2)
    }
}

pub fn hsv(hue: f32, saturation: f32, value: f32) -> Rgb {
    let sector = hue.rem_euclid(1.0) * 6.0;
    let offset = sector.fract();
    let low = value * (1.0 - saturation);
    let falling = value * (1.0 - saturation * offset);
    let rising = value * (1.0 - saturation * (1.0 - offset));
    let (red, green, blue) = match sector as u8 {
        0 => (value, rising, low),
        1 => (falling, value, low),
        2 => (low, value, rising),
        3 => (low, falling, value),
        4 => (rising, low, value),
        _ => (value, low, falling),
    };
    let channel = |level: f32| (level.clamp(0.0, 1.0) * 255.0).round() as u8;
    Rgb(channel(red), channel(green), channel(blue))
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ColorMode {
    TrueColor,
    Ansi256,
}

impl ColorMode {
    pub fn detect() -> ColorMode {
        let colorterm = std::env::var("COLORTERM").unwrap_or_default();
        if colorterm.contains("truecolor") || colorterm.contains("24bit") {
            ColorMode::TrueColor
        } else {
            ColorMode::Ansi256
        }
    }

    fn write(self, out: &mut String, layer: u8, color: Rgb) {
        let Rgb(red, green, blue) = color;
        match self {
            ColorMode::TrueColor => {
                let _ = write!(out, "\x1b[{layer};2;{red};{green};{blue}m");
            }
            ColorMode::Ansi256 => {
                let level = |channel: u8| (u16::from(channel) * 5 + 127) / 255;
                let index = 16 + 36 * level(red) + 6 * level(green) + level(blue);
                let _ = write!(out, "\x1b[{layer};5;{index}m");
            }
        }
    }
}

#[derive(Clone, Copy)]
struct Glyph {
    ch: char,
    color: Rgb,
    wide: bool,
}

/// A pixel buffer drawn with half blocks, so each terminal cell holds two square-ish pixels,
/// plus a layer of text glyphs drawn over the pixels.
pub struct Canvas {
    pub width: i32,
    pub height: i32,
    pixels: Vec<Rgb>,
    glyphs: Vec<Option<Glyph>>,
}

impl Canvas {
    pub fn new(columns: u16, rows: u16) -> Canvas {
        let width = i32::from(columns);
        let rows = i32::from(rows);
        Canvas {
            width,
            height: rows * 2,
            pixels: vec![BLACK; (width * rows * 2) as usize],
            glyphs: vec![None; (width * rows) as usize],
        }
    }

    pub fn rows(&self) -> i32 {
        self.height / 2
    }

    fn index(&self, x: i32, y: i32) -> Option<usize> {
        (x >= 0 && y >= 0 && x < self.width && y < self.height)
            .then(|| (y * self.width + x) as usize)
    }

    fn get(&self, x: i32, y: i32) -> Rgb {
        self.index(x, y).map_or(BLACK, |index| self.pixels[index])
    }

    pub fn set(&mut self, x: i32, y: i32, color: Rgb) {
        if let Some(index) = self.index(x, y) {
            self.pixels[index] = color;
        }
    }

    pub fn light(&mut self, x: i32, y: i32, color: Rgb, amount: f32) {
        if let Some(index) = self.index(x, y) {
            self.pixels[index] = self.pixels[index].lighten(color, amount);
        }
    }

    /// Recolors every pixel in the given rows.
    pub fn shade(&mut self, rows: Range<i32>, mut shader: impl FnMut(i32, i32, Rgb) -> Rgb) {
        for y in rows.start.max(0)..rows.end.min(self.height) {
            for x in 0..self.width {
                let index = (y * self.width + x) as usize;
                self.pixels[index] = shader(x, y, self.pixels[index]);
            }
        }
    }

    /// Fills every pixel within `radius` of the segment from `start` to `end`.
    pub fn fill_capsule(&mut self, start: (f32, f32), end: (f32, f32), radius: f32, color: Rgb) {
        let radius = radius.max(0.5);
        let (dx, dy) = (end.0 - start.0, end.1 - start.1);
        let length_squared = dx * dx + dy * dy;
        let columns = (start.0.min(end.0) - radius).floor() as i32
            ..(start.0.max(end.0) + radius).ceil() as i32;
        let rows = (start.1.min(end.1) - radius).floor() as i32
            ..(start.1.max(end.1) + radius).ceil() as i32;
        for y in rows.start.max(0)..rows.end.min(self.height) {
            for x in columns.start.max(0)..columns.end.min(self.width) {
                let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                let along = if length_squared > 0.0 {
                    (((px - start.0) * dx + (py - start.1) * dy) / length_squared).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let (gap_x, gap_y) = (start.0 + dx * along - px, start.1 + dy * along - py);
                if gap_x * gap_x + gap_y * gap_y <= radius * radius {
                    self.set(x, y, color);
                }
            }
        }
    }

    /// Fills an ellipse with a vertical gradient from `top` to `bottom`.
    pub fn fill_ellipse(&mut self, center: (f32, f32), radius: (f32, f32), top: Rgb, bottom: Rgb) {
        let (cx, cy) = center;
        let (rx, ry) = (radius.0.max(0.5), radius.1.max(0.5));
        for y in ((cy - ry).floor() as i32).max(0)..((cy + ry).ceil() as i32).min(self.height) {
            for x in ((cx - rx).floor() as i32).max(0)..((cx + rx).ceil() as i32).min(self.width) {
                let nx = (x as f32 + 0.5 - cx) / rx;
                let ny = (y as f32 + 0.5 - cy) / ry;
                if nx * nx + ny * ny <= 1.0 {
                    self.set(x, y, top.mix(bottom, (ny + 1.0) / 2.0));
                }
            }
        }
    }

    pub fn put_glyph(&mut self, column: i32, row: i32, ch: char, color: Rgb) {
        self.place(
            column,
            row,
            Glyph {
                ch,
                color,
                wide: false,
            },
        );
    }

    pub fn put_wide_glyph(&mut self, column: i32, row: i32, ch: char, color: Rgb) {
        if column + 1 < self.width {
            self.place(
                column,
                row,
                Glyph {
                    ch,
                    color,
                    wide: true,
                },
            );
        }
    }

    fn place(&mut self, column: i32, row: i32, glyph: Glyph) {
        if column < 0 || row < 0 || column >= self.width || row >= self.rows() {
            return;
        }
        let index = (row * self.width + column) as usize;
        // A double-width glyph covers the cell to its right, so the two cannot share a row slot.
        if column > 0 && self.glyphs[index - 1].is_some_and(|left| left.wide) {
            self.glyphs[index - 1] = None;
        }
        if glyph.wide {
            self.glyphs[index + 1] = None;
        }
        self.glyphs[index] = Some(glyph);
    }

    pub fn encode(&self, out: &mut String, mode: ColorMode, shake: (i32, i32)) {
        let (shake_x, shake_y) = shake;
        let mut fg = None;
        let mut bg = None;
        for row in 0..self.rows() {
            let _ = write!(out, "\x1b[{};1H", row + 1);
            let mut column = 0;
            while column < self.width {
                let top = self.get(column - shake_x, row * 2 - shake_y);
                let bottom = self.get(column - shake_x, row * 2 + 1 - shake_y);
                let glyph = self.glyphs[(row * self.width + column) as usize];
                let (ch, cell_fg, cell_bg) = match glyph {
                    Some(glyph) => (glyph.ch, glyph.color, top.mix(bottom, 0.5)),
                    None if top == bottom => (' ', top, top),
                    None => ('▀', top, bottom),
                };
                if bg != Some(cell_bg) {
                    mode.write(out, 48, cell_bg);
                    bg = Some(cell_bg);
                }
                if ch != ' ' && fg != Some(cell_fg) {
                    mode.write(out, 38, cell_fg);
                    fg = Some(cell_fg);
                }
                out.push(ch);
                column += if glyph.is_some_and(|glyph| glyph.wide) {
                    2
                } else {
                    1
                };
            }
        }
    }
}
