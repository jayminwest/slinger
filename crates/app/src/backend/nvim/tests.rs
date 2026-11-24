use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::*;
use crate::nvim::{Heard, Session};

/// The `nvim` this machine has, or `None` (and a line on stderr): a test
/// that needs one skips without it.
pub(crate) fn nvim() -> Option<PathBuf> {
    let found = find(std::env::var_os("PATH").as_deref(), None, "");
    if found.is_none() {
        eprintln!("SKIPPED: no nvim on PATH or in the usual places");
    }
    found
}

fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<OsString> {
    let pairs: Vec<(String, OsString)> =
        pairs.iter().map(|(k, v)| ((*k).to_string(), OsString::from(v))).collect();
    move |name| pairs.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone())
}

/// A file named `nvim` that anyone may run, in `dir` (never run).
fn fake(dir: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join("nvim");
    std::fs::write(&path, "").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

#[test]
fn slinger_nvim_turns_it_on_or_off_and_the_config_picks_which() {
    let dir = tempfile::tempdir().unwrap();
    let bin = dir.path().join("bin");
    std::fs::create_dir(&bin).unwrap();
    let program = fake(&bin);
    let path = bin.to_str().unwrap();
    let resolve = |nvim: &Nvim, pairs: &[(&str, &str)]| Setup::resolve(nvim, true, &env(pairs));
    let auto = resolve(&Nvim::Auto, &[("PATH", path)]).unwrap().unwrap();
    assert_eq!((&auto.program, auto.clean), (&program, false), "the config turns it on");
    assert!(resolve(&Nvim::Off, &[("PATH", path)]).unwrap().is_none());
    let driven = Setup::resolve(&Nvim::Auto, false, &env(&[("PATH", path)])).unwrap();
    assert!(driven.is_none(), "a driven window: only SLINGER_NVIM turns it on");
    assert!(resolve(&Nvim::Auto, &[("SLINGER_NVIM", "off"), ("PATH", path)]).unwrap().is_none());
    let on = resolve(&Nvim::Off, &[("SLINGER_NVIM", "clean"), ("PATH", path)]).unwrap().unwrap();
    assert_eq!((on.program, on.clean, on.clip), (program.clone(), true, None), "the system's");
    let pairs = [("SLINGER_NVIM", "clean"), ("PATH", path)];
    let driven = Setup::resolve(&Nvim::Auto, false, &env(&pairs)).unwrap().unwrap();
    assert_eq!(driven.clip, Some(driven.scratch.join("clip")), "a driven window's own clipboard");
    let home = dir.path().to_str().unwrap();
    let mine = Nvim::Path("~/bin/nvim".into());
    let on = resolve(&mine, &[("SLINGER_NVIM", "user"), ("HOME", home)]).unwrap().unwrap();
    assert_eq!((on.program, on.clean), (program, false));
    assert!(on.scratch.ends_with("slinger-nvim"), "state and cache go to slinger's own place");
    let gone = Nvim::Path("~/nope/nvim".into());
    let why = resolve(&gone, &[("SLINGER_NVIM", "user"), ("HOME", home)]).unwrap_err();
    assert!(why.starts_with("no nvim at ") && why.ends_with("nope/nvim"), "{why}");
}

#[test]
fn auto_looks_on_the_path_then_in_the_usual_places() {
    let dir = tempfile::tempdir().unwrap();
    let (first, second) = (dir.path().join("a"), dir.path().join("b"));
    std::fs::create_dir(&first).unwrap();
    std::fs::create_dir(&second).unwrap();
    let program = fake(&second);
    std::fs::write(first.join("nvim"), "").unwrap(); // not runnable: passed over
    let path = std::env::join_paths([&first, &second]).unwrap();
    assert_eq!(find(Some(&path), None, "me"), Some(program));
    let nix = dir.path().join(".nix-profile/bin");
    std::fs::create_dir_all(&nix).unwrap();
    let program = fake(&nix);
    let found = find(Some(OsString::from(&first).as_os_str()), Some(dir.path()), "me").unwrap();
    // A machine with Homebrew's nvim finds it first.
    assert!(
        found == program || found.starts_with("/opt/homebrew") || found.starts_with("/usr/local")
    );
    assert!(PLACES.iter().any(|p| p.contains("$USER")));
}

/// Waits for messages from `process` until `done` says so or 10 s pass.
fn hear_until(
    process: &mut Process,
    session: &mut Session,
    mut done: impl FnMut(&Heard, &Session) -> bool,
) {
    let until = Instant::now() + Duration::from_secs(10);
    while Instant::now() < until {
        let Some((message, _)) = process.wait(Duration::from_millis(100)) else { continue };
        let mut out = Vec::new();
        let heard = session.hear(&message, &mut out);
        process.send(&out);
        if done(&heard, session) {
            return;
        }
    }
    panic!("nvim said nothing for 10 s");
}

#[test]
fn an_nvim_edits_the_text_and_writes_nothing_but_its_state_and_cache() {
    let Some(program) = nvim() else { return };
    let dir = tempfile::tempdir().unwrap();
    let setup = Setup::clean_in(program, dir.path());
    let mut process = Process::start(&setup, &Wake::none()).unwrap();
    let mut out = Vec::new();
    let mut session = Session::start("plan", (false, usize::MAX), &mut out);
    session.input("A, then ship<Esc>", &mut out);
    process.send(&out);
    hear_until(&mut process, &mut session, |_, s| s.text().as_deref() == Some("plan, then ship"));
    let mut out = Vec::new();
    session.input(":qa!<CR>", &mut out);
    process.send(&out);
    let until = Instant::now() + Duration::from_secs(10);
    while !process.heard().1 {
        assert!(Instant::now() < until, "nvim did not quit");
        std::thread::sleep(Duration::from_millis(20));
    }
    let mut written: Vec<String> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    written.sort();
    assert_eq!(written, ["cache", "state"], "nothing in the config or data homes, nor the cwd");
}
