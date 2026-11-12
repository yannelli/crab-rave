use std::f64::consts::TAU;
use std::num::NonZero;
use std::time::Duration;

use rodio::cpal::traits::{DeviceTrait, HostTrait};
use rodio::cpal::{self, BufferSize, SupportedBufferSize};
use rodio::{DeviceSinkBuilder, DeviceSinkError, MixerDeviceSink, Source};

use crate::scene::{BEAT_SECONDS, INTRO_BEATS};

const SAMPLE_RATE: u32 = 44_100;
const BUFFER_FRAMES: u32 = 2048;

/// Bass root and chord tones as MIDI notes: Am, F, C, G.
const PROGRESSION: [(i32, [i32; 3]); 4] = [
    (33, [57, 60, 64]),
    (29, [53, 57, 60]),
    (36, [52, 55, 60]),
    (31, [55, 59, 62]),
];
const BUILD_UP_CHORD: (i32, [i32; 3]) = (31, [55, 59, 62]);
/// Sixteenth-note arpeggio over the chord tones plus the root and third an octave up.
const ARPEGGIO: [usize; 16] = [0, 1, 2, 3, 2, 1, 2, 4, 0, 2, 3, 4, 3, 2, 1, 2];

/// Starts an original synthesized track on the default output. It plays until the sink drops.
pub fn start() -> Result<MixerDeviceSink, DeviceSinkError> {
    let device = cpal::default_host()
        .default_output_device()
        .ok_or(DeviceSinkError::NoDevice)?;
    let supported = device
        .default_output_config()
        .map_err(DeviceSinkError::DefaultSinkConfigError)?;
    // rodio 0.23 requests a 50 ms buffer without checking the device's supported range,
    // which fails on outputs that cap the buffer below that (960 frames on some Macs).
    let buffer_frames = match supported.buffer_size() {
        SupportedBufferSize::Range { min, max } => BUFFER_FRAMES.clamp(*min, *max),
        SupportedBufferSize::Unknown => BUFFER_FRAMES,
    };
    let mut sink = DeviceSinkBuilder::from_device(device)?
        .with_buffer_size(BufferSize::Fixed(buffer_frames))
        .open_stream()?;
    sink.log_on_drop(false);
    sink.mixer().add(Track::default());
    Ok(sink)
}

/// One-pole filter states and the noise generator for each voice.
struct Track {
    sample: u64,
    noise_state: u32,
    arp: f32,
    bass: f32,
    pad: f32,
    hat: f32,
    snare: f32,
    riser: f32,
}

impl Default for Track {
    fn default() -> Track {
        Track {
            sample: 0,
            noise_state: 0x9e37_79b9,
            arp: 0.0,
            bass: 0.0,
            pad: 0.0,
            hat: 0.0,
            snare: 0.0,
            riser: 0.0,
        }
    }
}

impl Iterator for Track {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        let time = self.sample as f64 / f64::from(SAMPLE_RATE);
        self.sample += 1;
        Some(self.render(time))
    }
}

impl Source for Track {
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> NonZero<u16> {
        NonZero::<u16>::MIN
    }

    fn sample_rate(&self) -> NonZero<u32> {
        const RATE: NonZero<u32> = NonZero::new(SAMPLE_RATE).unwrap();
        RATE
    }

    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

impl Track {
    fn noise(&mut self) -> f32 {
        let mut state = self.noise_state;
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        self.noise_state = state;
        state as f32 / u32::MAX as f32 * 2.0 - 1.0
    }

