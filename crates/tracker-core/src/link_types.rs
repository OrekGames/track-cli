//! Canonical link type names to one backend's native names.

use std::collections::HashMap;

/// Maps the canonical link type names the CLI sends (`relates`, `depends`,
/// `required`, `duplicates`, `duplicated-by`) to one backend's native names.
///
/// A name resolves to the user's override from `[<backend>.link_mappings]`,
/// else the backend's default, else itself, so admin-defined types pass
/// through. Lookups are exact and case-sensitive; values are returned
/// verbatim.
#[derive(Debug, Clone)]
pub struct LinkTypeMap {
    defaults: &'static [(&'static str, &'static str)],
    overrides: HashMap<String, String>,
}

impl LinkTypeMap {
    /// `defaults` are (canonical, native) pairs; `overrides` (canonical ->
    /// native) take precedence over them.
    pub fn new(
        defaults: &'static [(&'static str, &'static str)],
        overrides: HashMap<String, String>,
    ) -> Self {
        Self {
            defaults,
            overrides,
        }
    }

    /// The native link type name for `canonical`.
    pub fn resolve(&self, canonical: &str) -> String {
        if let Some(native) = self.overrides.get(canonical) {
            return native.clone();
        }
        self.defaults
            .iter()
            .find(|(name, _)| *name == canonical)
            .map_or(canonical, |&(_, native)| native)
            .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_prefers_override_then_default_then_name() {
        let overrides = HashMap::from([
            ("depends".to_string(), "Custom Depend".to_string()),
            ("clones".to_string(), "Cloners".to_string()),
        ]);
        let map = LinkTypeMap::new(&[("relates", "Relates"), ("depends", "Depend")], overrides);

        // An override beats a default, and can add a name with no default
        assert_eq!(map.resolve("depends"), "Custom Depend");
        assert_eq!(map.resolve("clones"), "Cloners");
        // A default with no override is kept
        assert_eq!(map.resolve("relates"), "Relates");
        // An unknown name passes through
        assert_eq!(map.resolve("nonexistent"), "nonexistent");
        // Override and default keys are both case-sensitive
        assert_eq!(map.resolve("DEPENDS"), "DEPENDS");
        assert_eq!(map.resolve("RELATES"), "RELATES");
    }
}
