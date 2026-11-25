//! Runs the executable scenarios in `features/` against the simulated darktable (ADR-0006).
//! The self-test runs the `@darktable` ones against the real darktable (US-20).

fn main() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("features");
    let ok = photoflow::scenarios::run(&dir, photoflow::scenarios::Selection::Simulated, None);
    std::process::exit(if ok { 0 } else { 1 });
}
