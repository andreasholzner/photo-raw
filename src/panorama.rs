//! Panorama sets (US-12): the tags, and the proposals from photos taken in quick succession.
//!
//! A set is a tag on its photos named after the set's first photo; a proposal is a tag too, and
//! an open step, until the owner confirms or rejects it (ADR-0002). The actions on sets live in
//! photoflow's darktable script.

/// A confirmed set: `photoflow|panorama|<first photo>`.
pub const SET_TAG: &str = "photoflow|panorama|";
/// A proposed set: `photoflow|panorama proposal|<first photo>`.
pub const PROPOSAL_TAG: &str = "photoflow|panorama proposal|";
/// The owner rejected the proposal these photos were in: never proposed again.
pub const REJECTED_TAG: &str = "photoflow|not a panorama";
/// The open step that lists proposed sets.
pub const OPEN_STEP: &str = "panorama sets to confirm";
/// Longest gap between two photos of one set, in seconds (the owner's choice).
pub const MAX_GAP: f64 = 10.0;

/// One photo as the proposal rule sees it.
#[derive(Debug, Clone, PartialEq)]
pub struct Shot {
    /// File name without extension, e.g. "_DSC6319": the set's name if it comes first.
    pub name: String,
    pub time: f64,
    pub focal_mm: Option<f64>,
    /// Already in a set, or in a rejected proposal.
    pub excluded: bool,
}

/// Whether the tags keep a photo out of proposals.
pub fn excluded(tags: &[String]) -> bool {
    tags.iter()
        .any(|t| t.starts_with(SET_TAG) || t == REJECTED_TAG)
}

/// Proposed sets as indices into `shots`, each in capture order: consecutive photos at most
/// [`MAX_GAP`] apart with the same focal length in whole millimetres; at least two photos. An
/// excluded photo ends a run.
pub fn proposals(shots: &[Shot]) -> Vec<Vec<usize>> {
    let mut order: Vec<usize> = (0..shots.len()).collect();
    order.sort_by(|&a, &b| {
        shots[a]
            .time
            .total_cmp(&shots[b].time)
            .then_with(|| shots[a].name.cmp(&shots[b].name))
    });
    let mm = |s: &Shot| s.focal_mm.map(|f| f.round() as i64);
    let mut out = Vec::new();
    let mut run: Vec<usize> = Vec::new();
    for i in order {
        let s = &shots[i];
        let continues = run.last().is_some_and(|&j| {
            let p = &shots[j];
            s.time - p.time <= MAX_GAP && mm(s) == mm(p)
        });
        if !continues {
            if run.len() >= 2 {
                out.push(std::mem::take(&mut run));
            }
            run.clear();
        }
        if s.excluded {
            if run.len() >= 2 {
                out.push(std::mem::take(&mut run));
            }
            run.clear();
            continue;
        }
        run.push(i);
    }
    if run.len() >= 2 {
        out.push(run);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Shots taken `gaps` seconds apart with the given focal lengths.
    fn shots(gaps: &[f64], focal: &[f64]) -> Vec<Shot> {
        let mut t = 1000.0;
        focal
            .iter()
            .enumerate()
            .map(|(i, &f)| {
                if i > 0 {
                    t += gaps[i - 1];
                }
                Shot {
                    name: format!("_DSC{:04}", i + 1),
                    time: t,
                    focal_mm: Some(f),
                    excluded: false,
                }
            })
            .collect()
    }

    fn sizes(gaps: &[f64], focal: &[f64]) -> Vec<usize> {
        proposals(&shots(gaps, focal))
            .iter()
            .map(Vec::len)
            .collect()
    }

    #[test]
    fn the_feature_examples() {
        assert_eq!(sizes(&[3.0, 4.0, 5.0], &[16.0; 4]), vec![4]);
        assert_eq!(sizes(&[10.0], &[16.0; 2]), vec![2]);
        assert_eq!(sizes(&[11.0], &[16.0; 2]), Vec::<usize>::new());
        assert_eq!(sizes(&[3.0, 30.0, 3.0], &[16.0; 4]), vec![2, 2]);
        assert_eq!(sizes(&[3.0, 3.0], &[16.0, 16.0, 24.0]), vec![2]);
    }

    #[test]
    fn focal_lengths_compare_in_whole_millimetres() {
        assert_eq!(sizes(&[2.0], &[16.4, 15.6]), vec![2]);
        assert_eq!(sizes(&[2.0], &[16.4, 16.6]), Vec::<usize>::new());
    }

    #[test]
    fn excluded_photos_end_a_run_and_aren_t_proposed() {
        let mut s = shots(&[2.0, 2.0, 2.0, 2.0], &[16.0; 5]);
        s[2].excluded = true;
        assert_eq!(proposals(&s), vec![vec![0, 1], vec![3, 4]]);
        assert!(excluded(&["photoflow|panorama|_DSC0001".into()]));
        assert!(excluded(&[REJECTED_TAG.into()]));
        assert!(!excluded(&["photoflow|panorama proposal|_DSC0001".into()]));
    }

    #[test]
    fn order_is_by_capture_time() {
        let mut s = shots(&[2.0, 2.0], &[16.0; 3]);
        s.swap(0, 2);
        assert_eq!(proposals(&s), vec![vec![2, 1, 0]]);
    }
}
