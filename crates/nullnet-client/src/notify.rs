//! What the client can do outside its canvas: browser notifications when a
//! turn runs while the tab is in the background, and the mute switch kept
//! in the browser's storage. Nothing here does anything on the desktop.

/// Whether the browser may show notifications.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Permission {
    Granted,
    Denied,
    /// Not asked yet, or not available.
    Ask,
}

const MUTED_KEY: &str = "nullnet.muted";

#[cfg(target_arch = "wasm32")]
pub fn permission() -> Permission {
    match web_sys::Notification::permission() {
        web_sys::NotificationPermission::Granted => Permission::Granted,
        web_sys::NotificationPermission::Denied => Permission::Denied,
        _ => Permission::Ask,
    }
}

#[cfg(target_arch = "wasm32")]
pub fn request_permission() {
    let _ = web_sys::Notification::request_permission();
}

/// Shows a notification if the page is in the background and may.
#[cfg(target_arch = "wasm32")]
pub fn notify(title: &str, body: &str) {
    if permission() != Permission::Granted {
        return;
    }
    let hidden = web_sys::window()
        .and_then(|w| w.document())
        .is_some_and(|d| d.hidden());
    if !hidden {
        return;
    }
    let options = web_sys::NotificationOptions::new();
    options.set_body(body);
    let _ = web_sys::Notification::new_with_options(title, &options);
}

#[cfg(target_arch = "wasm32")]
fn storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok().flatten()
}

#[cfg(target_arch = "wasm32")]
pub fn muted_preference() -> bool {
    storage()
        .and_then(|s| s.get_item(MUTED_KEY).ok().flatten())
        .is_some_and(|v| v == "1")
}

#[cfg(target_arch = "wasm32")]
pub fn remember_muted(muted: bool) {
    if let Some(storage) = storage() {
        let _ = storage.set_item(MUTED_KEY, if muted { "1" } else { "0" });
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn permission() -> Permission {
    Permission::Denied
}

#[cfg(not(target_arch = "wasm32"))]
pub fn request_permission() {}

#[cfg(not(target_arch = "wasm32"))]
pub fn notify(_title: &str, _body: &str) {}

#[cfg(not(target_arch = "wasm32"))]
pub fn muted_preference() -> bool {
    let _ = MUTED_KEY;
    false
}

#[cfg(not(target_arch = "wasm32"))]
pub fn remember_muted(_muted: bool) {}

const GUIDE_KEY: &str = "nullnet.guide-hidden";

/// Whether the player hid the guide in an earlier visit.
#[cfg(target_arch = "wasm32")]
pub fn guide_hidden() -> bool {
    storage()
        .and_then(|s| s.get_item(GUIDE_KEY).ok().flatten())
        .is_some_and(|v| v == "1")
}

#[cfg(target_arch = "wasm32")]
pub fn remember_guide_hidden(hidden: bool) {
    if let Some(storage) = storage() {
        let _ = storage.set_item(GUIDE_KEY, if hidden { "1" } else { "0" });
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn guide_hidden() -> bool {
    let _ = GUIDE_KEY;
    false
}

#[cfg(not(target_arch = "wasm32"))]
pub fn remember_guide_hidden(_hidden: bool) {}

const STORY_KEY: &str = "nullnet.story-seen";

/// Whether the player has been told the story in an earlier visit.
#[cfg(target_arch = "wasm32")]
pub fn story_seen() -> bool {
    storage()
        .and_then(|s| s.get_item(STORY_KEY).ok().flatten())
        .is_some_and(|v| v == "1")
}

#[cfg(target_arch = "wasm32")]
pub fn remember_story_seen() {
    if let Some(storage) = storage() {
        let _ = storage.set_item(STORY_KEY, "1");
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn story_seen() -> bool {
    let _ = STORY_KEY;
    false
}

#[cfg(not(target_arch = "wasm32"))]
pub fn remember_story_seen() {}

const MUSIC_KEY: &str = "nullnet.music-off";

/// Whether the player turned the music off in an earlier visit.
#[cfg(target_arch = "wasm32")]
pub fn music_off_preference() -> bool {
    storage()
        .and_then(|s| s.get_item(MUSIC_KEY).ok().flatten())
        .is_some_and(|v| v == "1")
}

#[cfg(target_arch = "wasm32")]
pub fn remember_music_off(off: bool) {
    if let Some(storage) = storage() {
        let _ = storage.set_item(MUSIC_KEY, if off { "1" } else { "0" });
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn music_off_preference() -> bool {
    let _ = MUSIC_KEY;
    false
}

#[cfg(not(target_arch = "wasm32"))]
pub fn remember_music_off(_off: bool) {}

const REPORTS_KEY: &str = "nullnet.reports-off";

/// Whether the player turned the turn report off in an earlier visit.
#[cfg(target_arch = "wasm32")]
pub fn reports_off_preference() -> bool {
    storage()
        .and_then(|s| s.get_item(REPORTS_KEY).ok().flatten())
        .is_some_and(|v| v == "1")
}

#[cfg(target_arch = "wasm32")]
pub fn remember_reports_off(off: bool) {
    if let Some(storage) = storage() {
        let _ = storage.set_item(REPORTS_KEY, if off { "1" } else { "0" });
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn reports_off_preference() -> bool {
    let _ = REPORTS_KEY;
    false
}

#[cfg(not(target_arch = "wasm32"))]
pub fn remember_reports_off(_off: bool) {}
