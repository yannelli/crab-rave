use std::f32::consts::{PI, TAU};
use std::fmt::Write as _;

use crate::art::{self, BANNER, GLYPH_HEIGHT};
use crate::canvas::{Canvas, Rgb, WHITE, hsv};
use crate::crab::{Crab, Pose};

pub const BEAT_SECONDS: f32 = 60.0 / 125.0;
pub const INTRO_BEATS: f32 = 4.0;
const CRAB_EMOJI: char = '🦀';
const CONFETTI: [char; 8] = ['*', '+', '·', '•', '✦', '✧', '♦', '°'];
const HINT: &str = "press any key to stop";
const BANNER_OUTLINE: Rgb = Rgb(20, 0, 34);

struct Clock {
    time: f32,
    beats: f32,
    beat: i64,
    phase: f32,
    /// 1 on each beat, decaying until the next one. Speeds up during the build-up.
    kick: f32,
    raving: bool,
    /// Beats since the drop. Negative during the build-up.
    since_drop: f32,
    /// Build-up progress from 0 to 1.
    progress: f32,
    hue: f32,
}

impl Clock {
    fn at(time: f32) -> Clock {
        let beats = time / BEAT_SECONDS;
        let raving = beats >= INTRO_BEATS;
        let progress = (beats / INTRO_BEATS).min(1.0);
        let pulses_per_beat = if raving || progress < 0.5 {
            1.0
        } else if progress < 0.75 {
            2.0
        } else {
            4.0
        };
        Clock {
            time,
            beats,
            beat: beats.floor() as i64,
            phase: beats.fract(),
            kick: (-(beats * pulses_per_beat).fract() * 6.0).exp(),
            raving,
            since_drop: beats - INTRO_BEATS,
            progress,
            hue: time * 0.05,
        }
    }

    fn just_dropped(&self, beats: f32) -> bool {
        (0.0..beats).contains(&self.since_drop)
    }
}

/// Positions in canvas pixels.
struct Layout {
    banner_scale: i32,
    banner_top: i32,
    horizon: i32,
    sun_radius: f32,
    lead_ground: f32,
    lead_size: f32,
    backup_ground: f32,
}

impl Layout {
    fn new(canvas: &Canvas) -> Layout {
        let (width, height) = (canvas.width as f32, canvas.height as f32);
        let banner_scale = (canvas.width - 4) / art::banner_width();
        let banner_scale = banner_scale.clamp(0, 4);
        let banner_top = 2;
        let banner_bottom = (banner_top + GLYPH_HEIGHT * banner_scale) as f32;
        let horizon = (height * 0.62) as i32;
        let lead_ground = height - 3.0;
        let lead_size = ((lead_ground - banner_bottom - 3.0) / 30.0)
            .min(width * 0.55 / 50.0)
            .max(0.3);
        Layout {
            banner_scale,
            banner_top,
            horizon,
            sun_radius: ((horizon as f32 - banner_bottom) * 0.85)
                .min(width * 0.22)
                .max(4.0),
            lead_ground,
            lead_size,
            backup_ground: horizon as f32 + (lead_ground - horizon as f32) * 0.3,
        }
    }
}

struct Particle {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    hue: f32,
    ch: char,
}

pub struct Rave {
    rng: Rng,
    particles: Vec<Particle>,
    last_time: f32,
    burst_done: bool,
    last_effect_beat: i64,
}

impl Rave {
    pub fn new(seed: u64) -> Rave {
        Rave {
            rng: Rng(seed | 1),
            particles: Vec::new(),
            last_time: 0.0,
            burst_done: false,
            last_effect_beat: -1,
        }
    }

