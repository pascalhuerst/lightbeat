//! Which build this is.

/// The release this binary was built as ("0.1.0-beta.12"), or the crate
/// version for a build made by hand.
pub fn version() -> &'static str {
    option_env!("LIGHTBEAT_RELEASE").unwrap_or(env!("CARGO_PKG_VERSION"))
}

/// The commit it was built from; empty when that was not known.
pub fn commit() -> &'static str {
    env!("LIGHTBEAT_COMMIT")
}

pub fn short_commit() -> &'static str {
    let c = commit();
    &c[..c.len().min(7)]
}

/// True for the builds CI publishes. A build from a checkout is not one, and
/// does not check for updates: it would always find the latest release newer.
pub fn is_release() -> bool {
    option_env!("LIGHTBEAT_RELEASE").is_some()
}

/// For the menus and the title: "0.1.0-beta.12 (a1b2c3d)".
pub fn label() -> String {
    match (is_release(), short_commit()) {
        (true, "") => version().to_string(),
        (true, c) => format!("{} ({c})", version()),
        (false, "") => format!("{} (development build)", version()),
        (false, c) => format!("{} (development build {c})", version()),
    }
}
