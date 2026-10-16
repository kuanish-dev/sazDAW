//! Samples and sounds: import, sample editor, sound library, preview. Codes `SM-KNN`.

crate::codes! {
    NOT_FOUND = "SM-001", "Sample {id} not found";
    REGION_EMPTY = "SM-110", "Region {from}..{to} is empty or starts beyond the sample end ({frames} frames)";
    NO_REGIONS = "SM-111", "Choose at least one region";
    CUT_NEEDS_SAVED_PROJECT = "SM-112", "Save the project first: the cut sounds go into the project folder";
    FOLDER_NOT_ALLOWED = "SM-150", "Folder {path} is outside the project and linked folders";
    NO_AUDIO_TRACK = "SM-151", "File {path} has no audio track";
    FILE_HAS_NO_SOUND = "SM-152", "File {path} contains no sound";
    PREVIEW_FILE_TOO_SHORT = "SM-154", "File {path} is too short to preview";
    TOO_MANY = "SM-210", "The project already has the maximum of {max} samples";
    WAVEFORM_BUCKETS_OUT_OF_RANGE = "SM-211", "Waveform bucket count {buckets} is outside 1..={max}";
    TOO_MANY_MARKERS = "SM-212", "A sample holds at most {max} slice markers";
    TOO_MANY_REGIONS = "SM-213", "At most {max} regions can be cut at once";
    REVERSE_NEEDS_SAVED_PROJECT = "SM-310", "Save the project first: the reversed sound goes into the project folder";
    FOLDER_OPEN_FAILED = "SM-450", "Could not open folder {path}: {details}";
    FOLDER_READ_FAILED = "SM-451", "Could not read folder {path}: {details}";
    FILE_READ_FAILED = "SM-452", "Could not read audio file {path}: {details}";
    WAV_WRITE_FAILED = "SM-453", "Could not write WAV file {path}: {details}";
    CODEC_UNSUPPORTED = "SM-550", "The audio codec of {path} is not supported";
}