    /// Draws one frame and returns the screen shake offset in pixels.
    pub fn draw(&mut self, canvas: &mut Canvas, time: f32) -> (i32, i32) {
        let elapsed = (time - self.last_time).clamp(0.0, 0.1);
        self.last_time = time;
        let clock = Clock::at(time);
        let layout = Layout::new(canvas);
        let center_x = canvas.width as f32 / 2.0;

        draw_sky(canvas, &clock, &layout);
        draw_sun(canvas, &clock, &layout);
        draw_floor(canvas, &clock, &layout);
        if clock.raving {
            draw_beams(canvas, &clock);
            draw_shockwaves(
                canvas,
                &clock,
                (center_x, layout.lead_ground - 14.0 * layout.lead_size),
            );
        } else {
            draw_spotlight(canvas, &clock);
        }

        let backup_pose = if clock.raving {
            Pose::dance(clock.beats - 0.25)
        } else {
            Pose::build_up(clock.beats, clock.progress)
        };
        for offset in [-0.36, 0.36] {
            let backup = Crab {
                x: center_x + canvas.width as f32 * offset,
                ground: layout.backup_ground,
                size: layout.lead_size * 0.45,
                silhouette: !clock.raving,
                glow: 0.0,
            };
            backup.draw(canvas, &backup_pose);
        }
        let lead_pose = if clock.raving {
            Pose::dance(clock.beats)
        } else {
            Pose::build_up(clock.beats, clock.progress)
        };
        let lead = Crab {
            x: center_x,
            ground: layout.lead_ground,
            size: layout.lead_size,
            silhouette: !clock.raving,
            glow: if clock.raving { 0.25 * clock.kick } else { 0.0 },
        };
        lead.draw(canvas, &lead_pose);

        draw_banner(canvas, &clock, &layout);
        post_effects(canvas, &clock);

        let burst_origin = (
            center_x,
            (layout.lead_ground - 14.0 * layout.lead_size) / 2.0,
        );
        self.update_particles(&clock, elapsed, canvas, burst_origin);
        self.draw_particles(canvas, &clock);
        if time < 4.0 {
            let column = canvas.width - HINT.len() as i32 - 1;
            for (offset, ch) in HINT.chars().enumerate() {
                canvas.put_glyph(
                    column + offset as i32,
                    canvas.rows() - 1,
                    ch,
                    Rgb(190, 180, 220),
                );
            }
        }
        self.shake(&clock)
    }

    /// iTerm2 only: colors the tab on every beat and sets off fireworks every two bars.
    pub fn iterm_effects(&mut self, out: &mut String, time: f32, columns: u16, rows: u16) {
        let clock = Clock::at(time);
        if !clock.raving || clock.beat == self.last_effect_beat {
            return;
        }
        self.last_effect_beat = clock.beat;
        let Rgb(red, green, blue) = hsv(clock.hue, 0.9, 1.0);
        let _ = write!(
            out,
            "\x1b]6;1;bg;red;brightness;{red}\x07\x1b]6;1;bg;green;brightness;{green}\x07\x1b]6;1;bg;blue;brightness;{blue}\x07"
        );
        if (clock.beat - INTRO_BEATS as i64) % 8 == 0 {
            let column = self.rng.range(0.2, 0.8) * f32::from(columns);
            let row = self.rng.range(0.2, 0.6) * f32::from(rows);
            let _ = write!(
                out,
                "\x1b[{};{}H\x1b]1337;RequestAttention=fireworks\x07",
                row as u16 + 1,
                column as u16 + 1
            );
        }
    }

    fn shake(&mut self, clock: &Clock) -> (i32, i32) {
        let amplitude = if clock.just_dropped(2.0) {
            4.0 * (1.0 - clock.since_drop / 2.0)
        } else if clock.raving {
            1.3 * clock.kick
        } else {
            1.5 * clock.progress.powi(3)
        };
        let mut offset = || (self.rng.range(-1.0, 1.0) * amplitude).round() as i32;
        (offset(), offset())
    }

    /// Particles live in cell coordinates: x in columns, y in rows.
    fn update_particles(
        &mut self,
        clock: &Clock,
        elapsed: f32,
        canvas: &Canvas,
        burst_origin: (f32, f32),
    ) {
        let (columns, rows) = (canvas.width as f32, canvas.rows() as f32);
        if clock.raving && !self.burst_done {
            self.burst_done = true;
            for _ in 0..canvas.width * 2 {
                let angle = self.rng.range(0.0, TAU);
                let speed = self.rng.range(10.0, 45.0);
                let particle = self.particle(
                    burst_origin.0,
                    burst_origin.1,
                    angle.cos() * speed,
                    angle.sin() * speed * 0.5 - 8.0,
                );
                self.particles.push(particle);
            }
        }
        if clock.raving {
            let expected = columns * 0.4 * elapsed;
            let count = expected as usize + usize::from(self.rng.next_f32() < expected.fract());
            for _ in 0..count {
                let (x, vx, vy) = (
                    self.rng.range(0.0, columns),
                    self.rng.range(-2.0, 2.0),
                    self.rng.range(3.0, 8.0),
                );
                let particle = self.particle(x, -1.0, vx, vy);
                self.particles.push(particle);
            }
        }
        let drag = (1.0 - 1.5 * elapsed).max(0.0);
        for particle in &mut self.particles {
            particle.vx *= drag;
            particle.vy = (particle.vy + 9.0 * elapsed).min(12.0);
            particle.x += particle.vx * elapsed;
            particle.y += particle.vy * elapsed;
        }
        self.particles
            .retain(|particle| particle.y < rows && (0.0..columns).contains(&particle.x));
    }

