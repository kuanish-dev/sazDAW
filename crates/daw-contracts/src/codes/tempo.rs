//! Transport, tempo map, time signatures, metronome. Codes `TM-KNN`.

crate::codes! {
    INVALID_TEMPO = "TM-101", "Tempo {bpm} is outside {min}..={max} BPM";
    INVALID_SIGNATURE = "TM-102", "Time signature {numerator}/{denominator} is not supported";
    TEMPO_MAP_EMPTY = "TM-103", "The {list} list needs at least one point, at tick 0";
    TEMPO_MAP_FIRST_NOT_AT_ZERO = "TM-104", "The first point of {list} must be at tick 0, not {tick}";
    TEMPO_MAP_UNORDERED = "TM-105", "Ticks in {list} must strictly increase ({before}, then {tick})";
    SIGNATURE_OFF_BAR = "TM-106", "The time signature change at tick {tick} is not at a bar start ({numerator}/{denominator} bar = {bar} ticks from tick {from})";
    TEMPO_MAP_TOO_MANY = "TM-201", "Too many points in {list} (at most {max})";
    TICK_OUT_OF_RANGE = "TM-202", "Tick {tick} is beyond the song end ({max})";
}
