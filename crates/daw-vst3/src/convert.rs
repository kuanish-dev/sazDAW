//! The engine's interleaved stereo buffer <-> two flat VST3 channels.

/// Splits `L R L R …` into two channels. The channel length is the frame count.
pub fn deinterleave(interleaved: &[f32], left: &mut [f32], right: &mut [f32]) {
    for ((frame, l), r) in interleaved.chunks_exact(2).zip(left.iter_mut()).zip(right.iter_mut()) {
        *l = frame[0];
        *r = frame[1];
    }
}

/// Joins two channels back into `L R L R …`.
pub fn interleave(left: &[f32], right: &[f32], interleaved: &mut [f32]) {
    for ((frame, l), r) in interleaved.chunks_exact_mut(2).zip(left).zip(right) {
        frame[0] = *l;
        frame[1] = *r;
    }
}

/// **Adds** two channels to the engine buffer with `channels` channels per frame: left to the first, right to the second.
/// A mono buffer gets the half sum; extra channels stay untouched.
pub fn add_planar(left: &[f32], right: &[f32], target: &mut [f32], channels: usize) {
    if channels == 0 {
        return;
    }
    for ((frame, l), r) in target.chunks_exact_mut(channels).zip(left).zip(right) {
        if channels == 1 {
            frame[0] += 0.5 * (l + r);
        } else {
            frame[0] += l;
            frame[1] += r;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_planar_adds_instead_of_overwriting() {
        let mut stereo = [1.0, 1.0, 1.0, 1.0];
        add_planar(&[0.5, 0.25], &[-0.5, 0.75], &mut stereo, 2);
        assert_eq!(stereo, [1.5, 0.5, 1.25, 1.75]);
        let mut mono = [1.0, 1.0];
        add_planar(&[0.5, 0.0], &[0.5, 1.0], &mut mono, 1);
        assert_eq!(mono, [1.5, 1.5]);
        let mut surround = [0.0; 3];
        add_planar(&[1.0], &[2.0], &mut surround, 3);
        assert_eq!(surround, [1.0, 2.0, 0.0]);
    }

    #[test]
    fn roundtrip_keeps_samples() {
        let source = [0.1, -0.2, 0.3, -0.4, 0.5, -0.6];
        let (mut left, mut right) = ([0.0; 3], [0.0; 3]);
        deinterleave(&source, &mut left, &mut right);
        assert_eq!(left, [0.1, 0.3, 0.5]);
        assert_eq!(right, [-0.2, -0.4, -0.6]);
        let mut back = [0.0; 6];
        interleave(&left, &right, &mut back);
        assert_eq!(back, source);
    }

    #[test]
    fn shorter_channel_limits_work() {
        let source = [1.0, 2.0, 3.0, 4.0];
        let (mut left, mut right) = ([9.0; 1], [9.0; 1]);
        deinterleave(&source, &mut left, &mut right);
        assert_eq!((left, right), ([1.0], [2.0]));
    }
}
