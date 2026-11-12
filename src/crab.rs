use std::f32::consts::PI;

use crate::canvas::{Canvas, Rgb, WHITE};

const SHELL_TOP: Rgb = Rgb(255, 118, 66);
const SHELL_BOTTOM: Rgb = Rgb(190, 30, 34);
const HIGHLIGHT: Rgb = Rgb(255, 190, 150);
const CLAW: Rgb = Rgb(255, 92, 54);
const LEG: Rgb = Rgb(212, 52, 40);
const BLUSH: Rgb = Rgb(255, 140, 165);
const PUPIL: Rgb = Rgb(24, 8, 32);
const OUTLINE: Rgb = Rgb(46, 0, 18);
const SILHOUETTE: Rgb = Rgb(16, 0, 28);
const RIM_LIGHT: Rgb = Rgb(120, 70, 190);

/// Arm angles in radians from horizontal, pointing away from the body. Positive is up.
#[derive(Clone, Copy)]
pub struct Arm {
    shoulder: f32,
    elbow: f32,
}

const DOWN: Arm = Arm {
    shoulder: -0.35,
    elbow: 0.25,
};
const OUT: Arm = Arm {
    shoulder: 0.25,
    elbow: 0.85,
};
const HIGH: Arm = Arm {
    shoulder: 0.75,
    elbow: 1.3,
};
const UP: Arm = Arm {
    shoulder: 1.1,
    elbow: 1.75,
};
const CLAP: Arm = Arm {
    shoulder: 1.25,
    elbow: 2.6,
};

impl Arm {
    fn toward(self, target: Arm, amount: f32) -> Arm {
        Arm {
            shoulder: self.shoulder + (target.shoulder - self.shoulder) * amount,
            elbow: self.elbow + (target.elbow - self.elbow) * amount,
        }
    }
}

pub struct Pose {
    left: Arm,
    right: Arm,
    /// How far the pincers are open, from 0 to 1.
    pinch: f32,
    /// Positive flattens the body, negative stretches it.
    squash: f32,
    lean: f32,
    hop: f32,
    /// Pupil direction, from -1 (left) to 1 (right).
    look: f32,
    blink: bool,
    scuttle: f32,
}

impl Pose {
    /// Crouches and slowly raises the claws while the drop builds.
    pub fn build_up(beats: f32, progress: f32) -> Pose {
        let rise = progress * progress;
        Pose {
            left: DOWN.toward(UP, rise),
            right: DOWN.toward(UP, rise),
            pinch: 0.3 + 0.7 * (beats * PI).sin().abs(),
            squash: 0.5 * rise,
            lean: 0.0,
            hop: 0.0,
            look: (beats * PI / 2.0).sin(),
            blink: false,
            scuttle: 0.0,
        }
    }

    /// Switches between four routines every two bars and hits a new pose on every beat.
    pub fn dance(beats: f32) -> Pose {
        let beat = beats.floor() as i64;
        let phase = beats.fract();
        let kick = (-phase * 6.0).exp();
        let ease = smoothstep(phase / 0.3);
        let (from_left, from_right) = arms_for(beat - 1);
        let (to_left, to_right) = arms_for(beat);
        let shuffling = routine(beat) == 2;
        let landing = ((phase - 0.8) / 0.2).clamp(0.0, 1.0);
        Pose {
            left: from_left.toward(to_left, ease),
            right: from_right.toward(to_right, ease),
            pinch: 1.0 - kick,
            squash: 0.5 * landing - 0.35 * kick,
            lean: if shuffling {
                (beats * PI / 2.0).sin() * 5.0
            } else {
                0.0
            },
            hop: kick * if routine(beat) == 3 { 2.6 } else { 1.5 },
            look: if shuffling {
                (beats * PI / 2.0).cos()
            } else {
                (beats * PI / 4.0).sin() * 0.6
            },
            blink: (beats / 5.0).fract() > 0.95,
            scuttle: beats * PI * if shuffling { 3.0 } else { 1.0 },
        }
    }
}

