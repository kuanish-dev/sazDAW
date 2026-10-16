//! Markers and track folders. Codes `MK-KNN`.

crate::codes! {
    MARKER_NOT_FOUND = "MK-010", "Marker {id} not found";
    FOLDER_NOT_FOUND = "MK-011", "Folder {id} not found";
    MARKER_NAME_EMPTY = "MK-110", "A marker needs a name";
    MARKER_NAME_TOO_LONG = "MK-111", "The marker name is longer than {max} characters";
    FOLDER_NAME_EMPTY = "MK-112", "A folder needs a name";
    FOLDER_WITHOUT_TRACKS = "MK-113", "A folder needs at least one track";
    FOLDER_TRACK_TWICE = "MK-114", "Track {id} is listed twice";
    MARKER_PLACE_TAKEN = "MK-115", "Marker \"{name}\" already stands at tick {tick}";
    MARKER_TICK_OUT_OF_RANGE = "MK-210", "Marker tick {tick} is beyond {max}";
    TOO_MANY_MARKERS = "MK-211", "The project already has the maximum of {max} markers";
}
