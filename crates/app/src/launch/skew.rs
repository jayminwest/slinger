//! Whether the daemon that answers is this app's build (docs/service.md,
//! "The daemon's own verbs"). Pure: the app's build and what `daemon_info`
//! said go in, the [`Skew`] comes out. A skew is only ever offered for a
//! restart: a restart ends running agents, so the user decides.

/// A build as `daemon_info` and [`slinger_control::stamp`] name it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Build<'a> {
    /// The package version (`--version`).
    pub version: &'a str,
    /// The build stamp: a commit, `-dirty` after it, or `dev`.
    pub stamp: &'a str,
}

impl Build<'_> {
    /// How the window names it beside `other`: its stamp, after its
    /// version when the versions differ.
    fn named(self, other: Build<'_>) -> String {
        if self.version == other.version {
            self.stamp.into()
        } else {
            format!("{} {}", self.version, self.stamp)
        }
    }
}

/// How the daemon's build stands to the app's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Skew {
    /// The same version and stamp.
    Same,
    /// Another build: the daemon's and the app's, as the window names them.
    Differs { daemon: String, app: String },
    /// It answers `bad_request` to `daemon_info`: it predates the verb, so
    /// it cannot be stopped from here either. The app's build.
    Old { app: String },
}

/// The skew between the app's build `mine` and what the daemon said to
/// `daemon_info`: its build, or the code of its error. `None` for any
/// error but `bad_request`: a daemon that does not answer is asked again
/// later.
pub fn skew(mine: Build<'_>, theirs: Result<Build<'_>, &str>) -> Option<Skew> {
    match theirs {
        Ok(theirs) if theirs == mine => Some(Skew::Same),
        Ok(theirs) => Some(Skew::Differs { daemon: theirs.named(mine), app: mine.named(theirs) }),
        Err("bad_request") => Some(Skew::Old { app: mine.stamp.into() }),
        Err(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINE: Build<'static> = Build { version: "0.2.0", stamp: "abc1234" };

    fn differs(daemon: &str, app: &str) -> Skew {
        Skew::Differs { daemon: daemon.into(), app: app.into() }
    }

    #[test]
    fn the_skew_of_each_answer() {
        let build = |version, stamp| Ok(Build { version, stamp });
        let table: [(Result<Build<'_>, &str>, Option<Skew>); 8] = [
            (build("0.2.0", "abc1234"), Some(Skew::Same)),
            (build("0.2.0", "def5678"), Some(differs("def5678", "abc1234"))),
            (build("0.2.0", "abc1234-dirty"), Some(differs("abc1234-dirty", "abc1234"))),
            (build("0.1.0", "abc1234"), Some(differs("0.1.0 abc1234", "0.2.0 abc1234"))),
            (build("0.2.0", "dev"), Some(differs("dev", "abc1234"))),
            (Err("bad_request"), Some(Skew::Old { app: "abc1234".into() })),
            (Err("unavailable"), None),
            (Err("internal"), None),
        ];
        for (theirs, want) in table {
            assert_eq!(skew(MINE, theirs), want, "{theirs:?}");
        }
    }

    #[test]
    fn two_dev_builds_of_one_version_are_the_same() {
        let dev = Build { version: "0.2.0", stamp: "dev" };
        assert_eq!(skew(dev, Ok(dev)), Some(Skew::Same));
    }
}