fn routine(beat: i64) -> i64 {
    beat.div_euclid(8).rem_euclid(4)
}

fn arms_for(beat: i64) -> (Arm, Arm) {
    let even = beat.rem_euclid(2) == 0;
    match (routine(beat), even) {
        (0, true) => (UP, UP),
        (0, false) => (OUT, OUT),
        (1, true) => (UP, DOWN),
        (1, false) => (DOWN, UP),
        (2, true) => (HIGH, OUT),
        (2, false) => (OUT, HIGH),
        (_, true) => (CLAP, CLAP),
        (_, false) => (HIGH, HIGH),
    }
}

fn smoothstep(value: f32) -> f32 {
    let value = value.clamp(0.0, 1.0);
    value * value * (3.0 - 2.0 * value)
}

enum Shape {
    Capsule {
        start: (f32, f32),
        end: (f32, f32),
        radius: f32,
        color: Rgb,
    },
    Ellipse {
        center: (f32, f32),
        radius: (f32, f32),
        top: Rgb,
        bottom: Rgb,
    },
}

struct Part {
    shape: Shape,
    outlined: bool,
    /// Keeps its color when the crab is drawn as a silhouette.
    glows: bool,
}

impl Part {
    fn new(shape: Shape) -> Part {
        Part {
            shape,
            outlined: true,
            glows: false,
        }
    }

    fn detail(shape: Shape) -> Part {
        Part {
            shape,
            outlined: false,
            glows: false,
        }
    }

    fn draw(&self, canvas: &mut Canvas, grow: f32, paint: &dyn Fn(Rgb) -> Rgb) {
        match self.shape {
            Shape::Capsule {
                start,
                end,
                radius,
                color,
            } => canvas.fill_capsule(start, end, radius + grow, paint(color)),
            Shape::Ellipse {
                center,
                radius,
                top,
                bottom,
            } => canvas.fill_ellipse(
                center,
                (radius.0 + grow, radius.1 + grow),
                paint(top),
                paint(bottom),
            ),
        }
    }
}

fn capsule(start: (f32, f32), end: (f32, f32), radius: f32, color: Rgb) -> Shape {
    Shape::Capsule {
        start,
        end,
        radius,
        color,
    }
}

fn circle(center: (f32, f32), radius: f32, color: Rgb) -> Shape {
    capsule(center, center, radius, color)
}

/// Moves `length` from `from` at `angle`, mirrored by `side` (-1 is the crab's left).
fn reach(from: (f32, f32), side: f32, angle: f32, length: f32) -> (f32, f32) {
    (
        from.0 + side * angle.cos() * length,
        from.1 - angle.sin() * length,
    )
}

/// A procedurally drawn crab. `size` is pixels per crab unit; the crab is about 50 units wide
/// with its claws out and 30 units tall with its claws up.
pub struct Crab {
    pub x: f32,
    pub ground: f32,
    pub size: f32,
    pub silhouette: bool,
    pub glow: f32,
}

