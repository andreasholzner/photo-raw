//! Steps of US-8 — Only touch what was handed over.

use crate::PhotoWorld;
use crate::photo::{Entry, Hist, Photo};
use cucumber::{given, then, when};
use photoflow::edit_state::{EditState, edit_state};

fn state(w: &PhotoWorld) -> EditState {
    let sidecar = w
        .photo
        .sidecar
        .as_ref()
        .expect("darktable wrote a sidecar")
        .read();
    edit_state(&w.photo.library, &sidecar)
}

#[given("a photo that darktable reports as unedited")]
fn unedited(w: &mut PhotoWorld) {
    w.photo = Photo::unedited();
}

#[given("a photo I edited by hand")]
fn edited_by_hand(w: &mut PhotoWorld) {
    w.photo = Photo::unedited();
    w.photo.in_darkroom(|h| {
        h.push(Entry::new("exposure", &[0, 0, 0x80, 0x3f]));
        h.push(Entry::new("tonecurve", &[1, 1, 1]));
    });
    w.photo.leave_darkroom();
}

#[given("a photo the tool has edited")]
fn edited_by_tool(w: &mut PhotoWorld) {
    w.photo = Photo::edited_by_tool();
}

#[given("I have another photo open in the darkroom")]
fn other_photo_in_darkroom(w: &mut PhotoWorld) {
    w.photo.library.darkroom_open = true;
}

fn change(h: &mut Hist, what: &str) {
    match what {
        "change the exposure" => h.push(Entry::new("exposure", &[0, 0, 0xe0, 0x3f])),
        "switch a module on" => h.push(Entry::new("vignette", &[0, 0, 0x80, 0x3f])),
        "switch a module off" => {
            let mut e = Entry::new("bilat", &[9, 10, 11, 12]);
            e.enabled = false;
            h.push(e);
        }
        "draw a mask on a module" => {
            let mut e = Entry::new("bilat", &[9, 10, 11, 12]);
            e.blend = vec![1; 8];
            h.push(e);
            let n = h.entries.len() - 1;
            h.masks.push((n, vec![0x3f, 0x00, 0x10, 0x3e]));
        }
        other => panic!("unknown change: {other}"),
    }
}

#[when(expr = "I {} in the darkroom")]
fn change_in_darkroom(w: &mut PhotoWorld, what: String) {
    w.photo.in_darkroom(|h| change(h, &what));
}

#[when("I undo that change")]
fn undo(w: &mut PhotoWorld) {
    w.photo.in_darkroom(|h| h.end -= 1);
}

#[when("I leave the darkroom")]
fn leave(w: &mut PhotoWorld) {
    w.photo.leave_darkroom();
}

#[when("I apply a style to it in the lighttable")]
fn apply_style(w: &mut PhotoWorld) {
    w.photo.in_lighttable(|h| {
        h.push(Entry::new("lens", &[1, 2]));
        h.push(Entry::new("denoiseprofile", &[3, 4]));
    });
}

#[when("I compress its history stack")]
fn compress(w: &mut PhotoWorld) {
    w.photo.compress();
}

#[then("the tool may edit it")]
fn may_edit(w: &mut PhotoWorld) {
    let s = state(w);
    assert!(s.may_edit(), "the tool may not edit it: {s:?}");
}

#[then(expr = "the tool leaves it alone and reports {string}")]
fn leaves_alone(w: &mut PhotoWorld, report: String) {
    let s = state(w);
    assert!(!s.may_edit(), "the tool would edit it: {s:?}");
    assert_eq!(s.report(), Some(report.as_str()));
}
