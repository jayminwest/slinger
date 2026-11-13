//! Where the app finds the daemon (docs/service.md, "Socket"), as the CLI
//! does: the socket the config sets, else the one the running daemon named
//! in its data directory, else the default. A name left by a daemon that
//! died is passed over ([`control::live_socket`]), so a stale name never
//! holds the window on a dead socket (slinger-41f3b9). [`super::ConfigFiles`] stamps the name's file, so the
//! window follows a daemon that starts, moves or stops after it opened.

use std::path::{Path, PathBuf};

use slinger_control as control;

/// `set` (the config's socket), else the live socket the daemon serving
/// `data_dir` named, else [`control::default_socket`].
pub(super) fn socket(set: Option<PathBuf>, data_dir: Option<&Path>) -> PathBuf {
    set.or_else(|| data_dir.and_then(control::live_socket)).unwrap_or_else(control::default_socket)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::net::UnixListener;

    use super::super::tests::with_socket;
    use super::super::{ConfigFiles, Settings};
    use super::*;

    #[test]
    fn the_config_then_the_live_named_socket_then_the_default() {
        let data = tempfile::tempdir().unwrap();
        assert_eq!(socket(None, None), control::default_socket());
        assert_eq!(socket(None, Some(data.path())), control::default_socket(), "nothing named");
        with_socket(|live| {
            control::name_socket(data.path(), live).unwrap();
            assert_eq!(socket(None, Some(data.path())), live);
            let set = PathBuf::from("/set/d.sock");
            assert_eq!(socket(Some(set.clone()), Some(data.path())), set, "the config wins");
        });
    }

    #[test]
    fn a_name_left_by_a_daemon_that_died_is_passed_over() {
        let (data, sockets) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let dead = sockets.path().join("d.sock");
        // A socket file nothing listens on, as an unclean death leaves it.
        drop(UnixListener::bind(&dead).unwrap());
        control::name_socket(data.path(), &dead).unwrap();
        assert!(dead.exists());
        assert_eq!(socket(None, Some(data.path())), control::default_socket());
        assert_eq!(socket(Some(dead.clone()), Some(data.path())), dead, "a set socket is kept");
    }

    #[test]
    fn settings_follow_a_daemon_that_names_its_socket_after_they_load() {
        let (config, data) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let files = ConfigFiles {
            data_dir: Some(data.path().to_owned()),
            ..ConfigFiles::at(config.path().join("config.toml"))
        };
        let mut settings = Settings::load(files);
        assert_eq!(settings.socket(), control::default_socket());
        with_socket(|live| {
            control::name_socket(data.path(), live).unwrap();
            assert!(settings.refresh(false), "the name is new");
            assert_eq!(settings.socket(), live);
            fs::write(config.path().join("config.toml"), "[daemon]\nsocket = \"/x.sock\"\n")
                .unwrap();
            assert!(settings.refresh(false));
            assert_eq!(settings.socket(), PathBuf::from("/x.sock"), "the config wins");
            fs::remove_file(config.path().join("config.toml")).unwrap();
            assert!(settings.refresh(false));
            control::unname_socket(data.path()).unwrap();
            assert!(settings.refresh(false), "the daemon took its name back");
        });
        assert_eq!(settings.socket(), control::default_socket());
    }
}
