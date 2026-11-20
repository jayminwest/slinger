//! What `scripts/bundle.sh` builds slinger.app from: the Info.plist it fills
//! in. The script itself is not part of the gates; it verifies the bundle it
//! makes (`codesign --verify --strict`).

/// The plist the script fills in: `@VERSION@` and `@BUILD@` are its slots.
const PLIST: &str = include_str!("../../Info.plist");

/// The `<string>` value of `key` in [`PLIST`].
fn value(key: &str) -> Option<&'static str> {
    let after = PLIST.split_once(&format!("<key>{key}</key>"))?.1;
    let rest = after.trim_start().strip_prefix("<string>")?;
    Some(rest.split_once("</string>")?.0)
}

#[test]
fn the_plist_names_the_app_program_and_leaves_the_version_and_build_to_the_script() {
    assert_eq!(value("CFBundleExecutable"), Some("slinger-app"));
    assert_eq!(value("CFBundleShortVersionString"), Some("@VERSION@"));
    assert_eq!(value("CFBundleVersion"), Some("@BUILD@"));
}
