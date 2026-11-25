//! Runs the executable scenarios in `features/` (ADR-0006).
//! Scenarios tagged `@wip` describe behaviour that isn't built yet and are not run.
//! A step without matching step code fails the run instead of being skipped.

mod photo;
mod us07;
mod us08;

use cucumber::World;

#[derive(Debug, Default, World)]
pub struct PhotoWorld {
    /// US-7: the exposure of the photo under test.
    pub exposure: Option<photoflow::exif::Exposure>,
    /// US-8: the photo under test, with its library state and sidecar.
    pub photo: photo::Photo,
}

fn main() {
    let wip = |tags: &[String]| tags.iter().any(|t| t == "wip");
    futures::executor::block_on(
        PhotoWorld::cucumber()
            .fail_on_skipped()
            .filter_run_and_exit("features", move |feature, rule, scenario| {
                !(wip(&feature.tags) || rule.is_some_and(|r| wip(&r.tags)) || wip(&scenario.tags))
            }),
    );
}
