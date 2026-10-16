//! User preferences (the Settings page): profile, autosave, appearance.

use schemars::JsonSchema;
use serde::Deserialize;

use daw_contracts::Preferences;

use crate::RpcTable;

#[derive(Deserialize, JsonSchema)]
struct Set {
    preferences: Preferences,
}

pub(crate) fn register(table: &mut RpcTable) {
    table.add(
        "preferences.get",
        "Returns the user preferences: profile (author, projects folder), autosave interval and appearance.",
        |c, ()| Ok(c.preferences.get()),
    );
    table.add(
        "preferences.set",
        "Saves all user preferences and returns the saved ones: autosaveSeconds 0 (off) or 10..600, \
         uiScale 0.8..1.5, author up to 100 characters; invalid values are rejected, the autosave interval changes at once.",
        |c, p: Set| c.preferences.set(p.preferences),
    );
}
