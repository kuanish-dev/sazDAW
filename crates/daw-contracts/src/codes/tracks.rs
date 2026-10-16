//! Tracks: add, remove, order, instruments, monitoring. Codes `TR-KNN`.

crate::codes! {
    NOT_FOUND = "TR-001", "Track {id} not found";
    UNKNOWN_INSTRUMENT = "TR-002", "Unknown instrument \"{kind}\"";
    UNKNOWN_COLOR = "TR-101", "Unknown color \"{color}\": use one of {colors}";
    AUDIO_TRACK_TAKES_NO_PARAMS = "TR-110", "An audio track has no instrument, so it takes no instrument parameters";
    PARAM_NOT_A_NUMBER = "TR-111", "Instrument parameter value {value} is not a finite number";
    MONITOR_NEEDS_AUDIO_TRACK = "TR-112", "\"{name}\" is an instrument track: input monitoring exists only on audio tracks";
    UNKNOWN_INSTRUMENT_PARAM = "TR-113", "Instrument \"{kind}\" has no parameter \"{param}\"";
    FROZEN = "TR-114", "Track {id} is frozen: unfreeze it first";
    NO_INSTRUMENT = "TR-160", "Track {id} is an audio track and has no instrument";
    DEFAULT_PITCH_OUT_OF_RANGE = "TR-210", "Default note pitch {pitch} is outside 0..=127";
    TOO_MANY = "TR-211", "The project already has the maximum of {max} tracks";
    HEIGHT_OUT_OF_RANGE = "TR-212", "Track height {height} px is outside {min}..={max}";
    INSTRUMENT_NOT_CREATED = "TR-410", "Track \"{track}\": instrument \"{kind}\" was not created: {details}";
    INSTRUMENT_NOT_REMOVED = "TR-411", "The instrument of track {id} was not removed from the engine: {details}";
    NOT_CREATED = "TR-910", "The track was not created";
    NO_SOURCE_NODE = "TR-911", "Track {id} has no source node in the graph";
}
