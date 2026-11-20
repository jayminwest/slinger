//! Where an agent's program comes from and the `PATH` it runs with
//! (docs/service.md "The daemon"). A slingerd that the app or launchd
//! starts gets a short `PATH` (`/usr/bin:/bin:/usr/sbin:/sbin`): Claude Code
//! is not on it, nor are the tools an agent's hooks, MCP servers and Bash
//! calls run. So the daemon reads its user's login-shell `PATH` once, as it
//! starts (fixed folders when there is no shell to ask), and that one `PATH`
//! is both where Claude Code is looked for and what every agent gets.
//! Claude Code is looked for at each start, so an install made after the
//! daemon started is found. The environment is read here, once, and passed
//! on as a [`Search`]: the rest is pure functions over values.

use std::ffi::{OsStr, OsString};
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// The program's file name.
const CLAUDE: &str = "claude";

/// Where Claude Code is looked for after each folder of the `PATH`, in
/// order: `~` is the home folder, `{user}` the user's name.
const PLACES: [&str; 6] = [
    "~/.local/bin/claude",
    "~/.claude/local/claude",
    "/opt/homebrew/bin/claude",
    "/usr/local/bin/claude",
    "~/.nix-profile/bin/claude",
    "/etc/profiles/per-user/{user}/bin/claude",
];

/// The folders an agent's `PATH` starts with when no login shell says
/// what it is, in the same notation as [`PLACES`].
const FOLDERS: [&str; 7] = [
    "~/.local/bin",
    "/opt/homebrew/bin",
    "/usr/local/bin",
    "~/.nix-profile/bin",
    "/etc/profiles/per-user/{user}/bin",
    "/run/current-system/sw/bin",
    "/nix/var/nix/profiles/default/bin",
];

/// How long the login shell may take to say its `PATH`.
const SHELL_WAIT: Duration = Duration::from_secs(3);

/// The environment Claude Code is looked for in, read once.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Search {
    path: Option<OsString>,
    home: Option<PathBuf>,
    user: Option<String>,
}

impl Search {
    /// A search over `path`, with `home` and `user` for the fixed places.
    #[must_use]
    pub fn new(path: Option<OsString>, home: Option<PathBuf>, user: Option<String>) -> Self {
        Self { path, home, user }
    }

    /// The daemon's own environment as it is: its `PATH`, `HOME` and `USER`.
    #[must_use]
    pub fn inherited() -> Self {
        let home = std::env::var_os("HOME").filter(|h| !h.is_empty()).map(PathBuf::from);
        let user = std::env::var("USER").ok().filter(|u| !u.is_empty());
        Self::new(std::env::var_os("PATH"), home, user)
    }

    /// [`Search::inherited`], with the `PATH` the user's login shell
    /// (`$SHELL -l`) has in front of the daemon's own, or the fixed
    /// folders when there is no shell or it does not answer in time.
    #[must_use]
    pub fn login() -> Self {
        let own = Self::inherited();
        let shell = std::env::var_os("SHELL").map(PathBuf::from).filter(|s| s.is_absolute());
        let login = shell.and_then(|shell| login_path(&shell));
        let (home, user) = (own.home.as_deref(), own.user.as_deref());
        let path = agents_path(login.as_deref(), own.path.as_deref(), home, user);
        Self { path, ..own }
    }

    /// The `PATH` every agent's process gets; `None` for no `PATH` at all.
    #[must_use]
    pub fn path(&self) -> Option<&OsStr> {
        self.path.as_deref()
    }

    /// The Claude Code this search finds now.
    ///
    /// # Errors
    ///
    /// The places it looked in, when none holds an executable `claude`.
    pub fn claude(&self) -> Result<PathBuf, Vec<PathBuf>> {
        let all = candidates(self.path.as_deref(), self.home.as_deref(), self.user.as_deref());
        pick(&all, executable).cloned().ok_or(all)
    }
}

