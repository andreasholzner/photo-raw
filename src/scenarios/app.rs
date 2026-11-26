//! The real darktable application as scenario driver (US-20).
//!
//! darktable runs with a private copy of the owner's configuration (settings, presets, styles)
//! and its own throwaway library; the photos are copies of one raw of the reference set. The
//! owner's library, configuration and photos are never touched. A Lua driver inside darktable
//! executes the commands photoflow writes to a folder. Without a display (in the development
//! container) darktable runs under a virtual one (`xvfb-run`).

use super::{Change, Driver};
use crate::config::Config;
use crate::edit_state::{EditState, LibraryState, edit_state, edit_tag};
use crate::locate::locate;
use crate::{reference_set, sidecar};
use anyhow::{Context, Result, bail};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

/// Process group of the running test darktable, for Ctrl+C (0 = none).
static GROUP: AtomicU32 = AtomicU32::new(0);
/// Records the process group of the test darktable, so a later run can end a leftover one.
const PID_FILE: &str = "darktable.pid";

fn kill_group(pgid: u32, signal: &str) {
    let _ = Command::new("kill")
        .args([signal, "--", &format!("-{pgid}")])
        .stderr(Stdio::null())
        .status();
}

/// Ends a test darktable left over from a run that ended abruptly, recognised by its private
/// configuration in `work` (never the owner's darktable).
fn end_leftover(work: &Path) {
    let Ok(text) = std::fs::read_to_string(work.join(PID_FILE)) else {
        return;
    };
    let Ok(pgid) = text.trim().parse::<u32>() else {
        return;
    };
    let ours = std::fs::read(format!("/proc/{pgid}/cmdline"))
        .map(|c| String::from_utf8_lossy(&c).contains(&*work.to_string_lossy()))
        .unwrap_or(false);
    if ours {
        kill_group(pgid, "-KILL");
        std::thread::sleep(Duration::from_secs(1));
    }
}

const DRIVER_LUA: &str = include_str!("driver.lua");
/// photoflow's own darktable script, loaded next to the driver.
const PHOTOFLOW_LUA: &str = include_str!("../../darktable/lua/photoflow.lua");
/// The style applied "by hand" in the lighttable.
const STYLE_NAME: &str = "photoflow Druck v1";
const STYLE_FILE: &str = include_str!("../../darktable/styles/photoflow Druck v1.dtstyle");

pub struct App {
    child: Child,
    work: PathBuf,
    ipc: PathBuf,
    next: u64,
    raw: PathBuf,
    /// The sidecar that stands for the tool's edit.
    edit: String,
    /// A photo used as "another photo" (the one the stand-in edit was made on).
    other: PathBuf,
    photos: u32,
    current: Option<PathBuf>,
    in_darkroom: Option<PathBuf>,
    photo_root: PathBuf,
    /// darktable-cli for rendering the photo under test (US-9), prepared on first use.
    cli: (String, PathBuf),
    renderer: Option<crate::darktable::Darktable>,
}

fn sidecar_of(raw: &Path) -> PathBuf {
    PathBuf::from(format!("{}.xmp", raw.display()))
}

