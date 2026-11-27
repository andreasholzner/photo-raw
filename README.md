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
commented example (e.g. the paths of `darktable-cli` and `darktable`).

## Self-test

Run it after every darktable upgrade, and after changes to presets or styles. It checks two
things:

- **Rendering:** darktable still renders a fixed set of your edited photos
  ([data/reference-set.toml](data/reference-set.toml)) as before — and the tool's own day edit of
  each day photo of the set (`tool-*.png`) as before.
- **darktable's behaviour:** the scenarios tagged `@darktable-cli` and `@darktable` in
  [features/](features/) — the things photoflow relies on, such as recognising aurora on a
  render, recognising its own edits and leaving yours alone.

```sh
photoflow selftest init       # once: render the set and store it as the reference
photoflow selftest run        # render and compare, then run the darktable scenarios
photoflow selftest scenarios  # only the darktable scenarios (about 10 minutes)
photoflow selftest scenarios --only snow  # only scenarios whose name, rule, feature or file contains "snow"
photoflow selftest accept     # make the last renders the new reference (intended changes)
```

Renders are stored in `~/.local/share/photoflow/selftest/` (`reference/`, `current/`), as PNGs
you can open side by side. darktable-cli runs with a private copy of your darktable
configuration and an in-memory library.

The `@darktable-cli` scenarios render a few photos of the reference set small, with
darktable-cli, and look for their horizons (a few minutes; the first run downloads the sky
model, see below). For the other scenarios darktable runs for about 10 minutes on a
virtual screen (needs `xvfb-run`, Manjaro package `xorg-server-xvfb`): nothing shows on your
desktop, and it ends by itself. Your own darktable can stay open meanwhile. It runs with a
private copy of your configuration, its own library and copies of one raw of the reference set.
Scenarios that can't run unattended are listed at the end as manual checks. Your library,
configuration and photos are not touched by either part.

## Horizon

```sh
photoflow horizon --preview /tmp/horizon photo1.ARW photo2.ARW
```

prints where the land meets the sky in each photo, as fractions of the frame height from the
top, and with `--preview` writes a PNG per photo (sky tinted blue, skyline yellow). It uses a sky
segmentation model ([SkyWater-Seg](https://huggingface.co/Realcat/skywater_seg), MIT licence),
downloaded once with `curl` (95 MB) into `~/.local/share/photoflow/models/`; it runs on the CPU,
about 10 s per photo.

## darktable script

`darktable/lua/photoflow.lua` hands photos over to `photoflow process`: newly imported photos
at once (can be switched off in darktable's preferences, "lua options"), others with the
shortcut "photoflow: process photos". For now it sorts photos into day, night and aurora and
records open steps and reports as tags (`photoflow|class|…`, `photoflow|open|…`,
`photoflow|left alone|…`); the edits themselves follow. Installing it is US-17.

## darktable setup

One-time steps in darktable (styles, export presets): [docs/darktable-setup.md](docs/darktable-setup.md).
