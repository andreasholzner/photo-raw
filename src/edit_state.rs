//! Whose edit a photo carries, and whether the tool may edit it (US-8, ADR-0003).
//!
//! darktable's library is the source of truth (ADR-0001); the Lua layer reports what it knows
//! about a photo there. Changes made in the darkroom reach the library and the sidecar only
//! when the darkroom is left, so while it is open the tool touches nothing. Otherwise the Lua
//! layer makes darktable write the sidecar before the tool reads it (journal 2026-11-24).

use crate::sidecar::History;

/// Tag prefix that marks the tool's own edit; the rest of the tag is the history fingerprint.
pub const EDIT_TAG_PREFIX: &str = "photoflow|edit|";

/// What darktable knows about a photo, as reported by the Lua layer.
#[derive(Debug, Clone, Default)]
pub struct LibraryState {
    /// darktable's own flag: the history differs from what darktable applies on import.
    pub is_altered: bool,
    pub tags: Vec<String>,
    /// darktable currently shows the darkroom (with any photo).
    pub darkroom_open: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditState {
    /// Only what darktable applies by itself: the tool may edit.
    Unedited,
    /// Exactly the edit the tool made: the tool may redo it.
    OwnEdit,
    /// Changed by hand: never touched, reported.
    ManualEdit,
    /// darktable shows the darkroom: nothing is touched now, reported.
    DarkroomOpen,
}

impl EditState {
    pub fn may_edit(self) -> bool {
        matches!(self, EditState::Unedited | EditState::OwnEdit)
    }

    /// What the owner is told about a photo the tool leaves alone.
    pub fn report(self) -> Option<&'static str> {
        match self {
            EditState::Unedited | EditState::OwnEdit => None,
            EditState::ManualEdit => Some("edited by hand"),
            EditState::DarkroomOpen => Some("darkroom is open"),
        }
    }
}

/// The tag that records `history` as the tool's own edit.
pub fn edit_tag(history: &History) -> String {
    format!("{EDIT_TAG_PREFIX}{}", history.fingerprint())
}

/// `sidecar` is the photo's sidecar as darktable has just written it.
pub fn edit_state(library: &LibraryState, sidecar: &History) -> EditState {
    if library.darkroom_open {
        return EditState::DarkroomOpen;
    }
    if !library.is_altered {
        return EditState::Unedited;
    }
    let own = edit_tag(sidecar);
    if library.tags.iter().any(|t| *t == own) {
        EditState::OwnEdit
    } else {
        EditState::ManualEdit
    }
}