impl App {
    /// Starts darktable in `work` (emptied first) and prepares the tool's stand-in edit.
    pub fn start(cfg: &Config, work: &Path) -> Result<Self> {
        end_leftover(work);
        if work.exists() {
            std::fs::remove_dir_all(work)?;
        }
        let config = work.join("darktable-config");
        let ipc = work.join("ipc");
        for d in [
            &config.join("lua"),
            &ipc,
            &work.join("photos"),
            &work.join("edits"),
        ] {
            std::fs::create_dir_all(d)?;
        }

        // private configuration: the owner's settings, presets and styles, plus the Lua driver
        let owner = cfg.darktable_config_dir()?;
        let rc = std::fs::read_to_string(owner.join("darktablerc"))
            .with_context(|| format!("cannot read {}", owner.join("darktablerc").display()))?;
        if rc.lines().any(|l| l.trim() == "write_sidecar_files=never") {
            bail!(
                "darktable is set to never write sidecar files, which photoflow relies on (preferences → storage → write sidecar file for each image)"
            );
        }
        std::fs::write(config.join("darktablerc"), without_welcome_screen(&rc))?;
        if owner.join("data.db").is_file() {
            std::fs::copy(owner.join("data.db"), config.join("data.db"))?;
        }
        if owner.join("color").is_dir() {
            copy_dir(&owner.join("color"), &config.join("color"))?;
        }
        std::fs::write(config.join("lua/photoflow_selftest.lua"), DRIVER_LUA)?;
        std::fs::write(config.join("lua/photoflow.lua"), PHOTOFLOW_LUA)?;
        std::fs::write(
            config.join("luarc"),
            "require \"photoflow\"\nrequire \"photoflow_selftest\"\n",
        )?;
        let style = work.join(format!("{STYLE_NAME}.dtstyle"));
        std::fs::write(&style, STYLE_FILE)?;

        let p = reference_set::load()?
            .into_iter()
            .find(|p| p.file.to_ascii_uppercase().ends_with(".ARW"))
            .context("the reference set has no raw photo")?;
        let raw = locate(&cfg.photo_root, p.year, &p.file)?;

        // always on a virtual screen: nothing shows on the owner's desktop, and no mouse pointer
        // can make darktable act on another photo (journal 2026-11-26)
        let mut cmd = Command::new("xvfb-run");
        cmd.args(["-a", "-s", "-screen 0 1600x1000x24", &cfg.darktable.app])
            .env_remove("WAYLAND_DISPLAY")
            .env("GDK_BACKEND", "x11");
        let log = std::fs::File::create(work.join("darktable.log"))?;
        cmd.arg("--configdir")
            .arg(&config)
            .arg("--library")
            .arg(config.join("library.db"))
            .env("PHOTOFLOW_SELFTEST_IPC", &ipc)
            .stdin(Stdio::null())
            .stdout(log.try_clone()?)
            .stderr(log)
            .process_group(0);
        let child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => bail!(
                "xvfb-run not found: the self-test runs darktable on a virtual screen (Manjaro: sudo pacman -S xorg-server-xvfb)"
            ),
            Err(e) => {
                return Err(e)
                    .with_context(|| format!("cannot start xvfb-run {}", cfg.darktable.app));
            }
        };

        let pgid = child.id();
        std::fs::write(work.join(PID_FILE), pgid.to_string())?;
        GROUP.store(pgid, Ordering::SeqCst);
        // Ctrl+C reaches only photoflow (darktable runs in its own process group): end it too
        let _ = ctrlc::set_handler(|| {
            let pgid = GROUP.load(Ordering::SeqCst);
            if pgid != 0 {
                kill_group(pgid, "-KILL");
            }
            std::process::exit(130);
        });
        let mut app = App {
            child,
            work: work.to_path_buf(),
            ipc,
            next: 1,
            raw,
            edit: String::new(),
            other: PathBuf::new(),
            photos: 0,
            current: None,
            in_darkroom: None,
            photo_root: cfg.photo_root.clone(),
            cli: (cfg.darktable.cli.clone(), cfg.darktable_config_dir()?),
            renderer: None,
        };
        app.wait_ready()?;
        app.call(&["import_style", &style.to_string_lossy()])?;
        // photoflow's script runs this photoflow with this configuration; it processes imported
        // photos only in the scenarios about that
        let exe = std::env::current_exe()?;
        let cfg_file = cfg
            .source
            .as_deref()
            .map(Path::to_string_lossy)
            .unwrap_or_default();
        app.call(&["set_pref", "binary", "string", &exe.to_string_lossy()])?;
        app.call(&["set_pref", "config", "string", &cfg_file])?;
        app.call(&["set_pref", "after_import", "bool", "0"])?;