    fn particle(&mut self, x: f32, y: f32, vx: f32, vy: f32) -> Particle {
        let ch = if self.rng.next_f32() < 0.05 {
            CRAB_EMOJI
        } else {
            CONFETTI[self.rng.below(CONFETTI.len())]
        };
        Particle {
            x,
            y,
            vx,
            vy,
            hue: self.rng.next_f32(),
            ch,
        }
    }

    fn draw_particles(&self, canvas: &mut Canvas, clock: &Clock) {
        for particle in &self.particles {
            let (column, row) = (particle.x.floor() as i32, particle.y.floor() as i32);
            if particle.ch == CRAB_EMOJI {
                canvas.put_wide_glyph(column, row, particle.ch, WHITE);
            } else {
                canvas.put_glyph(
                    column,
                    row,
                    particle.ch,
                    hsv(particle.hue + clock.time * 0.3, 0.55, 1.0),
                );
            }
        }
    }
}

fn draw_sky(canvas: &mut Canvas, clock: &Clock, layout: &Layout) {
    let horizon = layout.horizon as f32;
    let glow = if clock.raving { 0.3 * clock.kick } else { 0.0 };
    let lights = if clock.raving {
        1.0
    } else {
        0.25 + 0.75 * clock.progress * clock.progress
    };
    canvas.shade(0..layout.horizon, |x, y, _| {
        let depth = y as f32 / horizon;
        let color = Rgb(6, 0, 20)
            .mix(Rgb(96, 10, 116), depth.powf(1.6))
            .mix(hsv(clock.hue + depth * 0.2, 0.9, 0.9), glow * depth)
            .scale(lights);
        let star = hash(x, y);
        if star < 0.006 {
            let twinkle = 0.5 + 0.5 * (clock.time * 3.0 + star * 5000.0).sin();
            color.lighten(WHITE, twinkle * 0.8 * (1.0 - depth))
        } else {
            color
        }
    });
}

fn draw_sun(canvas: &mut Canvas, clock: &Clock, layout: &Layout) {
    let radius = layout.sun_radius * (1.0 + 0.05 * clock.kick);
    let center = (
        canvas.width as f32 / 2.0,
        layout.horizon as f32 + radius * (1.0 - clock.progress),
    );
    canvas.shade(0..layout.horizon, |x, y, color| {
        let distance = (x as f32 + 0.5 - center.0).hypot(y as f32 + 0.5 - center.1);
        let height = (y as f32 + 0.5 - (center.1 - radius)) / radius;
        let stripe =
            height > 0.5 && (height * 10.0 - clock.time * 0.6).fract() < (height - 0.5) * 0.9;
        if distance < radius && !stripe {
            Rgb(255, 228, 92).mix(Rgb(255, 40, 150), height)
        } else {
            let halo = 1.0 - (distance - radius).max(0.0) / (radius * 0.4);
            color.lighten(Rgb(255, 50, 150), 0.35 * halo.max(0.0))
        }
    });
}

