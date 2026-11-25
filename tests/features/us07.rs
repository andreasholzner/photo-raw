//! Steps of US-7 — Classify day, night and aurora.

use crate::PhotoWorld;
use cucumber::{given, then};
use photoflow::exif::Exposure;
use photoflow::scene::{self, Scene};

/// "1/250" or "4" (seconds).
fn shutter(s: &str) -> f64 {
    match s.split_once('/') {
        Some((a, b)) => a.parse::<f64>().unwrap() / b.parse::<f64>().unwrap(),
        None => s.parse().unwrap(),
    }
}

/// "f/8", or "manual" for a lens that doesn't report its aperture.
fn aperture(s: &str) -> Option<f64> {
    if s == "manual" {
        return None;
    }
    Some(
        s.strip_prefix("f/")
            .expect("aperture like f/8 or manual")
            .parse()
            .unwrap(),
    )
}

#[given(expr = "a photo taken at {word} s, aperture {word} and ISO {int}")]
fn photo_taken(w: &mut PhotoWorld, t: String, f: String, iso: u32) {
    w.exposure = Some(Exposure {
        time: shutter(&t),
        aperture: aperture(&f),
        iso: f64::from(iso),
    });
}

#[then(expr = "its scene brightness is {float}")]
fn brightness_is(w: &mut PhotoWorld, ev: f64) {
    let got = scene::ev100(w.exposure.as_ref().unwrap());
    assert!(
        (got - ev).abs() < 0.05,
        "scene brightness {got:.2}, expected {ev}"
    );
}

#[then(expr = "it is a {word} photo")]
fn class_is(w: &mut PhotoWorld, class: String) {
    let expected = match class.as_str() {
        "day" => Scene::Day,
        "night" => Scene::Night,
        "unclear" => Scene::Unclear,
        other => panic!("unknown class {other}"),
    };
    assert_eq!(scene::classify(w.exposure.as_ref().unwrap()), expected);
}
