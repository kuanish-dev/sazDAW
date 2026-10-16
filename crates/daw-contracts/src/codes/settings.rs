//! Preferences, presets and project templates. Codes `PR-KNN`.

crate::codes! {
    AUTHOR_TOO_LONG = "PR-101", "The author name is longer than {max} characters";
    PROJECTS_FOLDER_NOT_ABSOLUTE = "PR-102", "The projects folder must be a full path, not \"{path}\"";
    NAME_EMPTY = "PR-103", "The name is empty";
    NAME_TOO_LONG = "PR-104", "The name \"{name}\" is longer than {max} characters";
    NAME_BAD_CHARS = "PR-105", "The name \"{name}\" cannot start with a dot or contain / \\ : or control characters";
    AUTOSAVE_OUT_OF_RANGE = "PR-201", "Autosave interval {seconds} s is outside {min}..={max} (0 turns it off)";
    UI_SCALE_OUT_OF_RANGE = "PR-202", "Interface scale {scale} is outside {min}..={max}";
    TEMPLATE_NOT_FOUND = "PR-070", "Template \"{name}\" not found";
    PRESET_NOT_FOUND = "PR-071", "Preset \"{name}\" not found";
    PRESET_DAMAGED = "PR-170", "Preset \"{name}\" is damaged: {details}";
    PRESET_WRONG_KIND = "PR-171", "Preset \"{name}\" is for \"{preset_kind}\", not \"{kind}\"";
    SOURCE_NOT_A_FOLDER = "PR-172", "{path} is not a folder";
    TOO_MANY_SOURCES = "PR-270", "At most {max} sound folders can be linked";
    STATE_NOT_WRITTEN = "PR-470", "Could not write the app state file {path}: {details}";
    PRESETS_NOT_READ = "PR-471", "Could not read the presets folder {path}: {details}";
    PRESET_NOT_READ = "PR-472", "Could not read preset \"{name}\": {details}";
    PRESET_NOT_WRITTEN = "PR-473", "Could not write the preset file {path}: {details}";
    PRESET_NOT_DELETED = "PR-474", "Could not delete preset \"{name}\": {details}";
    TEMPLATES_NOT_READ = "PR-475", "Could not read the templates folder {path}: {details}";
    TEMPLATE_NOT_REPLACED = "PR-476", "Could not replace template \"{name}\": {details}";
    TEMPLATE_NOT_DELETED = "PR-477", "Could not delete template \"{name}\": {details}";
}
