//! The command layer for the interface: one entry point `call(method, params)` instead of dozens of separate
//! functions. The method table is data; each method is a thin adapter over a contract with its "what it does"
//! description next to it (`RpcTable::add`). A method that saves or renders the project is added with `add_flushing`:
//! before it runs, the app's open plugin windows hand their state to the project (`PluginStateFlush`, given by a root
//! that has windows), whichever door the call came through. The crate knows nothing about Tauri or the transport: it takes and
//! returns JSON, and a method added with `add_binary` also returns its reply as bytes (`call_binary`, `binary.rs`) for a
//! client that reads it many times a second. Shared notions for descriptions: time is in ticks (960 per quarter note; a 4/4 bar = 3840,
//! a 16th = 240); ids of tracks, clips, notes, graph nodes and samples are numbers from the create methods and `project.get`.

#![allow(clippy::unit_arg, reason = "contract commands return `()`, and `Ok(command())` keeps the adapters short on purpose")]

mod analysis;
mod audio;
mod audio_record;
mod automation;
mod binary;
mod bounce;
mod catalog;
mod chords;
mod clips;
mod feed;
mod folders;
mod graph;
mod key;
mod live;
mod log;
mod markers;
mod midi;
mod note_effects;
mod notes;
mod patterns;
mod plugins;
mod preferences;
mod presets;
mod preview;
mod project;
mod record;
mod samples;
mod sounds;
mod stage;
mod telemetry;
mod templates;
mod tempo;
mod tracks;
mod transport;

use std::collections::HashMap;
use std::sync::Arc;

use daw_contracts::codes::rpc as codes;
use daw_contracts::{AppLog, ContractError, Controls, ErrorKind, LogEntry, LogLevel, PluginStateFlush};

use binary::BinaryReply;

/// How many characters of the call params go into the log on failure.
const MAX_LOGGED_PARAMS: usize = 600;
use schemars::JsonSchema;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

pub use feed::{ProjectFeed, ProjectUpdate, WholeRequest};
pub use telemetry::tick;

type Handler = Box<dyn Fn(&Controls, Value) -> Result<Value, ContractError> + Send + Sync>;
type BinaryHandler = Box<dyn Fn(&Controls, Value) -> Result<Vec<u8>, ContractError> + Send + Sync>;

/// A method as clients see it: name, meaning and the JSON schema of its params.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MethodInfo {
    pub name: String,
    pub description: String,
    /// JSON Schema of the params (`{"type":"null"}` for a method without params).
    pub params: Value,
}

struct Method {
    handler: Handler,
    description: &'static str,
    schema: Value,
    /// The method saves or renders the project: open plugin windows hand their state to it first.
    flushes: bool,
    /// The reply as bytes (`add_binary`), for `call_binary`.
    binary: Option<BinaryHandler>,
}

pub struct RpcTable {
    methods: HashMap<&'static str, Method>,
    /// The app's plugin windows; `None` in a root without windows (the console, the headless MCP server).
    windows: Option<Arc<dyn PluginStateFlush>>,
}

impl RpcTable {
    pub fn new() -> Self {
        let mut table = Self { methods: HashMap::new(), windows: None };
        for register in [
            transport::register,
            tempo::register,
            key::register,
            project::register,
            record::register,
            catalog::register,
            tracks::register,
            clips::register,
            markers::register,
            folders::register,
            notes::register,
            note_effects::register,
            graph::register,
            stage::register,
            automation::register,
            samples::register,
            preview::register,
            sounds::register,
            midi::register,
            live::register,
            log::register,
            audio::register,
            audio_record::register,
            plugins::register,
            preferences::register,
            patterns::register,
            chords::register,
            bounce::register,
            templates::register,
            presets::register,
            analysis::register,
        ] {
            register(&mut table);
        }
        let catalog = serde_json::to_value(table.describe()).unwrap_or(Value::Null);
        table.add("rpc.describe", "Lists all methods with their descriptions and parameter schemas.", move |_, (): ()| Ok(catalog.clone()));
        table
    }

    /// The root has plugin windows: methods added with `add_flushing` let them hand their state to the project first.
    pub fn set_plugin_windows(&mut self, windows: Arc<dyn PluginStateFlush>) {
        self.windows = Some(windows);
    }

    /// All methods with descriptions and param schemas, sorted by name (except `rpc.describe` itself).
    pub fn describe(&self) -> Vec<MethodInfo> {
        let mut methods: Vec<MethodInfo> = self
            .methods
            .iter()
            .map(|(name, method)| MethodInfo {
                name: (*name).to_string(),
                description: method.description.to_string(),
                params: method.schema.clone(),
            })
            .collect();
        methods.sort_by(|a, b| a.name.cmp(&b.name));
        methods
    }

    /// Registers a method: `description` is one "what it does" sentence for `rpc.describe` and AI clients; params are
    /// parsed into `P`, the result is serialized.
    pub(crate) fn add<P, R>(
        &mut self,
        name: &'static str,
        description: &'static str,
        handler: impl Fn(&Controls, P) -> Result<R, ContractError> + Send + Sync + 'static,
    ) where
        P: DeserializeOwned + JsonSchema,
        R: Serialize,
    {
        let generator =
            schemars::generate::SchemaSettings::draft2020_12().with(|settings| settings.inline_subschemas = true).into_generator();
        let mut schema = serde_json::to_value(generator.into_root_schema_for::<P>()).unwrap_or(Value::Null);
        if let Some(object) = schema.as_object_mut() {
            object.remove("$schema");
        }
        let wrapped: Handler = Box::new(move |controls, params| {
            let result = run(name, &handler, controls, params)?;
            serde_json::to_value(result).map_err(|e| codes::BAD_RESULT.with("method", name).with("details", e))
        });
        let previous = self.methods.insert(name, Method { handler: wrapped, description, schema, flushes: false, binary: None });
        debug_assert!(previous.is_none(), "method \"{name}\" is registered twice");
    }

