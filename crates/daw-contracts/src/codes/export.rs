//! Export, bounce, rendering and mix analysis. Codes `EX-KNN`.

crate::codes! {
    FREEZE_NEEDS_INSTRUMENT = "EX-110", "Only a track with an instrument can be frozen (track {id})";
    NOT_FROZEN = "EX-111", "Track {id} is not frozen";
    RANGE_BACKWARDS = "EX-150", "The range start ({from}) must be before its end ({to})";
    TAIL_OUT_OF_RANGE = "EX-250", "Tail {seconds} s is outside 0..={max} s";
    BOUNCE_TOO_LONG = "EX-251", "The bounce would last {seconds} s (at most {max} s)";
    TARGET_LOUDNESS_OUT_OF_RANGE = "EX-252", "Target loudness {lufs} LUFS is outside {min}..={max}";
    PEAK_CEILING_OUT_OF_RANGE = "EX-253", "True peak ceiling {db} dBTP is outside {min}..={max}";
    SAMPLE_RATE_OUT_OF_RANGE = "EX-254", "Sample rate {rate} Hz is outside {min}..={max}";
    EXPORT_TOO_LONG = "EX-255", "The export would last {seconds} s (at most {max} s)";
    ANALYSIS_TOO_LONG = "EX-256", "The measured range lasts {seconds} s (at most {max} s)";
    RESOLUTION_OUT_OF_RANGE = "EX-257", "Analysis section length {bars} bars is outside 1..={max}";
    BOUNCE_NEEDS_SAVED_PROJECT = "EX-310", "Save the project first: the track sound goes into the project folder";
    TRACK_HAS_NO_CLIPS = "EX-350", "Track {id} has no clips: nothing to bounce";
    NOTHING_TO_MEASURE = "EX-351", "Nothing to measure: the project has no clips (or pass toTick)";
    NOTHING_TO_EXPORT = "EX-352", "Nothing to export: the arrangement is empty";
    NO_LOOP_REGION = "EX-353", "The loop region is not set";
    TOO_QUIET_TO_NORMALIZE = "EX-354", "Nothing to normalize: the sound is quieter than -70 LUFS or shorter than 0.4 s; export without normalization";
    WRITE_FAILED = "EX-450", "Could not write the export file: {details}";
    MP3_ENCODER_FAILED = "EX-451", "The MP3 encoder failed: {details}";
    FLAC_ENCODER_FAILED = "EX-452", "The FLAC encoder failed: {details}";
    PARTIAL_FILE_LEFT = "EX-453", "Export failed ({cause}); the unfinished file {path} could not be removed: {details}";
    TEMP_FILE_NOT_REMOVED = "EX-454", "The export was written, but the temporary file {path} could not be removed: {details}";
    FAILED_TEMP_FILE_LEFT = "EX-455", "Export failed ({cause}); the temporary file {path} could not be removed: {details}";
    MP3_BITRATE_UNSUPPORTED = "EX-550", "MP3 bitrate {kbps} kbps is not supported (use one of {allowed})";
    FLAC_FLOAT_UNSUPPORTED = "EX-551", "FLAC stores only integer samples: choose 16 or 24 bits";
}