/// Where Claude Code is looked for, in order: `claude` in each folder of
/// `path` (empty entries, which mean the working directory, left out),
/// then [`PLACES`]. A place under `~` is left out without a `home`, one
/// that names the user without a `user`.
#[must_use]
pub fn candidates(path: Option<&OsStr>, home: Option<&Path>, user: Option<&str>) -> Vec<PathBuf> {
    let on_path = path.into_iter().flat_map(std::env::split_paths);
    let on_path = on_path.filter(|f| !f.as_os_str().is_empty()).map(|f| f.join(CLAUDE));
    on_path.chain(PLACES.iter().filter_map(|p| expand(p, home, user))).collect()
}

/// The first of `candidates` that `executable` holds for.
#[must_use]
pub fn pick(candidates: &[PathBuf], executable: impl Fn(&Path) -> bool) -> Option<&PathBuf> {
    candidates.iter().find(|c| executable(c))
}

/// The `PATH` an agent gets: the login shell's `login`, or [`FOLDERS`]
/// when there is none, then each folder of the daemon's `own` not named
/// yet. Empty entries are left out. `None` when that leaves nothing.
#[must_use]
pub fn agents_path(
    login: Option<&OsStr>,
    own: Option<&OsStr>,
    home: Option<&Path>,
    user: Option<&str>,
) -> Option<OsString> {
    let first: Vec<PathBuf> = match login {
        Some(login) => std::env::split_paths(login).collect(),
        None => FOLDERS.iter().filter_map(|f| expand(f, home, user)).collect(),
    };
    let own = own.into_iter().flat_map(std::env::split_paths);
    let mut folders: Vec<PathBuf> = Vec::new();
    for folder in first.into_iter().chain(own) {
        if !folder.as_os_str().is_empty() && !folders.contains(&folder) {
            folders.push(folder);
        }
    }
    // A folder with a ':' in it cannot be on a PATH: such a PATH is not used.
    (!folders.is_empty()).then(|| std::env::join_paths(folders).ok()).flatten()
}

/// The daemon's `base` PATH with the folder of `slinger` first, when
/// `slinger` is an absolute path, so an agent's bare `slinger` is the one
/// its hooks run. `None` (no PATH set) when there is neither.
#[must_use]
pub fn path_first(slinger: &Path, base: Option<&OsStr>) -> Option<OsString> {
    let folder = slinger.parent().filter(|_| slinger.is_absolute()).map(Path::to_owned);
    // An empty PATH adds nothing: not an empty entry, the working directory.
    let base = base.filter(|p| !p.is_empty());
    if folder.is_none() && base.is_none() {
        return None;
    }
    let folders = folder.into_iter().chain(base.into_iter().flat_map(std::env::split_paths));
    // A folder with a ':' in it cannot be on a PATH: leave it as it is.
    std::env::join_paths(folders).ok().or_else(|| base.map(OsStr::to_owned))
}

/// The `PATH` a login-shell environment listing (`env`'s output) holds.
#[must_use]
pub fn path_in(listing: &str) -> Option<OsString> {
    let path = listing.lines().rev().find_map(|line| line.strip_prefix("PATH="))?;
    path.contains('/').then(|| path.into())
}

/// `place` with `~` and `{user}` filled in; `None` when one is needed and
/// not known.
fn expand(place: &str, home: Option<&Path>, user: Option<&str>) -> Option<PathBuf> {
    let place = match (place.contains("{user}"), user) {
        (false, _) => place.to_string(),
        (true, Some(user)) => place.replace("{user}", user),
        (true, None) => return None,
    };
    match place.strip_prefix("~/") {
        Some(rest) => home.map(|home| home.join(rest)),
        None => Some(PathBuf::from(place)),
    }
}