    fn render(&mut self, time: f64) -> f32 {
        let beat_seconds = f64::from(BEAT_SECONDS);
        let step_seconds = beat_seconds / 4.0;
        let beats = time / beat_seconds;
        let beat_time = beats.fract() * beat_seconds;
        let step = (time / step_seconds).floor() as i64;
        let step_time = time - step as f64 * step_seconds;
        let drop_step = (INTRO_BEATS * 4.0) as i64;
        let bar = (step - drop_step).div_euclid(16);
        let step_in_bar = (step - drop_step).rem_euclid(16) as usize;
        let building = step < drop_step;
        let progress = (beats / f64::from(INTRO_BEATS)).min(1.0) as f32;
        let noise = self.noise();

        let (root, tones) = if building {
            BUILD_UP_CHORD
        } else {
            PROGRESSION[bar.rem_euclid(4) as usize]
        };
        let arp_notes = [tones[0], tones[1], tones[2], tones[0] + 12, tones[1] + 12];
        let arp_frequency = midi(arp_notes[ARPEGGIO[step_in_bar]] + 12);
        let arp_envelope = attack(step_time) * (-step_time * 12.0).exp() as f32;
        let arp_raw = 0.5 * (saw(arp_frequency, step_time) + saw(arp_frequency * 1.006, step_time));
        let arp_cutoff = if building {
            250.0 + 3000.0 * progress * progress
        } else {
            1600.0 + 2600.0 * arp_envelope
        };
        self.arp += coefficient(arp_cutoff) * (arp_raw * arp_envelope - self.arp);

        self.snare += coefficient(1200.0) * (noise - self.snare);
        let snare_noise = noise - self.snare;
        self.hat += coefficient(6500.0) * (noise - self.hat);
        let hat_noise = noise - self.hat;

        if building {
            // The snare roll doubles its speed halfway through and again for the last beat.
            let hits_per_step = if progress < 0.5 {
                0.5
            } else if progress < 0.75 {
                1.0
            } else {
                2.0
            };
            let hit_time = time % (step_seconds / hits_per_step);
            let roll = snare_noise * (-hit_time * 25.0).exp() as f32 * (0.2 + 0.6 * progress);
            self.riser += coefficient(400.0 + 7000.0 * progress) * (noise - self.riser);
            let riser = (noise - self.riser) * 0.3 * progress * progress;
            return master(self.arp * 0.45 + roll * 0.5 + riser);
        }

        let since_drop = time - f64::from(INTRO_BEATS) * beat_seconds;
        let first_beat = since_drop < beat_seconds;
        let (kick_sweep, kick_decay) = if first_beat {
            (200.0, 4.0)
        } else {
            (120.0, 9.0)
        };
        let kick_phase =
            TAU * (45.0 * beat_time + kick_sweep / 35.0 * (1.0 - (-35.0 * beat_time).exp()));
        let kick = (kick_phase.sin() * (-beat_time * kick_decay).exp()) as f32;
        let duck = 1.0 - 0.65 * (-beat_time * 7.0).exp() as f32;

        let fill = bar.rem_euclid(8) == 7 && step_in_bar >= 12;
        let clap_time = if fill {
            Some(step_time)
        } else if (step_in_bar / 4) % 2 == 1 {
            Some(beat_time)
        } else {
            None
        };
        let clap = clap_time.map_or(0.0, |time| snare_noise * (-time * 18.0).exp() as f32);

        let hat_velocity = [0.25, 0.45, 1.0, 0.45][step_in_bar % 4];
        let hat = hat_noise * (-step_time * 70.0).exp() as f32 * hat_velocity;
        let crash = hat_noise * (-since_drop * 2.0).exp() as f32;

        let bass_time = match step_in_bar % 4 {
            2 => Some(step_time),
            3 => Some(step_time + step_seconds),
            _ => None,
        };
        let bass_raw = bass_time.map_or(0.0, |time| {
            saw(midi(root), time) * attack(time) * (-time * 5.0).exp() as f32
        });
        self.bass += coefficient(420.0) * (bass_raw - self.bass);

        let pad_raw: f32 = tones
            .iter()
            .map(|&note| saw(midi(note), time) + saw(midi(note) * 1.004, time))
            .sum::<f32>()
            / 6.0;
        self.pad += coefficient(1000.0) * (pad_raw - self.pad);

        let music = self.arp * 0.32 + self.bass * 0.6 + self.pad * 0.16;
        master(kick * 0.85 + clap * 0.45 + hat * 0.16 + crash * 0.3 + duck * music)
    }
}

fn midi(note: i32) -> f64 {
    440.0 * 2f64.powf(f64::from(note - 69) / 12.0)
}

fn saw(frequency: f64, time: f64) -> f32 {
    (2.0 * (frequency * time).fract() - 1.0) as f32
}

/// A 4 ms fade-in so notes do not click.
fn attack(time: f64) -> f32 {
    (time / 0.004).min(1.0) as f32
}

fn coefficient(cutoff: f32) -> f32 {
    1.0 - (-std::f32::consts::TAU * cutoff / SAMPLE_RATE as f32).exp()
}

fn master(mix: f32) -> f32 {
    (mix * 1.1).tanh() * 0.75
}
