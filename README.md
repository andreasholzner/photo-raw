# photoflow

Automation around [darktable](https://www.darktable.org/) for the repetitive parts of raw
editing. darktable stays the editor; photoflow prepares edits and keeps track of open steps.

Requirements, decisions and background: [docs/](docs/) — start with
[docs/requirements.md](docs/requirements.md).

## Build

```sh
cargo build --release   # binary: target/release/photoflow
```

Needs `darktable-cli` (same version as the darktable you edit with).

## Configuration

`~/.config/photoflow/config.toml` (or `--config <file>`):

```toml
photo_root = "~/Pictures/raw"   # folder that contains the trip folders
```

Optional settings and their defaults: run any command without a configuration to see a
commented example.

## Self-test

Checks that darktable still renders a fixed set of your edited photos
([data/reference-set.toml](data/reference-set.toml)) as before — run it after every darktable
upgrade, and after changes to presets or styles.

```sh
photoflow selftest init     # once: render the set and store it as the reference
photoflow selftest run      # render again and compare with the reference
photoflow selftest accept   # make the last run the new reference (intended changes)
```

Renders are stored in `~/.local/share/photoflow/selftest/` (`reference/`, `current/`), as PNGs
you can open side by side. darktable-cli runs with a private copy of your darktable
configuration and an in-memory library; your library, configuration and photos are not
touched.
