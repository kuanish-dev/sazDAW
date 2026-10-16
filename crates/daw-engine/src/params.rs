//! Which params to send to a DSP object on a new snapshot: only the changed ones. Otherwise every edit (and every
//! command) would push all params of all objects as strings through `set_param` again.

use daw_contracts::Params;

/// The params in `params` whose values differ from `before`. Without `before`, all of them: the object has just arrived
/// or the params must return to the project values after automation.
pub fn changed<'a>(params: &'a Params, before: Option<&'a Params>) -> impl Iterator<Item = (&'a str, f32)> + 'a {
    params
        .iter()
        .filter(move |(name, value)| before.and_then(|b| b.get(*name)) != Some(*value))
        .map(|(name, value)| (name.as_str(), *value))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(list: &[(&str, f32)]) -> Params {
        list.iter().map(|(name, value)| ((*name).to_string(), *value)).collect()
    }

    #[test]
    fn only_new_and_changed_values_are_sent() {
        let before = params(&[("cutoff", 0.5), ("res", 0.2)]);
        let now = params(&[("cutoff", 0.5), ("res", 0.3), ("drive", 1.0)]);
        let sent: Vec<_> = changed(&now, Some(&before)).collect();
        assert_eq!(sent, [("drive", 1.0), ("res", 0.3)]);
        assert_eq!(changed(&now, None).count(), 3, "without a previous snapshot, all of them");
    }
}