fn draw_floor(canvas: &mut Canvas, clock: &Clock, layout: &Layout) {
    let horizon = layout.horizon as f32;
    let center = canvas.width as f32 / 2.0;
    let span = canvas.height as f32 - horizon;
    let reflection_width = canvas.width as f32 * 0.12;
    let brightness = if clock.raving {
        0.6 + 0.4 * clock.kick
    } else {
        0.2 + 0.4 * clock.progress
    };
    let row_color = hsv(clock.hue + 0.85, 0.85, 1.0);
    let column_color = hsv(clock.hue + 0.5, 0.8, 1.0);
    // Perspective grid: depth is inversely proportional to the distance below the horizon.
    let depth = |below: f32| 40.0 / below + clock.beats;
    let lane = |x: f32, below: f32| (x - center) / below * 6.0;
    canvas.shade(layout.horizon..canvas.height, |x, y, _| {
        let below = y as f32 - horizon + 1.0;
        let fog = (below / span).powf(0.7);
        // Lines closer than a few pixels apart blur into noise, so they fade in with distance.
        let row_fade = ((below - 8.0) / 8.0).clamp(0.0, 1.0);
        let column_fade = ((below - 12.0) / 12.0).clamp(0.0, 1.0);
        let mut color = Rgb(14, 0, 30).mix(Rgb(42, 0, 62), fog);
        if depth(below).floor() != depth(below + 1.0).floor() {
            color = color.lighten(row_color, brightness * fog * row_fade);
        }
        if lane(x as f32, below).floor() != lane(x as f32 + 1.0, below).floor() {
            color = color.lighten(column_color, 0.8 * brightness * fog * column_fade);
        }
        let reflection = (1.0 - (x as f32 + 0.5 - center).abs() / reflection_width).max(0.0);
        color.lighten(
            Rgb(255, 60, 150),
            0.5 * reflection * (1.0 - fog) * clock.progress,
        )
    });
}

fn draw_beams(canvas: &mut Canvas, clock: &Clock) {
    const HALF_WIDTH: f32 = 0.045;
    let strength = 0.2 + 0.25 * clock.kick;
    let width = canvas.width as f32;
    for (source, (origin_x, side)) in [(0.0, 1.0), (width, -1.0)].into_iter().enumerate() {
        let sweep = (clock.time * 1.2 + source as f32 * PI).sin() * 0.35;
        let angles = [0.35, 0.7, 1.05].map(|angle| angle + sweep);
        let color = hsv(clock.hue + 0.1 + source as f32 * 0.5, 0.8, 1.0);
        canvas.shade(0..canvas.height, |x, y, mut pixel| {
            let angle = (y as f32 + 0.5).atan2((x as f32 + 0.5 - origin_x) * side);
            for beam in angles {
                let distance = (angle - beam).abs();
                if distance < HALF_WIDTH {
                    pixel = pixel.lighten(color, strength * (1.0 - distance / HALF_WIDTH));
                }
            }
            pixel
        });
    }
}

fn draw_spotlight(canvas: &mut Canvas, clock: &Clock) {
    let center = canvas.width as f32 / 2.0;
    let strength = (0.1 + 0.25 * clock.kick) * clock.progress;
    canvas.shade(0..canvas.height, |x, y, pixel| {
        let angle = (x as f32 + 0.5 - center).atan2(y as f32 + 0.5).abs();
        pixel.lighten(WHITE, strength * (1.0 - angle / 0.2).max(0.0))
    });
}

fn draw_shockwaves(canvas: &mut Canvas, clock: &Clock, center: (f32, f32)) {
    let max_radius = canvas.width.max(canvas.height) as f32 * 0.7;
    let mut rings = vec![(clock.phase, 2.0, hsv(clock.hue + 0.3, 0.6, 1.0), 0.5)];
    if clock.just_dropped(2.0) {
        rings.push((clock.since_drop / 2.0, 5.0, WHITE, 0.9));
    }
    canvas.shade(0..canvas.height, |x, y, mut pixel| {
        let distance = (x as f32 + 0.5 - center.0).hypot(y as f32 + 0.5 - center.1);
        for &(age, thickness, color, strength) in &rings {
            let gap = (distance - age * max_radius).abs();
            if gap < thickness {
                pixel = pixel.lighten(color, strength * (1.0 - age) * (1.0 - gap / thickness));
            }
        }
        pixel
    });
}

