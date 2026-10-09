//! The background music: one track in `web/music/loop.mp3`, played on repeat
//! by the browser's own audio element, like the narrator. The track carries
//! its own fade in and fade out, so the loop dips and starts again instead
//! of cutting. It starts with the first click, as browsers want, sinks
//! while the story is read aloud, and stops with the sound or its own
//! switch. On the desktop there is no music.

/// Where the track is served.
#[cfg(target_arch = "wasm32")]
const TRACK: &str = "/music/loop.mp3";
/// The music's volume, and its volume under the narrator.
#[cfg(target_arch = "wasm32")]
const VOLUME: f64 = 0.35;
#[cfg(target_arch = "wasm32")]
const UNDER_VOICE: f64 = 0.1;

#[cfg(target_arch = "wasm32")]
mod browser {
    use std::cell::RefCell;

    use web_sys::HtmlAudioElement;

    #[derive(Default)]
    pub struct Player {
        pub audio: Option<HtmlAudioElement>,
        pub ducked: bool,
    }

    thread_local! {
        pub static PLAYER: RefCell<Player> = RefCell::new(Player::default());
    }
}

#[cfg(target_arch = "wasm32")]
fn volume(ducked: bool) -> f64 {
    if ducked { UNDER_VOICE } else { VOLUME }
}

/// Starts the music, or resumes it where it was paused.
#[cfg(target_arch = "wasm32")]
pub fn start() {
    browser::PLAYER.with(|player| {
        let mut player = player.borrow_mut();
        if player.audio.is_none() {
            let Ok(audio) = web_sys::HtmlAudioElement::new_with_src(TRACK) else {
                return;
            };
            audio.set_loop(true);
            player.audio = Some(audio);
        }
        let ducked = player.ducked;
        if let Some(audio) = &player.audio {
            audio.set_volume(volume(ducked));
            crate::voice::play_quietly(audio);
        }
    });
}

/// Pauses the music.
#[cfg(target_arch = "wasm32")]
pub fn stop() {
    browser::PLAYER.with(|player| {
        if let Some(audio) = &player.borrow().audio {
            let _ = audio.pause();
        }
    });
}

/// Lowers the music under the narrator, or raises it again.
#[cfg(target_arch = "wasm32")]
pub fn duck(ducked: bool) {
    browser::PLAYER.with(|player| {
        let mut player = player.borrow_mut();
        player.ducked = ducked;
        if let Some(audio) = &player.audio {
            audio.set_volume(volume(ducked));
        }
    });
}

#[cfg(not(target_arch = "wasm32"))]
pub fn start() {}

#[cfg(not(target_arch = "wasm32"))]
pub fn stop() {}

#[cfg(not(target_arch = "wasm32"))]
pub fn duck(_ducked: bool) {}
