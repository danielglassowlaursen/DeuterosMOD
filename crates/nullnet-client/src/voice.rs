//! The narrator: the recorded voice-over of the story, one clip per page in
//! `web/voice/story-<page>.mp3`. The browser's own audio element plays it,
//! so the WebAssembly build carries no MP3 decoder. Browsers only let a page
//! make sound after a click, so the story is read aloud from the first page
//! turn, or from the page's Listen button. On the desktop there is no voice.

/// Pages with a recorded narration. The first two (ResetN00L and NullNet)
/// are unchanged; pages 3-5 were rewritten for the new rules and wait to be
/// recorded again, so they are read in silence for now.
const VOICED: usize = 2;

/// Where the clip for a page of the story is served, counting pages from 0,
/// or nothing when the page has no up-to-date recording.
fn story_clip(page: usize) -> Option<String> {
    (page < VOICED).then(|| format!("/voice/story-{}.mp3", page + 1))
}

#[cfg(target_arch = "wasm32")]
mod browser {
    use std::cell::RefCell;

    use wasm_bindgen::JsValue;
    use wasm_bindgen::closure::Closure;
    use web_sys::HtmlAudioElement;

    thread_local! {
        /// The clip playing now, kept so the next page or a close can stop it.
        static PLAYING: RefCell<Option<HtmlAudioElement>> = const { RefCell::new(None) };
    }

    pub fn play(url: &str) {
        stop();
        let Ok(audio) = HtmlAudioElement::new_with_src(url) else {
            return;
        };
        play_quietly(&audio);
        PLAYING.with(|playing| *playing.borrow_mut() = Some(audio));
    }

    /// Plays an audio element. A blocked or interrupted one rejects its
    /// promise; that is expected, not an error worth a line in the console.
    pub fn play_quietly(audio: &HtmlAudioElement) {
        if let Ok(promise) = audio.play() {
            let ignore = Closure::<dyn FnMut(JsValue)>::new(|_| {});
            let _ = promise.catch(&ignore);
            ignore.forget();
        }
    }

    pub fn stop() {
        PLAYING.with(|playing| {
            if let Some(audio) = playing.borrow_mut().take() {
                let _ = audio.pause();
            }
        });
    }
}

#[cfg(target_arch = "wasm32")]
pub use browser::play_quietly;

/// Reads a page of the story aloud, stopping whatever was being read. Pages
/// with no current recording are read in silence.
#[cfg(target_arch = "wasm32")]
pub fn play_story(page: usize) {
    match story_clip(page) {
        Some(url) => browser::play(&url),
        None => browser::stop(),
    }
}

/// Stops the narrator.
#[cfg(target_arch = "wasm32")]
pub fn stop() {
    browser::stop();
}

#[cfg(not(target_arch = "wasm32"))]
pub fn play_story(page: usize) {
    let _ = story_clip(page);
}

#[cfg(not(target_arch = "wasm32"))]
pub fn stop() {}
