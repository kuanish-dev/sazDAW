//! Plugins: the "Scan plugins" button, the list of installed ones (the Settings page), disabling a plugin and the report of
//! the previous session crashing inside a plugin.

use std::path::PathBuf;

use daw_contracts::InstalledPlugin;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::RpcTable;

/// The `plugins.list` answer: where we search and what is in the catalogs now.
#[derive(Serialize)]
struct PluginList {
    folders: Vec<PathBuf>,
    plugins: Vec<InstalledPlugin>,
}

#[derive(Deserialize, JsonSchema)]
struct SetDisabled {
    /// Plugin kind from `plugins.list` (`vst3:surge_xt`).
    kind: String,
    disabled: bool,
}

pub(crate) fn register(table: &mut RpcTable) {
    // The answer `{ found, effects, instruments, failed: [{ path, reason }] }` is the `ScanReport` itself.
    table.add(
        "plugins.scan",
        "Finds the installed VST3 plugins (built-in instruments and effects are not plugins and need no scan).",
        |c, ()| c.plugins.scan(),
    );
    table.add(
        "plugins.list",
        "Lists the found plugins (instruments first, then effects) and the folders \"Scan plugins\" searches.",
        |c, ()| Ok(PluginList { folders: c.plugins.folders(), plugins: c.plugins.installed() }),
    );
    table.add(
        "plugins.setDisabled",
        "Disables a plugin (it is not offered or created; a project using it opens with a warning) or enables it back.",
        |c, p: SetDisabled| c.plugins.set_disabled(&p.kind, p.disabled),
    );
    table.add(
        "plugins.lastCrash",
        "Returns the plugin inside which the previous session ended abruptly ({ kind, name, project, places }), or null.",
        |c, ()| Ok(c.plugin_crash.last_crash()),
    );
    table.add("plugins.dismissCrash", "Marks the crash report as shown: plugins.lastCrash returns null from then on.", |c, ()| {
        c.plugin_crash.dismiss();
        Ok(())
    });
}