    /// Like `add`, for a method whose reply a client reads many times a second (a live picture's sound): it also answers
    /// as bytes through `call_binary` (the format is in `binary.rs`), and JSON as usual through `call`.
    pub(crate) fn add_binary<P, R>(
        &mut self,
        name: &'static str,
        description: &'static str,
        handler: impl Fn(&Controls, P) -> Result<R, ContractError> + Send + Sync + 'static,
    ) where
        P: DeserializeOwned + JsonSchema,
        R: Serialize + BinaryReply,
    {
        let handler = Arc::new(handler);
        let json = Arc::clone(&handler);
        self.add(name, description, move |controls, params: P| json(controls, params));
        let binary: BinaryHandler = Box::new(move |controls, params| Ok(run(name, &*handler, controls, params)?.encode()));
        if let Some(method) = self.methods.get_mut(name) {
            method.binary = Some(binary);
        }
    }

    /// Like `add`, for a method that saves or renders the project (the file, a preset, a template, export, bounce, level
    /// analysis): the state of open plugin windows must be in the project before it runs.
    pub(crate) fn add_flushing<P, R>(
        &mut self,
        name: &'static str,
        description: &'static str,
        handler: impl Fn(&Controls, P) -> Result<R, ContractError> + Send + Sync + 'static,
    ) where
        P: DeserializeOwned + JsonSchema,
        R: Serialize,
    {
        self.add(name, description, handler);
        if let Some(method) = self.methods.get_mut(name) {
            method.flushes = true;
        }
    }

    pub fn call(&self, controls: &Controls, method: &str, params: Value) -> Result<Value, ContractError> {
        let entry = self.entry(controls, method, &params)?;
        (entry.handler)(controls, params)
    }

    /// Calls a method added with `add_binary` and returns its reply as bytes (the format is in `binary.rs`); any other
    /// method is refused with `RP-502`.
    pub fn call_binary(&self, controls: &Controls, method: &str, params: Value) -> Result<Vec<u8>, ContractError> {
        let entry = self.entry(controls, method, &params)?;
        let Some(binary) = &entry.binary else {
            let error = codes::NO_BINARY_REPLY.with("method", method);
            log_failure(controls.log.as_ref(), LogLevel::Error, method, &error, &params.to_string());
            return Err(error);
        };
        binary(controls, params)
    }

    /// The method to call, after the plugin windows handed their state to the project if it needs that.
    fn entry(&self, controls: &Controls, method: &str, params: &Value) -> Result<&Method, ContractError> {
        let Some(entry) = self.methods.get(method) else {
            let error = codes::UNKNOWN_METHOD.with("method", method);
            log_failure(controls.log.as_ref(), LogLevel::Error, method, &error, &params.to_string());
            return Err(error);
        };
        if entry.flushes
            && let Some(windows) = &self.windows
        {
            windows.flush();
        }
        Ok(entry)
    }

    pub fn methods(&self) -> Vec<&'static str> {
        let mut names: Vec<_> = self.methods.keys().copied().collect();
        names.sort_unstable();
        names
    }
}

/// Parses the params into `P` and runs the handler, logging a failure: what `call` and `call_binary` share.
fn run<P, R>(
    name: &'static str,
    handler: &impl Fn(&Controls, P) -> Result<R, ContractError>,
    controls: &Controls,
    params: Value,
) -> Result<R, ContractError>
where
    P: DeserializeOwned,
{
    let shown = params.to_string();
    let params: P = serde_json::from_value(params).map_err(|e| {
        let error = codes::BAD_PARAMS.with("method", name).with("details", e);
        log_failure(controls.log.as_ref(), LogLevel::Error, name, &error, &shown);
        error
    })?;
    handler(controls, params).inspect_err(|error| {
        let level = if matches!(error.kind(), ErrorKind::System | ErrorKind::Internal) { LogLevel::Error } else { LogLevel::Warn };
        log_failure(controls.log.as_ref(), level, name, error, &shown);
    })
}

/// Logs a failed call: method, code, English message and the (trimmed) params.
fn log_failure(log: &dyn AppLog, level: LogLevel, method: &str, error: &ContractError, params: &str) {
    let shown: String = params.chars().take(MAX_LOGGED_PARAMS).collect();
    let text = format!("{method}: {} {error}", error.code().code);
    log.record(LogEntry::new(level, "rpc", text).with_details(format!("params: {shown}")));
}

impl Default for RpcTable {
    fn default() -> Self {
        Self::new()
    }
}

/// Common param shapes.
pub(crate) mod params {
    use schemars::JsonSchema;
    use serde::Deserialize;

    #[derive(Deserialize, JsonSchema)]
    pub struct Id {
        pub id: u32,
    }

    #[derive(Deserialize, JsonSchema)]
    pub struct IdName {
        pub id: u32,
        pub name: String,
    }

    #[derive(Deserialize, JsonSchema)]
    pub struct Path {
        pub path: std::path::PathBuf,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptions_are_real_sentences_and_schemas_are_json_objects() {
        for method in RpcTable::new().describe() {
            assert!(
                method.description.chars().count() > 12 && method.description.ends_with('.'),
                "{}: «{}»",
                method.name,
                method.description
            );
            assert!(method.params.is_object(), "{}: param schema is not an object: {}", method.name, method.params);
        }
    }
}
