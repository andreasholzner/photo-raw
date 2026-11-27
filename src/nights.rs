//! Nights of aurora photos and their sky colour (US-10).
//!
//! A night runs from noon to noon by capture time. Its sky colour is the owner's choice, a tag on
//! any of its photos; until then the night's photos get the default colour, and a few of them a
//! duplicate in the other colour to compare.

use crate::aurora_edit::SkyColour;

/// The owner's choice for a night, on any of its photos.
pub const NIGHT_COLOUR_TAG: &str = "photoflow|night colour|";
/// The owner's choice for a single photo; it wins over the night's.
pub const SKY_COLOUR_TAG: &str = "photoflow|sky colour|";
/// The colour of the tool's aurora edit of a photo.
pub const SKY_TAG: &str = "photoflow|sky|";
/// On a comparison duplicate: `photoflow|comparison|<colour>`.
pub const COMPARISON_TAG: &str = "photoflow|comparison|";
/// On a photo that has its comparison duplicate.
pub const COMPARED_TAG: &str = "photoflow|compared";
/// The open step of an undecided night.
pub const QUESTION: &str = "sky colour: neutral or violet?";
/// Until the owner chooses.
pub const DEFAULT: SkyColour = SkyColour::NeutralBlue;

/// The night a capture time (seconds, local clock) belongs to.
pub fn night(time: f64) -> i64 {
    ((time - 12.0 * 3600.0) / 86400.0).floor() as i64
}

/// The colour a tag value names.
fn colour(value: &str) -> Option<SkyColour> {
    match value {
        "neutral" => Some(SkyColour::NeutralBlue),
        "violet" => Some(SkyColour::BlueViolet),
        _ => None,
    }
}

/// The tag value of a colour.
pub fn value(c: SkyColour) -> &'static str {
    match c {
        SkyColour::NeutralBlue => "neutral",
        SkyColour::BlueViolet => "violet",
    }
}

/// The colours tags with this prefix ask for, without repeats.
fn asked(tags: &[String], prefix: &str) -> Vec<SkyColour> {
    let mut out = Vec::new();
    for c in tags
        .iter()
        .filter_map(|t| t.strip_prefix(prefix))
        .filter_map(colour)
    {
        if !out.contains(&c) {
            out.push(c);
        }
    }
    out
}

/// What the owner chose for a night.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    Undecided,
    Chosen(SkyColour),
    /// Both colours are tagged: a question again.
    Conflict,
}

/// The night's choice from the tags of its photos.
pub fn choice<'a>(tags: impl IntoIterator<Item = &'a Vec<String>>) -> Choice {
    let mut all = Vec::new();
    for t in tags {
        for c in asked(t, NIGHT_COLOUR_TAG) {
            if !all.contains(&c) {
                all.push(c);
            }
        }
    }
    match all[..] {
        [] => Choice::Undecided,
        [c] => Choice::Chosen(c),
        _ => Choice::Conflict,
    }
}

/// The sky colour of one photo: its own tag, else the night's choice, else the default.
pub fn photo_colour(tags: &[String], night: Choice) -> SkyColour {
    match (&asked(tags, SKY_COLOUR_TAG)[..], night) {
        ([c], _) => *c,
        (_, Choice::Chosen(c)) => c,
        _ => DEFAULT,
    }
}

/// Which of a night's `n` photos (in capture order) get a comparison duplicate: a few, at least
/// 20 % of them, spread over the night (owner, 2026-11-27).
pub fn comparisons(n: usize) -> Vec<usize> {
    if n == 0 {
        return Vec::new();
    }
    let k = n.min(3).max(n.div_ceil(5));
    (0..k).map(|i| (2 * i + 1) * n / (2 * k)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const H: f64 = 3600.0;

    #[test]
    fn a_night_runs_from_noon_to_noon() {
        let day = 86400.0 * 20000.0;
        assert_eq!(night(day + 22.7 * H), night(day + 25.25 * H));
        assert_ne!(night(day + 11.8 * H), night(day + 12.2 * H));
        assert_ne!(night(day + 23.0 * H), night(day + 47.5 * H));
    }

    #[test]
    fn a_few_comparisons_spread_over_the_night() {
        assert_eq!(comparisons(1), vec![0]);
        assert_eq!(comparisons(2), vec![0, 1]);
        assert_eq!(comparisons(5), vec![0, 2, 4]);
        assert_eq!(comparisons(30).len(), 6);
        assert_eq!(comparisons(30), vec![2, 7, 12, 17, 22, 27]);
    }

    fn tags(t: &[&str]) -> Vec<String> {
        t.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn the_photos_tag_wins_over_the_nights() {
        let violet_night = vec![tags(&["photoflow|night colour|violet"]), tags(&[])];
        let c = choice(&violet_night);
        assert_eq!(c, Choice::Chosen(SkyColour::BlueViolet));
        assert_eq!(photo_colour(&tags(&[]), c), SkyColour::BlueViolet);
        assert_eq!(
            photo_colour(&tags(&["photoflow|sky colour|neutral"]), c),
            SkyColour::NeutralBlue
        );
        assert_eq!(photo_colour(&tags(&[]), Choice::Undecided), DEFAULT);
    }

    #[test]
    fn both_colours_for_a_night_are_a_question() {
        let both = vec![
            tags(&["photoflow|night colour|violet"]),
            tags(&["photoflow|night colour|neutral"]),
        ];
        assert_eq!(choice(&both), Choice::Conflict);
        assert_eq!(photo_colour(&tags(&[]), Choice::Conflict), DEFAULT);
    }
}