        // the tool's stand-in edit (until the tool writes its own, US-9): an exposure change made
        // in the darkroom, as darktable writes it
        let other = app.new_photo()?;
        app.darkroom(&other)?;
        app.action("iop/exposure/exposure", "value", "set", 1.3)?;
        let before = std::fs::read_to_string(sidecar_of(&other)).ok();
        app.lighttable()?;
        app.wait_for_sidecar(&other, &before, 60);
        app.call(&["touch", &other.to_string_lossy()])?;
        app.edit = std::fs::read_to_string(sidecar_of(&other))?;
        app.other = other;
        Ok(app)
    }

    fn wait_ready(&mut self) -> Result<()> {
        let ready = self.ipc.join("ready");
        let start = Instant::now();
        while !ready.exists() {
            if let Some(status) = self.child.try_wait()? {
                bail!(
                    "darktable ended ({status}); see {}",
                    self.work.join("darktable.log").display()
                );
            }
            if start.elapsed() > Duration::from_secs(180) {
                bail!(
                    "darktable didn't start within 3 minutes; see {}",
                    self.work.join("darktable.log").display()
                );
            }
            std::thread::sleep(Duration::from_millis(300));
        }
        Ok(())
    }

    /// Sends a command to the Lua driver and waits for its reply.
    fn call(&mut self, args: &[&str]) -> Result<String> {
        let n = self.next;
        self.next += 1;
        let cmd = self.ipc.join(format!("cmd-{n}"));
        let tmp = self.ipc.join(format!("cmd-{n}.tmp"));
        std::fs::write(&tmp, args.join("\t"))?;
        std::fs::rename(&tmp, &cmd)?;
        let reply = self.ipc.join(format!("reply-{n}"));
        let start = Instant::now();
        loop {
            if let Ok(text) = std::fs::read_to_string(&reply) {
                return match text.split_once('\t') {
                    Some(("ok", r)) => Ok(r.to_string()),
                    Some((_, msg)) => bail!("darktable: {} failed: {msg}", args[0]),
                    None => bail!("darktable: bad reply {text:?}"),
                };
            }
            if let Some(status) = self.child.try_wait()? {
                bail!(
                    "darktable ended ({status}) during {}; see {}",
                    args[0],
                    self.work.join("darktable.log").display()
                );
            }
            if start.elapsed() > Duration::from_secs(1500) {
                bail!("darktable didn't answer {} within 25 minutes", args[0]);
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    /// A new folder with `n` copies of the test raw (not imported yet).
    fn new_folder(&mut self, n: usize) -> Result<(PathBuf, Vec<PathBuf>)> {
        self.photos += 1;
        let dir = self.work.join("photos").join(self.photos.to_string());
        std::fs::create_dir_all(&dir)?;
        let name = self
            .raw
            .file_name()
            .context("raw without name")?
            .to_string_lossy();
        let mut photos = Vec::new();
        for i in 1..=n {
            // unique across folders: panorama sets are named after their first photo (US-12)
            let photo = dir.join(format!("{}-{i}{name}", self.photos));
            std::fs::copy(&self.raw, &photo)?;
            photos.push(photo);
        }
        Ok((dir, photos))
    }

    fn import_folder(&mut self, n: usize, process: bool) -> Result<Vec<PathBuf>> {
        self.lighttable()?;
        let (dir, photos) = self.new_folder(n)?;
        let before = self.call(&["photoflow_runs"])?;
        self.call(&[
            "set_pref",
            "after_import",
            "bool",
            if process { "1" } else { "0" },
        ])?;
        let imported = self.call(&["import_n", &dir.to_string_lossy(), &n.to_string()]);
        self.call(&["set_pref", "after_import", "bool", "0"])?;
        imported?;
        if process {
            self.wait_photoflow(&before)?;
        }
        Ok(photos)
    }

    /// Waits until photoflow's script has finished a run started after `before` runs.
    fn wait_photoflow(&mut self, before: &str) -> Result<()> {
        let error = self.call(&["wait_photoflow", before])?;
        if !error.is_empty() {
            bail!("photoflow's darktable script: {error}");
        }
        Ok(())
    }

    /// Imports a fresh copy of `raw`, in its own folder.
    fn import_copy(&mut self, raw: &Path) -> Result<PathBuf> {
        self.lighttable()?;
        self.photos += 1;
        let dir = self.work.join("photos").join(self.photos.to_string());
        std::fs::create_dir_all(&dir)?;
        let photo = dir.join(raw.file_name().context("raw without name")?);
        std::fs::copy(raw, &photo)?;
        self.call(&["import", &dir.to_string_lossy()])?;
        Ok(photo)
    }

    /// Imports a fresh copy of the test raw, in its own folder.
    fn new_photo(&mut self) -> Result<PathBuf> {
        self.lighttable()?;
        self.photos += 1;
        let dir = self.work.join("photos").join(self.photos.to_string());
        std::fs::create_dir_all(&dir)?;
        let photo = dir.join(self.raw.file_name().context("raw without name")?);
        std::fs::copy(&self.raw, &photo)?;
        self.call(&["import", &dir.to_string_lossy()])?;
        Ok(photo)
    }

    fn lighttable(&mut self) -> Result<()> {
        self.call(&["lighttable"])?;
        self.in_darkroom = None;
        Ok(())
    }

    fn darkroom(&mut self, photo: &Path) -> Result<()> {
        if self.in_darkroom.as_deref() != Some(photo) {
            self.call(&["darkroom", &photo.to_string_lossy()])?;
            self.in_darkroom = Some(photo.to_path_buf());
        }
        Ok(())
    }

    fn current(&self) -> Result<PathBuf> {
        self.current.clone().context("no photo under test")
    }

    fn action(&mut self, path: &str, element: &str, effect: &str, size: f64) -> Result<()> {
        self.call(&["action", path, element, effect, &size.to_string()])?;
        std::thread::sleep(Duration::from_secs(1));
        Ok(())
    }

    /// Waits until darktable has rewritten the sidecar (it may not, if nothing changed), then
    /// until the file has stopped changing.
    fn wait_for_sidecar(&self, photo: &Path, before: &Option<String>, secs: u64) {
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(secs) {
            if std::fs::read_to_string(sidecar_of(photo)).ok() != *before {
                break;
            }
            std::thread::sleep(Duration::from_millis(300));
        }
        settled_sidecar(photo);
    }

    pub fn stop(&mut self) {
        // leave the darkroom first: a sidecar darktable wrote while being ended in the darkroom
        // didn't match its library (journal 2026-11-25)
        if self.child.try_wait().ok().flatten().is_none() && self.lighttable().is_ok() {
            std::thread::sleep(Duration::from_secs(5));
        }
        // darktable has no reliable "quit" for scripts: end the whole process group
        let pgid = self.child.id();
        kill_group(pgid, "-TERM");
        std::thread::sleep(Duration::from_secs(2));
        kill_group(pgid, "-KILL");
        let _ = self.child.wait();
        GROUP.store(0, Ordering::SeqCst);
        let _ = std::fs::remove_file(self.work.join(PID_FILE));
    }
}

impl Drop for App {
    fn drop(&mut self) {
        self.stop();
    }
}

impl Driver for App {
    fn unedited(&mut self) -> Result<()> {
        self.current = Some(self.new_photo()?);
        Ok(())
    }

    fn tool_edit(&mut self) -> Result<()> {
        let photo = self.current()?;
        let dir = self.work.join("edits").join(self.photos.to_string());
        std::fs::create_dir_all(&dir)?;
        let xmp = dir.join(format!(
            "{}.xmp",
            photo.file_name().unwrap().to_string_lossy()
        ));
        std::fs::write(&xmp, &self.edit)?;
        let p = photo.to_string_lossy().to_string();
        let before = std::fs::read_to_string(sidecar_of(&photo)).ok();
        self.call(&["apply_sidecar", &p, &xmp.to_string_lossy()])?;
        self.call(&["touch", &p])?;
        // darktable writes the sidecar in the background: a read right after the tag change
        // could still see the unedited one, and the tag would name the wrong history
        self.wait_for_sidecar(&photo, &before, 20);
        let history = sidecar::read(&sidecar_of(&photo))?;
        self.call(&["tag", &p, &edit_tag(&history)])?;
        Ok(())
    }

    fn change_in_darkroom(&mut self, change: Change) -> Result<()> {
        let photo = self.current()?;
        self.darkroom(&photo)?;
        match change {
            Change::Exposure => self.action("iop/exposure/exposure", "value", "set", 2.0),
            Change::ModuleOn => self.action("iop/vignette", "enable", "on", 1.0),
            Change::ModuleOff => self.action("iop/exposure", "enable", "off", 1.0),
            Change::Mask => bail!("drawing a mask can't be scripted (a @manual check)"),
        }
    }

    fn undo(&mut self) -> Result<()> {
        self.action("views/darkroom/undo", "", "", 1.0)
    }

    fn leave_darkroom(&mut self) -> Result<()> {
        let photo = self.current()?;
        let before = std::fs::read_to_string(sidecar_of(&photo)).ok();
        self.lighttable()?;
        self.wait_for_sidecar(&photo, &before, 60);
        Ok(())
    }

    fn apply_style_in_lighttable(&mut self) -> Result<()> {
        let photo = self.current()?;
        let before = std::fs::read_to_string(sidecar_of(&photo)).ok();
        self.lighttable()?;
        self.call(&["apply_style", STYLE_NAME, &photo.to_string_lossy()])?;
        self.wait_for_sidecar(&photo, &before, 10);
        Ok(())
    }

    fn compress_history(&mut self) -> Result<()> {
        let photo = self.current()?;
        self.darkroom(&photo)?;
        self.action("lib/history/compress history stack", "", "", 1.0)?;
        self.leave_darkroom()
    }

    fn open_other_photo_in_darkroom(&mut self) -> Result<()> {
        let other = self.other.clone();
        self.darkroom(&other)
    }

    fn edit_state(&mut self) -> Result<EditState> {
        let photo = self.current()?;
        let p = photo.to_string_lossy().to_string();
        let state = self.call(&["state", &p])?;
        let mut f = state.splitn(3, '\t');
        let altered = f.next() == Some("1");
        let view = f.next().unwrap_or("").to_string();
        let tags = f
            .next()
            .unwrap_or("")
            .split('\u{1f}')
            .filter(|t| !t.is_empty())
            .map(String::from)
            .collect();
        let library = LibraryState {
            is_altered: altered,
            tags,
            darkroom_open: view == "darkroom",
        };
        if !library.darkroom_open {
            // before the tool reads an edit, darktable is made to write the sidecar
            self.call(&["touch", &p])?;
            settled_sidecar(&photo);
        }
        let history = sidecar::read(&sidecar_of(&photo))?;
        Ok(edit_state(&library, &history))
    }

    fn import_new_folder(&mut self, n: usize) -> Result<Vec<PathBuf>> {
        self.import_folder(n, true)
    }

    fn old_folder(&mut self, n: usize) -> Result<Vec<PathBuf>> {
        self.import_folder(n, false)
    }

    fn start_tool_on(&mut self, photos: &[PathBuf]) -> Result<()> {
        self.lighttable()?;
        let before = self.call(&["photoflow_runs"])?;
        let paths: Vec<String> = photos.iter().map(|p| p.to_string_lossy().into()).collect();
        let mut args = vec!["process"];
        args.extend(paths.iter().map(String::as_str));
        self.call(&args)?;
        self.wait_photoflow(&before)
    }

    fn processed(&mut self) -> Result<Vec<PathBuf>> {
        Ok(self
            .call(&["processed"])?
            .split('\u{1f}')
            .filter(|p| !p.is_empty())
            .map(PathBuf::from)
            .collect())
    }

    fn select(&mut self, photos: &[PathBuf]) -> Result<()> {
        self.lighttable()?;
        let paths: Vec<String> = photos.iter().map(|p| p.to_string_lossy().into()).collect();
        let mut args = vec!["select"];
        args.extend(paths.iter().map(String::as_str));
        self.call(&args)?;
        Ok(())
    }

    fn selection(&mut self) -> Result<Vec<PathBuf>> {
        Ok(self
            .call(&["selection"])?
            .split('\u{1f}')
            .filter(|p| !p.is_empty())
            .map(PathBuf::from)
            .collect())
    }

    fn pano(&mut self, action: &str) -> Result<()> {
        self.lighttable()?;
        self.call(&["pano", action])?;
        Ok(())
    }

    fn tags_of(&mut self, photo: &Path) -> Result<Vec<String>> {
        Ok(self
            .call(&["tags_of", &photo.to_string_lossy()])?
            .split('\u{1f}')
            .filter(|t| !t.is_empty())
            .map(String::from)
            .collect())
    }

    fn edits(&mut self, photos: &[PathBuf]) -> Result<Vec<(String, Vec<String>)>> {
        self.lighttable()?;
        let mut out = Vec::new();
        for photo in photos {
            let p = photo.to_string_lossy().to_string();
            self.call(&["touch", &p])?;
            settled_sidecar(photo);
            let fp = sidecar::read(&sidecar_of(photo))?.fingerprint();
            let state = self.call(&["state", &p])?;
            let mut tags: Vec<String> = state
                .splitn(3, '\t')
                .nth(2)
                .unwrap_or("")
                .split('\u{1f}')
                .filter(|t| !t.is_empty())
                .map(String::from)
                .collect();
            tags.sort();
            let own = tags
                .iter()
                .any(|t| t.starts_with(crate::edit_state::EDIT_TAG_PREFIX));
            out.push((if own { fp } else { String::new() }, tags));
        }
        Ok(out)
    }

    fn unedited_from(&mut self, file: &str, year: u16) -> Result<()> {
        let raw = locate(&self.photo_root, year, file)?;
        self.current = Some(self.import_copy(&raw)?);
        Ok(())
    }

    fn current_photo(&self) -> Result<PathBuf> {
        self.current()
    }

    fn tag(&mut self, tag: &str) -> Result<()> {
        let p = self.current()?.to_string_lossy().to_string();
        self.call(&["tag", &p, tag])?;
        Ok(())
    }

    fn untag(&mut self, prefix: &str) -> Result<()> {
        let p = self.current()?.to_string_lossy().to_string();
        self.call(&["untag", &p, prefix])?;
        Ok(())
    }

    fn tags(&mut self) -> Result<Vec<String>> {
        let p = self.current()?.to_string_lossy().to_string();
        let state = self.call(&["state", &p])?;
        Ok(state
            .splitn(3, '\t')
            .nth(2)
            .unwrap_or("")
            .split('\u{1f}')
            .filter(|t| !t.is_empty())
            .map(String::from)
            .collect())
    }

    fn sidecar_text(&mut self) -> Result<String> {
        let photo = self.current()?;
        self.lighttable()?;
        self.call(&["touch", &photo.to_string_lossy()])?;
        settled_sidecar(&photo);
        Ok(std::fs::read_to_string(sidecar_of(&photo))?)
    }

    fn render(&mut self, size: u32) -> Result<crate::aurora::Image> {
        let photo = self.current()?;
        let xmp_text = self.sidecar_text()?;
        if self.renderer.is_none() {
            self.renderer = Some(crate::darktable::Darktable::prepare(
                &self.cli.0,
                &self.cli.1,
                &self.work.join("darktable-cli"),
            )?);
        }
        let dir = self.work.join("renders");
        std::fs::create_dir_all(&dir)?;
        let name = photo
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let xmp = dir.join(format!("{}-{name}.xmp", self.photos));
        let out = dir.join(format!("{}-{name}.pfm", self.photos));
        std::fs::write(&xmp, xmp_text)?;
        self.renderer
            .as_ref()
            .unwrap()
            .render(&photo, &xmp, &out, size)?;
        crate::aurora::load_pfm(&out)
    }
}

/// Waits (up to 10 s) until two reads of the sidecar 0.5 s apart are the same.
fn settled_sidecar(photo: &Path) {
    let start = Instant::now();
    let mut last = std::fs::read_to_string(sidecar_of(photo)).ok();
    while start.elapsed() < Duration::from_secs(10) {
        std::thread::sleep(Duration::from_millis(500));
        let now = std::fs::read_to_string(sidecar_of(photo)).ok();
        if now == last {
            return;
        }
        last = now;
    }
}

fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

/// The private configuration starts with a new library, for which darktable shows its welcome
/// dialog; it isn't needed for an unattended run.
fn without_welcome_screen(rc: &str) -> String {
    let mut out: Vec<&str> = rc
        .lines()
        .filter(|l| !l.starts_with("ui/show_welcome_screen="))
        .collect();
    out.push("ui/show_welcome_screen=FALSE");
    out.join("\n") + "\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn welcome_screen_is_switched_off() {
        let rc = without_welcome_screen("a=1\nui/show_welcome_screen=TRUE\nb=2\n");
        assert_eq!(rc, "a=1\nb=2\nui/show_welcome_screen=FALSE\n");
    }
}
