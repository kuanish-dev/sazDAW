//! `templates.*` and `project.createFromTemplate` methods: thin adapters over `ProjectTemplates`.

use std::path::PathBuf;

use schemars::JsonSchema;
use serde::Deserialize;

use crate::RpcTable;

#[derive(Deserialize, JsonSchema)]
struct Name {
    name: String,
}

#[derive(Deserialize, JsonSchema)]
struct Create {
    /// The new project's `.dawproj` file (the folder is created).
    path: PathBuf,
    /// Template name from `templates.list`.
    template: String,
}

pub(crate) fn register(table: &mut RpcTable) {
    table.add("templates.list", "Lists project template names, sorted by name.", |c, ()| c.templates.list());
    table.add_flushing(
        "templates.save",
        "Saves the structure of the current project as a template: tracks, instruments, graph, effects, tempo — without clips and automation.",
        |c, p: Name| c.templates.save(&p.name),
    );
    table.add("templates.remove", "Removes a project template by name.", |c, p: Name| c.templates.remove(&p.name));
    table.add("project.createFromTemplate", "Creates a project folder at a path from a template and opens it.", |c, p: Create| {
        c.templates.create_project(&p.template, &p.path)
    });
}