fn draw_banner(canvas: &mut Canvas, clock: &Clock, layout: &Layout) {
    let scale = layout.banner_scale;
    if scale == 0 {
        return;
    }
    let left = (canvas.width - art::banner_width() * scale) / 2;
    let top = layout.banner_top
        - if clock.raving {
            (clock.kick * scale as f32).round() as i32
        } else {
            0
        };
    let visible_letters = if clock.raving {
        usize::MAX
    } else {
        (clock.beats * 2.0) as usize + 1
    };

    let mut pixels: Vec<(i32, i32, usize)> = Vec::new();
    let mut column = 0;
    let mut letter = 0;
    for ch in BANNER.chars() {
        let glyph = art::glyph(ch);
        if ch != ' ' {
            if letter < visible_letters {
                for (row, line) in glyph.iter().enumerate() {
                    for (offset, _) in line.bytes().enumerate().filter(|&(_, byte)| byte == b'#') {
                        for (dx, dy) in (0..scale).flat_map(|dx| (0..scale).map(move |dy| (dx, dy)))
                        {
                            let x = left + (column + offset as i32) * scale + dx;
                            pixels.push((x, top + row as i32 * scale + dy, letter));
                        }
                    }
                }
            }
            letter += 1;
        }
        column += glyph[0].len() as i32 + 1;
    }

    for &(x, y, _) in &pixels {
        for (dx, dy) in [
            (-1, -1),
            (0, -1),
            (1, -1),
            (-1, 0),
            (1, 0),
            (-1, 1),
            (0, 1),
            (1, 1),
            (1, 2),
            (2, 2),
        ] {
            canvas.set(x + dx, y + dy, BANNER_OUTLINE);
        }
    }
    let drop_split = if clock.just_dropped(1.0) {
        (1.0 - clock.since_drop) * 8.0
    } else {
        0.0
    };
    let split = if clock.raving {
        ((clock.kick * 2.0 + drop_split) * scale as f32).round() as i32
    } else {
        0
    };
    if split > 0 {
        for &(x, y, _) in &pixels {
            canvas.light(x - split, y, Rgb(255, 0, 120), 0.8);
            canvas.light(x + split, y, Rgb(0, 220, 255), 0.8);
        }
    }
    let banner_height = (GLYPH_HEIGHT * scale) as f32;
    let newest = (clock.beats * 2.0) as usize;
    for &(x, y, letter) in &pixels {
        let shine = (1.0 - 2.0 * (y - top) as f32 / banner_height).max(0.0) * 0.6;
        let color = if clock.raving {
            hsv(clock.hue + x as f32 / canvas.width as f32 * 0.5, 0.75, 1.0).mix(WHITE, shine)
        } else {
            let flash = if letter == newest { clock.kick } else { 0.0 };
            Rgb(170, 150, 210).mix(WHITE, flash)
        };
        canvas.set(x, y, color);
    }
}

fn post_effects(canvas: &mut Canvas, clock: &Clock) {
    if clock.just_dropped(0.75) {
        let fade = 1.0 - clock.since_drop / 0.75;
        if (clock.time * 16.0) as i64 % 2 == 0 {
            canvas.shade(0..canvas.height, |_, _, pixel| {
                pixel.mix(WHITE, 0.85 * fade)
            });
        } else {
            canvas.shade(0..canvas.height, |_, _, pixel| pixel.invert());
        }
    } else if clock.raving && clock.beat % 2 == 1 {
        canvas.shade(0..canvas.height, |_, _, pixel| {
            pixel.mix(WHITE, 0.12 * clock.kick)
        });
    } else if !clock.raving {
        canvas.shade(0..canvas.height, |_, _, pixel| {
            pixel.lighten(WHITE, 0.12 * clock.kick * clock.progress)
        });
    }
}

fn hash(x: i32, y: i32) -> f32 {
    let mut state = (x as u32).wrapping_mul(374_761_393) ^ (y as u32).wrapping_mul(668_265_263);
    state = (state ^ (state >> 13)).wrapping_mul(1_274_126_177);
    (state ^ (state >> 16)) as f32 / u32::MAX as f32
}

struct Rng(u64);

impl Rng {
    fn next_u64(&mut self) -> u64 {
        let mut state = self.0;
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        self.0 = state;
        state
    }

    fn next_f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }

    fn range(&mut self, low: f32, high: f32) -> f32 {
        low + (high - low) * self.next_f32()
    }

    fn below(&mut self, limit: usize) -> usize {
        (self.next_u64() % limit as u64) as usize
    }
}
