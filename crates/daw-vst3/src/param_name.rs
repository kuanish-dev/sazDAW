//! Plugin parameter names in the contract: `p<id>`, where `id` is the VST3 parameter id.
//! The only place that knows this format (catalog, engine queue, plugin window).

/// Contract parameter name for VST3 parameter `id`.
pub fn name(id: u32) -> String {
    format!("p{id}")
}

/// `p<id>` -> VST3 parameter id; any other name gives `None`.
pub fn parse(name: &str) -> Option<u32> {
    name.strip_prefix('p')?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_map_to_ids_and_back() {
        assert_eq!(parse("p0"), Some(0));
        assert_eq!(parse("p4294967295"), Some(u32::MAX));
        assert_eq!(parse("drive"), None);
        assert_eq!(parse("p"), None);
        assert_eq!(parse("p-1"), None);
        assert_eq!(parse(&name(42)), Some(42));
        assert_eq!(name(7), "p7");
    }
}
