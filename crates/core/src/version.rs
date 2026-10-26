//! SolidCraft version tags (`v0.22.1`, `0.22.1`).

use semver::Version;

/// `v0.22.1` and `0.22.1` both parse. Anything else is not a SolidCraft release tag.
pub fn parse(tag: &str) -> Option<Version> {
    let bare = tag.strip_prefix('v').unwrap_or(tag);
    Version::parse(bare).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orders_real_solidcraft_versions() {
        let older = parse("v0.18.2").expect("0.18.2");
        let newer = parse("0.22.1").expect("0.22.1");
        assert!(newer > older);
        assert!(parse("not-a-version").is_none());
        assert_eq!(
            parse("v0.12.0+blendfix")
                .expect("build metadata")
                .to_string(),
            "0.12.0+blendfix"
        );
    }
}