/// Whether `path` is a file someone may run.
fn executable(path: &Path) -> bool {
    std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

/// The `PATH` that `shell`, started as a login shell, exports; `None` when
/// it cannot start, fails, or takes longer than [`SHELL_WAIT`] (it is killed).
fn login_path(shell: &Path) -> Option<OsString> {
    let mut child = Command::new(shell)
        .args(["-l", "-c", "env"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let deadline = Instant::now() + SHELL_WAIT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    };
    let mut listing = String::new();
    child.stdout.take()?.read_to_string(&mut listing).ok()?;
    status.success().then(|| path_in(&listing)).flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(all: &[&str]) -> Vec<PathBuf> {
        all.iter().map(PathBuf::from).collect()
    }

    #[test]
    fn candidates_are_the_path_then_the_fixed_places() {
        let path = OsStr::new("/a::/b");
        let all = candidates(Some(path), Some(Path::new("/h")), Some("me"));
        let want = paths(&[
            "/a/claude",
            "/b/claude",
            "/h/.local/bin/claude",
            "/h/.claude/local/claude",
            "/opt/homebrew/bin/claude",
            "/usr/local/bin/claude",
            "/h/.nix-profile/bin/claude",
            "/etc/profiles/per-user/me/bin/claude",
        ]);
        assert_eq!(all, want, "the empty entry is dropped");
    }

    #[test]
    fn without_a_path_home_or_user_their_places_are_left_out() {
        let all = candidates(None, None, None);
        assert_eq!(all, paths(&["/opt/homebrew/bin/claude", "/usr/local/bin/claude"]));
        assert_eq!(candidates(Some(OsStr::new("")), None, None).len(), 2, "an empty PATH");
    }

    #[test]
    fn the_first_executable_candidate_is_picked() {
        let all = paths(&["/a/claude", "/b/claude", "/c/claude"]);
        let runs = |p: &Path| p.starts_with("/b") || p.starts_with("/c");
        assert_eq!(pick(&all, runs), Some(&all[1]));
        assert_eq!(pick(&all, |_| false), None);
    }

    #[test]
    fn the_login_path_comes_first_then_the_daemons_own_once() {
        let login = OsStr::new("/h/.local/bin:/opt/homebrew/bin:/usr/bin");
        let own = OsStr::new("/usr/bin:/bin::/opt/slinger");
        let path = agents_path(Some(login), Some(own), Some(Path::new("/h")), Some("me"));
        let want = "/h/.local/bin:/opt/homebrew/bin:/usr/bin:/bin:/opt/slinger";
        assert_eq!(path.as_deref(), Some(OsStr::new(want)));
    }

    #[test]
    fn without_a_login_shell_the_fixed_folders_come_first() {
        let own = OsStr::new("/usr/bin:/bin:/usr/local/bin");
        let path = agents_path(None, Some(own), Some(Path::new("/h")), Some("me"));
        let want = "/h/.local/bin:/opt/homebrew/bin:/usr/local/bin:/h/.nix-profile/bin:\
                    /etc/profiles/per-user/me/bin:/run/current-system/sw/bin:\
                    /nix/var/nix/profiles/default/bin:/usr/bin:/bin";
        assert_eq!(path.as_deref(), Some(OsStr::new(want)));
        let bare = agents_path(None, None, None, None).unwrap();
        assert!(
            bare.to_str().unwrap().starts_with("/opt/homebrew/bin:/usr/local/bin:"),
            "{bare:?}"
        );
    }

    #[test]
    fn the_path_is_read_from_the_shells_env_listing() {
        let listing = "Welcome!\nHOME=/h\nPATH=/h/.local/bin:/usr/bin\nSHLVL=1\n";
        assert_eq!(path_in(listing).as_deref(), Some(OsStr::new("/h/.local/bin:/usr/bin")));
        assert_eq!(path_in("HOME=/h\n"), None);
        assert_eq!(path_in("PATH=\n"), None, "an empty PATH is no answer");
    }

    #[test]
    fn a_shell_that_fails_or_does_not_exist_gives_no_path() {
        assert_eq!(login_path(Path::new("/nonexistent/shell")), None);
        assert_eq!(login_path(Path::new("/usr/bin/false")), None);
    }

    #[test]
    fn the_hooks_cli_comes_first_on_the_given_path() {
        let slinger = Path::new("/opt/slinger/bin/slinger");
        let base = Some(OsStr::new("/usr/bin:/bin"));
        let first = path_first(slinger, base);
        assert_eq!(first.as_deref(), Some(OsStr::new("/opt/slinger/bin:/usr/bin:/bin")));
        assert_eq!(path_first(Path::new("slinger"), base).as_deref(), base);
        assert_eq!(path_first(Path::new("slinger"), None), None);
        let alone = path_first(slinger, Some(OsStr::new("")));
        assert_eq!(alone.as_deref(), Some(OsStr::new("/opt/slinger/bin")));
    }
}
