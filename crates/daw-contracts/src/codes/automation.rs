//! Automation lanes, their clips and points. Codes `AT-KNN`.

crate::codes! {
    LANE_NOT_FOUND = "AT-010", "Automation lane {id} not found";
    CLIP_NOT_FOUND = "AT-011", "Automation clip {id} not found in lane {lane}";
    POINT_NOT_A_NUMBER = "AT-110", "Automation point value {value} is not a finite number";
    LANE_EXISTS = "AT-111", "The parameter already has an automation lane";
    SHAPE_EMPTY_RANGE = "AT-112", "The shape range {from}…{to} is empty: the end must be after the start";
    SHAPE_PERIOD_TOO_SHORT = "AT-113", "The shape period must be at least {min} ticks";
    POINT_CURVE_NOT_A_NUMBER = "AT-114", "Automation point curve {curve} is not a finite number";
    CLIP_OVERLAPS = "AT-115", "Automation clips of one lane cannot overlap: {start}…{end} is taken";
    CLIP_EMPTY = "AT-116", "An automation clip needs a length and a loop of at least 1 tick";
    CLIP_PAST_END = "AT-117", "The automation clip {start}+{length} ends past the end of the timeline";
    LOOP_TOO_LONG = "AT-118", "An automation clip loop is at most {max} ticks";
    POINT_OUTSIDE_PATTERN = "AT-119", "Tick {tick} is after the end of the pattern ({loop} ticks)";
    TOO_MANY_LANES = "AT-210", "The project already has the maximum of {max} automation lanes";
    TOO_MANY_POINTS = "AT-211", "An automation clip holds at most {max} points";
    TOO_MANY_CLIPS = "AT-212", "An automation lane holds at most {max} clips";
}
