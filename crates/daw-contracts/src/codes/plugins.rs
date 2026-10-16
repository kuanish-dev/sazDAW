//! Plugins: VST3 scan, loading, state, editor windows. Codes `PL-KNN`.

crate::codes! {
    EDITOR_NOT_FOUND = "PL-001", "Plugin window {id} not found";
    WINDOW_FAILED = "PL-401", "Could not open the plugin window: {details}";
    NOT_FOUND = "PL-060", "Plugin \"{kind}\" is not among the scanned plugins; scan plugins again";
    NO_PARENT_VIEW = "PL-160", "The window has no parent view for the plugin interface";
    WINDOW_TOO_LARGE = "PL-260", "Plugin window size {width}x{height} is too large";
    DISABLED = "PL-360", "Plugin \"{name}\" is disabled (after a crash or by hand); turn it on in Settings > Plugins";
    EDITOR_ALREADY_OPEN = "PL-361", "The window of this plugin is already open";
    ALREADY_ATTACHED = "PL-362", "The plugin interface is already attached to a window";
    LOAD_FAILED = "PL-460", "Could not load the plugin: {details}";
    SCAN_HELPER_MISSING = "PL-461", "The plugin scan helper {path} is missing: build it (cargo build -p daw-vst3 --bin daw-plugin-scan)";
    ATTACH_FAILED = "PL-462", "The plugin did not attach its interface to the window (code {code})";
    BUILTIN_EFFECT_HAS_NO_WINDOW = "PL-560", "The built-in effect \"{kind}\" has no window of its own";
    BUILTIN_INSTRUMENT_HAS_NO_WINDOW = "PL-561", "The built-in instrument \"{kind}\" has no window of its own";
    NO_EDITOR = "PL-562", "Plugin \"{name}\" has no window of its own; its parameters are available as knobs";
    DISABLED_LIST_UNAVAILABLE = "PL-563", "This session does not keep a list of disabled plugins";
}
