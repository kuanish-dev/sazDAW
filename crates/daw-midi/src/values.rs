//! MIDI values ↔ project controller values: 7-bit CC and pressure are `0..=1`, 14-bit pitch bend is `-1..=1`
//! (center 8192 is 0, the bottom edge 0 is −1, the top edge 16383 is exactly 1). The same math for live input and files.

const BEND_CENTER: f32 = 8192.0;

/// 7-bit value (0..=127) → `0..=1`.
pub(crate) fn from_7bit(value: u8) -> f32 {
    f32::from(value.min(127)) / 127.0
}

/// `0..=1` → 7-bit value.
pub(crate) fn to_7bit(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 127.0).round() as u8
}

/// 14-bit pitch bend (0..=16383) → `-1..=1`.
pub(crate) fn bend_from_14bit(raw: u16) -> f32 {
    let offset = f32::from(raw.min(16_383)) - BEND_CENTER;
    if offset >= 0.0 { offset / (BEND_CENTER - 1.0) } else { offset / BEND_CENTER }
}

/// `-1..=1` → 14-bit pitch bend.
pub(crate) fn bend_to_14bit(value: f32) -> u16 {
    let value = value.clamp(-1.0, 1.0);
    let scale = if value >= 0.0 { BEND_CENTER - 1.0 } else { BEND_CENTER };
    (BEND_CENTER + (value * scale).round()) as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bend_edges_and_center_are_exact_and_round_trip() {
        assert_eq!(bend_from_14bit(0), -1.0);
        assert_eq!(bend_from_14bit(8192), 0.0);
        assert_eq!(bend_from_14bit(16_383), 1.0);
        for raw in [0, 1, 4000, 8191, 8192, 8193, 12_000, 16_383] {
            assert_eq!(bend_to_14bit(bend_from_14bit(raw)), raw);
        }
    }

    #[test]
    fn seven_bit_values_round_trip() {
        for raw in 0..=127 {
            assert_eq!(to_7bit(from_7bit(raw)), raw);
        }
        assert_eq!(to_7bit(2.0), 127);
    }
}
