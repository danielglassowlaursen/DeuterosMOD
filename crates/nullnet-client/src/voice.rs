//! The narrator: the story read aloud, one page at a time. A page with a
//! recording in `web/voice/story-<page>.mp3` plays it through the browser's
//! own audio element, so the WebAssembly build carries no MP3 decoder. A
//! page with no recording (pages 3-5 were rewritten and wait to be recorded
//! again) is read by the browser's speech voice instead; dropping a new clip
//! into `web/voice` is enough for it to be played instead.
//!
//! Browsers only let a page make sound after a click, so the story is read
//! from a click: turning a page after Listen, the Listen button, or opening
//! the story from the top bar. On the desktop there is no voice.

/// Where the clip for a page of the story is served, counting pages from 0.
fn story_clip(page: usize) -> String {
    format!("/voice/story-{}.mp3", page + 1)
}

#[cfg(target_arch = "wasm32")]
mod browser {
    use std::cell::{Cell, RefCell};

    use wasm_bindgen::closure::Closure;
    use wasm_bindgen::{JsCast, JsValue};
    use web_sys::{HtmlAudioElement, SpeechSynthesisUtterance};

    thread_local! {
        /// The clip playing now, kept so the next page or a close can stop it.
        static PLAYING: RefCell<Option<HtmlAudioElement>> = const { RefCell::new(None) };
        /// Counts what was asked to be read, so a clip that fails to load
        /// late does not start reading a page the player has left.
        static ASKED: Cell<u32> = const { Cell::new(0) };
    }

    /// Plays the clip at `url`, or reads `text` aloud if it cannot be loaded.
    pub fn play_or_say(url: &str, text: String) {
        stop();
        let asked = ASKED.with(|a| a.get());
        let Ok(audio) = HtmlAudioElement::new_with_src(url) else {
            say(&text);
            return;
        };
        let fallback = Closure::<dyn FnMut(JsValue)>::new(move |_| {
            if ASKED.with(|a| a.get()) == asked {
                say(&text);
            }
        });
        let _ = audio.add_event_listener_with_callback("error", fallback.as_ref().unchecked_ref());
        fallback.forget();
        play_quietly(&audio);
        PLAYING.with(|playing| *playing.borrow_mut() = Some(audio));
    }

    /// Reads text aloud with the browser's speech voice, in English whatever
    /// the browser's own language.
    fn say(text: &str) {
        let Some(speech) = web_sys::window().and_then(|w| w.speech_synthesis().ok()) else {
            return;
        };
        speech.cancel();
        if let Ok(utterance) = SpeechSynthesisUtterance::new_with_text(text) {
            utterance.set_lang("en-GB");
            utterance.set_rate(0.95);
            speech.speak(&utterance);
        }
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
        ASKED.with(|a| a.set(a.get().wrapping_add(1)));
        PLAYING.with(|playing| {
            if let Some(audio) = playing.borrow_mut().take() {
                let _ = audio.pause();
            }
        });
        if let Some(speech) = web_sys::window().and_then(|w| w.speech_synthesis().ok()) {
            speech.cancel();
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub use browser::play_quietly;

/// Reads a page of the story aloud, stopping whatever was being read: its
/// recording if there is one, else `text` in the browser's voice.
#[cfg(target_arch = "wasm32")]
pub fn play_story(page: usize, text: &str) {
    browser::play_or_say(&story_clip(page), text.to_string());
}

/// Stops the narrator.
#[cfg(target_arch = "wasm32")]
pub fn stop() {
    browser::stop();
}

#[cfg(not(target_arch = "wasm32"))]
pub fn play_story(page: usize, text: &str) {
    let _ = (story_clip(page), text);
}

#[cfg(not(target_arch = "wasm32"))]
pub fn stop() {}
