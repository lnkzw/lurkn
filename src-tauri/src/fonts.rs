//! Enumerating the fonts installed on this machine.
//!
//! Lives in the shell rather than a domain crate: it is a desktop capability,
//! like launching a browser. `fontdb` reads the platform's own font
//! directories, so this works the same way on Windows and macOS.

use std::sync::OnceLock;

/// Cached because the installed set does not change while Lurkn runs. The scan
/// itself is quick — around 50ms for ~400 families on a typical Linux box — so
/// it is done on first use rather than at startup.
static FAMILIES: OnceLock<Vec<String>> = OnceLock::new();

pub fn system_families() -> &'static [String] {
    FAMILIES.get_or_init(|| {
        let mut db = fontdb::Database::new();
        db.load_system_fonts();

        let raw: Vec<String> = db
            .faces()
            .flat_map(|face| face.families.iter().map(|(name, _)| name.clone()))
            .collect();

        let families = usable_families(raw);
        tracing::debug!(count = families.len(), "loaded system font families");
        families
    })
}

/// Tidy a raw list of family names into something worth showing in a menu.
///
/// Kept separate from the scan so the rules are testable without touching the
/// filesystem.
fn usable_families(raw: Vec<String>) -> Vec<String> {
    let mut names: Vec<String> = raw
        .into_iter()
        .map(|name| name.trim().to_owned())
        .filter(|name| {
            // A name Lurkn cannot put in a stylesheet is not worth offering, and
            // the leading-dot families are macOS internals users never pick.
            !name.is_empty()
                && !name.starts_with('.')
                && lurkn_core::settings::sanitize_font_name(name).as_deref() == Some(name.as_str())
        })
        .collect();

    // One entry per family, whatever mix of cases the faces report.
    names.sort_by_key(|name| name.to_lowercase());
    names.dedup_by_key(|name| name.to_lowercase());
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    fn families(raw: &[&str]) -> Vec<String> {
        usable_families(raw.iter().map(|s| (*s).to_owned()).collect())
    }

    #[test]
    fn sorts_case_insensitively_and_deduplicates() {
        assert_eq!(
            families(&["Ubuntu", "arial", "Arial", "ubuntu", "Noto Sans"]),
            ["arial", "Noto Sans", "Ubuntu"]
        );
    }

    #[test]
    fn drops_names_that_could_not_be_used_in_a_stylesheet() {
        // Only the names that survive sanitising unchanged are offered.
        assert_eq!(
            families(&["Fine Name", "Bad;Name", "Quote\"Name"]),
            ["Fine Name"]
        );
    }

    #[test]
    fn drops_platform_internal_families() {
        assert_eq!(
            families(&[".SF NS", ".LastResort", "Helvetica"]),
            ["Helvetica"]
        );
    }

    #[test]
    fn drops_blank_entries() {
        assert_eq!(families(&["", "   ", "Real"]), ["Real"]);
    }

    /// Reads this machine's real fonts. Ignored by default so a CI runner with
    /// no fonts installed cannot fail the build; run with
    /// `cargo test -p lurkn --lib fonts -- --ignored --nocapture`.
    #[test]
    #[ignore = "reads the fonts installed on this machine"]
    fn the_real_scan_finds_usable_families() {
        let families = system_families();
        println!(
            "{} families, first few: {:?}",
            families.len(),
            &families[..families.len().min(8)]
        );

        assert!(!families.is_empty(), "no font families found");
        assert!(families.iter().all(|name| !name.is_empty()));

        // Anything offered must be applicable, or picking it does nothing.
        for name in families {
            assert_eq!(
                lurkn_core::settings::sanitize_font_name(name).as_deref(),
                Some(name.as_str()),
                "{name:?} would not survive sanitising"
            );
        }
    }

    /// Every offered name must round-trip the sanitiser, or picking it would
    /// silently fall back to the system font.
    #[test]
    fn every_offered_family_survives_sanitising() {
        for name in families(&["Ubuntu Mono", "Noto Sans CJK JP", "DejaVu Sans"]) {
            assert_eq!(
                lurkn_core::settings::sanitize_font_name(&name).as_deref(),
                Some(name.as_str())
            );
        }
    }
}
