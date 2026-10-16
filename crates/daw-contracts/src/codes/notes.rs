//! Notes in note clips: notes, controller lanes, transforms, MIDI note effects. Codes `NT-KNN`.

crate::codes! {
    NOT_FOUND = "NT-001", "Note {id} not found";
    UNKNOWN_NOTE_EFFECT = "NT-002", "Unknown MIDI effect \"{kind}\"";
    NOTE_EFFECT_NOT_FOUND = "NT-011", "MIDI effect {id} not found";
    AUDIO_CLIP_HAS_NO_NOTES = "NT-110", "Clip {id} is an audio clip and has no notes";
    NO_NOTES_SELECTED = "NT-111", "No notes are selected (to take all notes of the clip, omit ids)";
    NOTE_EFFECT_PARAM_NOT_A_NUMBER = "NT-112", "MIDI effect parameter value {value} is not a finite number";
    NOTE_EFFECTS_NEED_INSTRUMENT = "NT-113", "\"{name}\" is an audio track: MIDI effects go before an instrument";
    CONTROLLER_VALUE_OUT_OF_RANGE = "NT-114", "Controller point value {value} is outside {min}..={max}";
    TRANSPOSED_PITCH_OUT_OF_RANGE = "NT-115", "Transposing by {semitones} semitones moves note {id} (pitch {pitch}) outside 0..=127";
    VARIATION_DENSITY_OUT_OF_RANGE = "NT-116", "Variation density {value} is outside -1..=1";
    VARIATION_PITCH_OUT_OF_RANGE = "NT-117", "Variation pitch share {value} is outside 0..=1";
    VARIATION_RHYTHM_OUT_OF_RANGE = "NT-118", "Variation rhythm share {value} is outside 0..=1";
    VARIATION_VELOCITY_OUT_OF_RANGE = "NT-119", "Variation velocity share {value} is outside 0..=1";
    GROOVE_SOURCE_EMPTY = "NT-120", "Clip {id} has no notes to take the groove from";
    UNKNOWN_NOTE_EFFECT_PARAM = "NT-121", "MIDI effect \"{kind}\" has no parameter \"{param}\"";
    PITCH_OUT_OF_RANGE = "NT-201", "Note pitch {pitch} is outside 0..=127";
    VELOCITY_OUT_OF_RANGE = "NT-202", "Velocity {velocity} is outside 1..=127";
    NOTE_END_OUT_OF_RANGE = "NT-203", "Note end {end} is outside the clip content (1..={max} ticks)";
    CC_NUMBER_OUT_OF_RANGE = "NT-204", "MIDI controller number {number} is outside 0..={max}";
    TOO_MANY_NOTES = "NT-210", "A clip holds at most {max} notes";
    LOOP_LENGTH_OUT_OF_RANGE = "NT-211", "Clip content length {ticks} must be a multiple of {step} within {step}..={max} ticks";
    TOO_MANY_CONTROLLER_POINTS = "NT-212", "A controller lane holds at most {max} points";
    CONTROLLER_TICK_OUT_OF_RANGE = "NT-213", "Controller point tick {tick} is outside the clip content (0..={max})";
    QUANTIZE_GRID_OUT_OF_RANGE = "NT-215", "Quantize grid {grid} is outside 1..={max} ticks";
    QUANTIZE_STRENGTH_OUT_OF_RANGE = "NT-216", "Quantize strength {strength} is outside 0..=1";
    HUMANIZE_TIMING_OUT_OF_RANGE = "NT-217", "Humanize timing spread {ticks} is outside 0..={max} ticks";
    HUMANIZE_VELOCITY_OUT_OF_RANGE = "NT-218", "Humanize velocity spread {velocity} is outside 0..={max}";
    TRANSPOSE_OUT_OF_RANGE = "NT-219", "Transpose by {semitones} semitones is outside -127..=127";
    SCALE_ROOT_OUT_OF_RANGE = "NT-220", "Scale root {root} is outside 0..=11";
    GROOVE_AMOUNT_OUT_OF_RANGE = "NT-222", "Groove amount {amount} is outside 0..=1";
    SWING_OUT_OF_RANGE = "NT-223", "Swing {percent} % is outside 50..=75";
    GROOVE_SHIFT_OUT_OF_RANGE = "NT-224", "Groove shift {ticks} is outside 0..={max} ticks";
    ACCENT_OUT_OF_RANGE = "NT-225", "Accent {percent} % is outside 100..=200";
    TOO_MANY_NOTE_EFFECTS = "NT-230", "A track holds at most {max} MIDI effects";
    NOTE_EFFECT_NOT_CREATED = "NT-410", "Track \"{track}\": MIDI effect \"{kind}\" was not created: {details}";
    NOTE_EFFECTS_NOT_SENT = "NT-411", "Track \"{track}\": MIDI effects were not passed to the engine: {details}";
    NOTE_EFFECTS_NOT_CLEARED = "NT-412", "MIDI effects of track {id} were not removed from the engine: {details}";
    NO_RESULT = "NT-910", "The note edit returned no note";
    GROOVE_NOT_EXTRACTED = "NT-911", "The groove of the source clip was not extracted";
}
