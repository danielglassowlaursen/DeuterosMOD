//! Sound: a handful of short cues synthesised at startup, so the client
//! ships no audio files. Nothing plays before the first click, which is
//! when a browser lets a page make sound, and a mute switch in the top bar
//! turns it all off.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::audio::AudioSource;
use bevy::prelude::*;

const RATE: u32 = 22_050;

pub struct SoundPlugin;

impl Plugin for SoundPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Play>()
            .add_systems(Startup, setup)
            .add_systems(Update, play);
    }
}

/// A cue the game plays.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Cue {
    /// A button was pressed.
    Click,
    /// A turn has run.
    TurnRan,
    /// A swarm is coming, a siege is on, or the crew was raided.
    Alarm,
    /// A battle replay starts.
    Battle,
    /// The game is over.
    GameOver,
}

/// Asks for a cue to be played.
#[derive(Message)]
pub struct Play(pub Cue);

#[derive(Resource)]
pub struct Sounds {
    pub muted: bool,
    /// Whether the page has had the click a browser wants before sound.
    pub unlocked: bool,
    cues: HashMap<Cue, Handle<AudioSource>>,
}

fn setup(mut commands: Commands, mut audio: ResMut<Assets<AudioSource>>) {
    let cues = [
        (Cue::Click, click()),
        (Cue::TurnRan, turn_ran()),
        (Cue::Alarm, alarm()),
        (Cue::Battle, battle()),
        (Cue::GameOver, game_over()),
    ]
    .into_iter()
    .map(|(cue, samples)| {
        let bytes: Arc<[u8]> = wav(&samples).into();
        (cue, audio.add(AudioSource { bytes }))
    })
    .collect();
    commands.insert_resource(Sounds {
        muted: crate::notify::muted_preference(),
        unlocked: false,
        cues,
    });
}

fn play(mut requests: MessageReader<Play>, sounds: Res<Sounds>, mut commands: Commands) {
    for Play(cue) in requests.read() {
        if sounds.muted || !sounds.unlocked {
            continue;
        }
        if let Some(handle) = sounds.cues.get(cue) {
            commands.spawn((AudioPlayer::new(handle.clone()), PlaybackSettings::DESPAWN));
        }
    }
}

// ------------------------------------------------------------ synthesis

/// A note with a little second harmonic, fading at `decay` per second.
fn tone(freq: f32, seconds: f32, decay: f32, volume: f32) -> Vec<f32> {
    let n = (seconds * RATE as f32) as usize;
    (0..n)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            let phase = t * freq * std::f32::consts::TAU;
            let wave = phase.sin() + 0.3 * (phase * 2.0).sin() + 0.1 * (phase * 3.0).sin();
            let attack = (t * 400.0).min(1.0);
            wave * (-t * decay).exp() * attack * volume * 0.7
        })
        .collect()
}

/// A burst of noise fading at `decay` per second.
fn noise(seconds: f32, decay: f32, volume: f32) -> Vec<f32> {
    let n = (seconds * RATE as f32) as usize;
    let mut state: u32 = 0x9E37_79B9;
    let mut last = 0.0f32;
    (0..n)
        .map(|i| {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let white = (state >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0;
            // A touch of low-pass, for a thud rather than a hiss.
            last = last * 0.6 + white * 0.4;
            let t = i as f32 / RATE as f32;
            last * (-t * decay).exp() * volume
        })
        .collect()
}

fn mix(a: &[f32], b: &[f32]) -> Vec<f32> {
    (0..a.len().max(b.len()))
        .map(|i| a.get(i).copied().unwrap_or(0.0) + b.get(i).copied().unwrap_or(0.0))
        .collect()
}

fn sequence(parts: &[Vec<f32>]) -> Vec<f32> {
    parts.iter().flatten().copied().collect()
}

fn click() -> Vec<f32> {
    tone(1400.0, 0.05, 70.0, 0.25)
}

fn turn_ran() -> Vec<f32> {
    sequence(&[
        tone(523.0, 0.14, 9.0, 0.5),
        tone(659.0, 0.14, 9.0, 0.5),
        tone(784.0, 0.14, 9.0, 0.5),
        tone(1046.0, 0.4, 5.0, 0.5),
    ])
}

fn alarm() -> Vec<f32> {
    let mut parts = Vec::new();
    for _ in 0..3 {
        parts.push(tone(880.0, 0.16, 6.0, 0.5));
        parts.push(tone(660.0, 0.16, 6.0, 0.5));
    }
    sequence(&parts)
}

fn battle() -> Vec<f32> {
    mix(&noise(0.55, 6.0, 0.6), &tone(70.0, 0.35, 8.0, 0.8))
}

fn game_over() -> Vec<f32> {
    sequence(&[
        tone(392.0, 0.18, 8.0, 0.5),
        tone(523.0, 0.18, 8.0, 0.5),
        tone(659.0, 0.18, 8.0, 0.5),
        tone(784.0, 0.18, 8.0, 0.5),
        tone(1046.0, 0.9, 2.5, 0.5),
    ])
}

/// Wraps samples in a 16-bit mono WAV.
fn wav(samples: &[f32]) -> Vec<u8> {
    let data_len = samples.len() as u32 * 2;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&RATE.to_le_bytes());
    out.extend_from_slice(&(RATE * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for sample in samples {
        let value = (sample.clamp(-1.0, 1.0) * 32_767.0) as i16;
        out.extend_from_slice(&value.to_le_bytes());
    }
    out
}
