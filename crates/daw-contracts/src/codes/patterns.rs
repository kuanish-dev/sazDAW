//! Patterns and the parts built from them: drum parts, chord parts (strums) and melodic step lines. Codes `PT-KNN`.

crate::codes! {
    BAD_GRID_SYMBOL = "PT-101", "\"{lane}\": unexpected symbol \"{symbol}\" in the grid (use X x o . or -)";
    GRID_LENGTH_MISMATCH = "PT-102", "\"{lane}\": the grid has {steps} steps, the pattern has {expected}";
    BAD_CHORD = "PT-103", "\"{chord}\" is not a chord: a root C…B with an optional # or b, then a type: m, 7, maj7, m7, sus2, sus4, dim or aug (\"Am\", \"F#m7\", \"Bb\")";
    BAD_STEP = "PT-104", "Step {step} \"{token}\" is not a step: write a note name (F2, Eb3, C#4) or a scale degree (1, 5, 3+, 7-), then ! for an accent and ~ to slide; - is a rest, _ holds the note before it";
    HOLD_WITHOUT_NOTE = "PT-105", "Step {step}: _ holds a note, but no note sounds right before it";
    NO_STEPS = "PT-106", "The step line has no steps";
    NOT_FOUND = "PT-070", "Pattern \"{id}\" not found: see the pattern list";
    NOT_FOUND_SIMILAR = "PT-071", "Pattern \"{id}\" not found; similar: {similar}";
    LANE_NOT_IN_PART = "PT-072", "Drum \"{lane}\" is not used in the part (it has: {lanes}); check the name";
    LANE_INSTRUMENT_NOT_FOUND = "PT-073", "There is no instrument \"{lane}\" for the pattern drum";
    STRUM_NOT_FOUND = "PT-074", "Strum pattern \"{id}\" not found; there are: {ids}";
    NO_SLOTS = "PT-170", "The drum part has no patterns (slots is empty)";
    LANE_ASSIGNED_TWICE = "PT-171", "The sound for drum \"{lane}\" is given twice";
    ALL_LANES_SKIPPED = "PT-172", "All drums of the part are off (skip): there is nothing to play";
    LANE_ON_AUDIO_TRACK = "PT-173", "\"{name}\" is an audio track: the notes of drum \"{lane}\" cannot go on it";
    NO_CHORDS = "PT-174", "The chord part has no bars";
    BAR_CHORD_COUNT = "PT-175", "Bar {bar} has {count} chords: put one or two in a bar";
    CHORDS_ON_AUDIO_TRACK = "PT-176", "\"{name}\" is an audio track: a chord part needs an instrument track";
    TOO_MANY_SLOTS = "PT-270", "A drum part has at most {max} steps, this one has {count}";
    REPEAT_OUT_OF_RANGE = "PT-271", "\"{pattern}\": repeats must be within 1..={max}, not {repeat}";
    BAR_TOO_FAR = "PT-272", "Bar {bar} is too far";
    PART_TOO_LONG = "PT-273", "The drum part goes too far in time";
    PREVIEW_TOO_LONG = "PT-274", "A preview plays at most {max} bars, this part has {bars}";
    TOO_MANY_CHORD_BARS = "PT-275", "A chord progression has at most {max} bars, this one has {count}";
    GATE_OUT_OF_RANGE = "PT-276", "Gate {gate} is outside 0..=1 (the share of a step a note sounds, above 0)";
    STEP_LENGTH_OUT_OF_RANGE = "PT-277", "Step length {ticks} is outside 1..={max} ticks";
    PLACE_TAKEN = "PT-370", "Track \"{name}\" already has clips here (bars {from}–{to}, counted from zero); set replace: true to replace them";
    DEGREE_WITHOUT_KEY = "PT-371", "Step {step} \"{token}\" is a scale degree, but the project has no key: set the key or write note names";
}
