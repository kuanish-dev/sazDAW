//! Calls from the interface and AI clients: methods and their params. Codes `RP-KNN`.

crate::codes! {
    UNKNOWN_METHOD = "RP-001", "Unknown method \"{method}\"";
    UNKNOWN_TOOL = "RP-002", "Unknown tool \"{tool}\"";
    BAD_PARAMS = "RP-101", "Invalid params for \"{method}\": {details}";
    NO_TOOL_NAME = "RP-102", "The request has no tool name (name)";
    BAD_JSON = "RP-103", "Could not parse JSON: {details}";
    UNSUPPORTED_PROTOCOL_METHOD = "RP-501", "Protocol method \"{method}\" is not supported";
    BAD_RESULT = "RP-901", "Could not encode the result of \"{method}\": {details}";
    TASK_FAILED = "RP-902", "The call \"{method}\" stopped unexpectedly: {details}";
}
