//! Shape of a recorded note: a press–release pair becomes a note inside the recording loop (snapped to the grid or not),
//! and the number of the pass it was played on.

use daw_contracts::NewNote;

/// The shortest recorded note without grid snapping.
pub(crate) const MIN_LENGTH_TICKS: u32 = 30;

/// Turns a press–release pair into a note inside a loop of `loop_ticks`.
/// `start` and `end` are absolute transport ticks; a note crossing the loop end is cut there.
/// `quantize == 0` means no snapping.
pub(crate) fn finish_note(pitch: u8, velocity: f32, start: f64, end: f64, quantize: u32, loop_ticks: u32) -> Option<NewNote> {
    if !start.is_finite() || !end.is_finite() || start < 0.0 || loop_ticks == 0 {
        return None;
    }
    let loop_len = f64::from(loop_ticks);
    let position = start.rem_euclid(loop_len);
    let raw_length = (end - start).max(0.0);
    let (start_tick, length) = if quantize > 0 {
        let unit = f64::from(quantize);
        let snapped = (position / unit).round() * unit;
        // Rounding up to the loop end means the start of the next pass.
        let snapped = if snapped >= loop_len { 0.0 } else { snapped };
        (snapped as u32, ((raw_length / unit).round() * unit).max(unit) as u32)
    } else {
        let tick = position.round() as u32 % loop_ticks;
        (tick, (raw_length.round() as u32).max(MIN_LENGTH_TICKS))
    };
    Some(NewNote {
        pitch,
        start_tick,
        length_ticks: length.min(loop_ticks - start_tick).max(1),
        velocity: ((velocity * 127.0).round() as u8).clamp(1, 127),
    })
}

/// The pass number a tick falls into.
pub(crate) fn pass_of(tick: f64, loop_ticks: u32) -> u64 {
    (tick.max(0.0) / f64::from(loop_ticks.max(1))).floor() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOOP: u32 = 3840;

    #[test]
    fn snaps_start_and_length_to_the_grid() {
        let note = finish_note(60, 1.0, 250.0, 700.0, 240, LOOP).unwrap();
        assert_eq!((note.start_tick, note.length_ticks, note.velocity), (240, 480, 127));
        let short = finish_note(60, 0.5, 10.0, 20.0, 240, LOOP).unwrap();
        assert_eq!((short.start_tick, short.length_ticks), (0, 240), "no shorter than one grid step");
    }

    #[test]
    fn without_quantize_keeps_raw_timing_with_a_minimum() {
        let note = finish_note(64, 0.8, 101.4, 400.2, 0, LOOP).unwrap();
        assert_eq!((note.start_tick, note.length_ticks), (101, 299));
        assert_eq!(note.velocity, 102);
        assert_eq!(finish_note(64, 0.8, 100.0, 105.0, 0, LOOP).unwrap().length_ticks, MIN_LENGTH_TICKS);
    }

    #[test]
    fn later_loop_passes_land_on_the_same_positions() {
        let first = finish_note(60, 1.0, 100.0, 340.0, 0, LOOP).unwrap();
        let second = finish_note(60, 1.0, f64::from(LOOP) + 100.0, f64::from(LOOP) + 340.0, 0, LOOP).unwrap();
        assert_eq!(first, second);
        let fifth = finish_note(60, 1.0, f64::from(LOOP) * 4.0 + 960.0, f64::from(LOOP) * 4.0 + 1200.0, 240, LOOP).unwrap();
        assert_eq!((fifth.start_tick, fifth.length_ticks), (960, 240));
    }

    #[test]
    fn notes_crossing_the_loop_end_are_clipped_to_it() {
        let note = finish_note(60, 1.0, 3_800.0, 4_300.0, 0, LOOP).unwrap();
        assert_eq!((note.start_tick, note.length_ticks), (3_800, 40));
        // A note held longer than a whole pass does not cross the boundary either.
        let long = finish_note(60, 1.0, 100.0, 100.0 + 10.0 * f64::from(LOOP), 0, LOOP).unwrap();
        assert_eq!(long.start_tick + long.length_ticks, LOOP);
    }

    #[test]
    fn rounding_up_to_the_loop_end_wraps_to_the_start() {
        let note = finish_note(60, 1.0, f64::from(LOOP) - 20.0, f64::from(LOOP) + 100.0, 240, LOOP).unwrap();
        assert_eq!(note.start_tick, 0);
        assert!(note.start_tick + note.length_ticks <= LOOP);
    }

    #[test]
    fn rejects_garbage() {
        assert!(finish_note(60, 1.0, -5.0, 10.0, 0, LOOP).is_none());
        assert!(finish_note(60, 1.0, f64::NAN, 10.0, 0, LOOP).is_none());
        assert!(finish_note(60, 1.0, 0.0, 10.0, 0, 0).is_none());
        assert_eq!(finish_note(60, 0.0001, 0.0, 100.0, 0, LOOP).unwrap().velocity, 1);
    }

    #[test]
    fn pass_numbers_follow_the_loop_length() {
        assert_eq!(pass_of(0.0, LOOP), 0);
        assert_eq!(pass_of(f64::from(LOOP) - 1.0, LOOP), 0);
        assert_eq!(pass_of(f64::from(LOOP), LOOP), 1);
        assert_eq!(pass_of(f64::from(LOOP) * 3.5, LOOP), 3);
    }
}
