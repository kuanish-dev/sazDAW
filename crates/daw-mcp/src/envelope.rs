//! JSON-RPC 2.0 envelopes: a reply with a result and a reply with an error. Error codes are from the JSON-RPC spec.

use serde_json::{Value, json};

/// The input is not JSON.
pub(crate) const PARSE_ERROR: i64 = -32700;
/// The protocol method is not supported.
pub(crate) const METHOD_NOT_FOUND: i64 = -32601;
/// Invalid params (e.g. an unknown tool).
pub(crate) const INVALID_PARAMS: i64 = -32602;
/// Server error: the app the bridge leads to did not answer.
pub(crate) const SERVER_ERROR: i64 = -32000;

pub(crate) fn success(id: &Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

pub(crate) fn failure(id: &Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}
