//! Clips on the timeline: placement, edits, warp, takes and comping. Codes `CL-KNN`.

crate::codes! {
    NOT_FOUND = "CL-001", "Clip {id} not found";
    NOTE_CLIP_NEEDS_INSTRUMENT_TRACK = "CL-110", "Note clips go on instrument tracks";
    AUDIO_CLIP_NEEDS_AUDIO_TRACK = "CL-111", "Audio clips go on audio tracks";
    SPLIT_OUTSIDE_CLIP = "CL-112", "Split point {tick} is outside clip {id}";
    LOOP_REGION_BACKWARDS = "CL-113", "The loop start ({start}) must be before its end ({end})";
    NOTE_CLIPS_OVERLAP = "CL-114", "Note clips on track {track} overlap";
    AUDIO_CLIP_INSIDE_ANOTHER = "CL-115", "On track {track} an audio clip lies inside another: audio clips may overlap only at their edges (crossfade)";
    TOO_MANY_AUDIO_CLIPS_AT_ONCE = "CL-116", "On track {track} more than two audio clips overlap at one place";
    GAIN_NOT_A_NUMBER = "CL-117", "Clip gain {value} is not a finite number";
    NOTHING_TO_COPY = "CL-118", "No clips to copy";
    WARP_NEEDS_AUDIO_CLIP = "CL-119", "Only audio clips have tempo fitting and pitch (clip {id})";
    PITCH_NOT_A_NUMBER = "CL-120", "Clip pitch shift {value} is not a finite number";
    NOTE_CLIP_HAS_NO_TAKES = "CL-121", "A note clip has no takes";
    TAKE_ALREADY_ADDED = "CL-122", "Sample {take} is already a take of the clip";
    COMP_NOT_FROM_START = "CL-123", "The comp must start at the clip start (tick 0), not at tick {tick}";
    COMP_UNORDERED = "CL-124", "Comp segment starts must strictly increase ({before}, then {tick})";
    COMP_PAST_END = "CL-125", "A comp segment starts at tick {tick}, beyond the clip end ({length})";
    TAKE_RANGE_EMPTY = "CL-126", "The take range {from}..{to} is empty: the start must be before the end";
    NOT_A_TAKE = "CL-127", "Sample {take} is not a take of the clip";
    ONLY_SOUND = "CL-128", "Sample {take} is the only sound of the clip: delete the clip instead";
    TRIM_PAST_END = "CL-129", "The new start {tick} of clip {id} must be before its end ({end})";
    NEEDS_AUDIO_CLIP = "CL-130", "Clip {id} is not an audio clip";
    REVERSE_NEEDS_ONE_TAKE = "CL-131", "Clip {id} has several takes: keep one take before reversing it";
    NORMALIZE_LEVEL_NOT_A_NUMBER = "CL-132", "Normalize level {value} is not a finite number";
    STRETCH_CURVE_TOO_SHORT = "CL-150", "A stretch curve needs at least two points";
    STRETCH_CURVE_UNORDERED = "CL-151", "Stretch curve points must strictly increase";
    RANGE_OUT_OF_BOUNDS = "CL-210", "Clip start {start} or length {length} is out of range (start 0..={max}, length 1..={max} ticks)";
    COPY_TOO_FAR = "CL-211", "Clips copied to tick {tick} would go beyond tick {max}";
    GAIN_OUT_OF_RANGE = "CL-212", "Clip gain {gain} is outside 0..={max}";
    TOO_MANY = "CL-213", "The project already has the maximum of {max} clips";
    PITCH_OUT_OF_RANGE = "CL-214", "Clip pitch shift {semitones} is outside -{max}..={max} semitones";
    TOO_MANY_COMP_SEGMENTS = "CL-215", "A comp has at most {max} segments";
    NORMALIZE_LEVEL_OUT_OF_RANGE = "CL-216", "Normalize level {db} dB is outside {min}..=0 dB";
    STRETCH_OUT_OF_RANGE = "CL-250", "Stretch {stretch} is outside {min}..={max}";
    STRETCH_WORKER_DOWN = "CL-450", "The audio processing thread is not running";
    COPY_NOT_CREATED = "CL-910", "The clip copy was not created";
}