impl Crab {
    pub fn draw(&self, canvas: &mut Canvas, pose: &Pose) {
        let unit = self.size;
        let rx = 9.0 * (1.0 + 0.12 * pose.squash) * unit;
        let ry = 5.5 * (1.0 - 0.18 * pose.squash) * unit;
        let feet = self.ground - pose.hop * unit;
        let cx = self.x + pose.lean * unit;
        let cy = feet - 4.5 * unit - ry;
        let mut parts = Vec::new();

        for side in [-1.0, 1.0] {
            for leg in 0..3 {
                let leg = leg as f32;
                let step = (pose.scuttle + leg * 2.1 + side).sin();
                let hip = (cx + side * (3.5 + leg * 2.0) * unit, cy + 2.0 * unit);
                let knee = (
                    cx + side * (10.0 + leg * 2.2) * unit,
                    cy - (0.5 + step.max(0.0) * 1.2) * unit,
                );
                let foot = (
                    cx + side * (12.5 + leg * 2.8) * unit + step * 1.2 * unit,
                    feet,
                );
                parts.push(Part::new(capsule(hip, knee, 0.8 * unit, LEG)));
                parts.push(Part::new(capsule(knee, foot, 0.65 * unit, LEG)));
            }
            let stalk_base = (cx + side * 2.8 * unit, cy - ry * 0.6);
            let stalk_top = (cx + side * 3.8 * unit, cy - ry - 3.0 * unit);
            parts.push(Part::new(capsule(stalk_base, stalk_top, 0.6 * unit, LEG)));
        }

        for (side, arm) in [(-1.0, pose.left), (1.0, pose.right)] {
            let shoulder = (cx + side * (rx - 2.0 * unit), cy - unit);
            let elbow = reach(shoulder, side, arm.shoulder, 6.0 * unit);
            let wrist = reach(elbow, side, arm.elbow, 5.0 * unit);
            let palm = reach(wrist, side, arm.elbow, 1.2 * unit);
            let opening = 0.12 + 0.42 * pose.pinch;
            parts.push(Part::new(capsule(shoulder, elbow, 1.1 * unit, CLAW)));
            parts.push(Part::new(capsule(elbow, wrist, unit, CLAW)));
            let upper_tip = reach(palm, side, arm.elbow + opening, 4.2 * unit);
            let lower_tip = reach(palm, side, arm.elbow - opening, 3.6 * unit);
            parts.push(Part::new(capsule(palm, upper_tip, 1.15 * unit, CLAW)));
            parts.push(Part::new(capsule(palm, lower_tip, 0.95 * unit, CLAW)));
            parts.push(Part::new(circle(palm, 2.6 * unit, CLAW)));
        }

        parts.push(Part::new(Shape::Ellipse {
            center: (cx, cy),
            radius: (rx, ry),
            top: SHELL_TOP,
            bottom: SHELL_BOTTOM,
        }));
        parts.push(Part::detail(Shape::Ellipse {
            center: (cx - 3.2 * unit, cy - ry * 0.5),
            radius: (2.8 * unit, 1.1 * unit),
            top: HIGHLIGHT,
            bottom: SHELL_TOP,
        }));
        let smile = |step: f32| {
            let across = step / 3.0;
            (
                cx + step * 0.6 * unit,
                cy + (1.0 + (1.0 - across * across)) * unit,
            )
        };
        for step in -3..3 {
            let step = step as f32;
            parts.push(Part::detail(capsule(
                smile(step),
                smile(step + 1.0),
                0.35 * unit,
                OUTLINE,
            )));
        }

        for side in [-1.0, 1.0] {
            parts.push(Part::detail(circle(
                (cx + side * 5.2 * unit, cy + 1.2 * unit),
                unit,
                BLUSH,
            )));
            let eye = (cx + side * 3.8 * unit, cy - ry - 3.0 * unit);
            if pose.blink {
                let lid = (1.6 * unit, 0.0);
                parts.push(Part::detail(capsule(
                    (eye.0 - lid.0, eye.1),
                    (eye.0 + lid.0, eye.1),
                    0.45 * unit,
                    OUTLINE,
                )));
            } else {
                let pupil = (eye.0 + pose.look * 0.8 * unit, eye.1 + 0.2 * unit);
                parts.push(Part {
                    shape: circle(eye, 2.0 * unit, WHITE),
                    outlined: true,
                    glows: true,
                });
                parts.push(Part {
                    shape: circle(pupil, unit, PUPIL),
                    outlined: false,
                    glows: true,
                });
            }
        }

        let outline = (0.55 * unit).max(0.8);
        let edge = if self.silhouette { RIM_LIGHT } else { OUTLINE };
        for part in parts.iter().filter(|part| part.outlined) {
            part.draw(canvas, outline, &|_| edge);
        }
        for part in &parts {
            let fill = |color: Rgb| match (self.silhouette, part.glows) {
                (true, false) => SILHOUETTE,
                _ => color.mix(WHITE, self.glow),
            };
            part.draw(canvas, 0.0, &fill);
        }
    }
}
