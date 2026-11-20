//! Which `slinger` a `slingerd` runs for its agents: the one beside it, also
//! when it was started through a link (`scripts/bundle.sh --install` links
//! `~/.local/bin/slingerd` into slinger.app).

use std::fs;
use std::os::unix::fs::symlink;

use tempfile::TempDir;

use crate::beside;

#[test]
fn a_slingerd_run_through_a_link_finds_the_slinger_beside_the_real_one() {
    let dir = TempDir::new().unwrap();
    let app = dir.path().join("slinger.app/Contents/MacOS");
    fs::create_dir_all(&app).unwrap();
    fs::write(app.join("slingerd"), "").unwrap();
    fs::write(app.join("slinger"), "").unwrap();
    let bin = dir.path().join("bin");
    fs::create_dir(&bin).unwrap();
    symlink(app.join("slingerd"), bin.join("slingerd")).unwrap();

    let found = beside(&bin.join("slingerd")).unwrap();
    assert_eq!(found, fs::canonicalize(&app).unwrap().join("slinger"));
}

#[test]
fn a_slinger_beside_the_link_counts_when_the_real_folder_has_none() {
    let dir = TempDir::new().unwrap();
    let real = dir.path().join("real");
    let bin = dir.path().join("bin");
    fs::create_dir(&real).unwrap();
    fs::create_dir(&bin).unwrap();
    fs::write(real.join("slingerd"), "").unwrap();
    fs::write(bin.join("slinger"), "").unwrap();
    symlink(real.join("slingerd"), bin.join("slingerd")).unwrap();

    assert_eq!(beside(&bin.join("slingerd")), Some(bin.join("slinger")));
}

#[test]
fn no_slinger_beside_means_none() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("slingerd"), "").unwrap();
    assert_eq!(beside(&dir.path().join("slingerd")), None);
}
