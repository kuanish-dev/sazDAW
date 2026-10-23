//! The binary form of the replies a client reads many times a second: the sample windows of the live pictures
//! (`audio.masterWindow`, `audio.nodeWindows`), through `RpcTable::call_binary`. Why: as JSON a sample costs about ten
//! characters and a number parse on each side, about 30 times a second for up to 16 nodes; as bytes it costs four and the
//! interface reads it in place as a `Float32Array`. The same methods still answer JSON through `call` (AI clients, tests).
//!
//! The format, mirrored by `app/src/api/wire.ts` (decoding) and `app/src/mock/wire.ts` (the mock's encoding): little-endian;
//! every field is 4 bytes (`end` is 8), so each run of samples starts at a multiple of 4 and is read without a copy.
//! - `MasterWindow`: `sample_rate: u32`, `frames: u32`, then `frames` × `f32`.
//! - `Vec<NodeWindow>`: `count: u32`, then per window `node: u32`, `watch: u32`, `end: u64`, `sample_rate: u32`,
//!   `frames: u32`, then `frames` × `f32` of the input and `frames` × `f32` of the output.

use daw_contracts::{MasterWindow, NodeWindow};

/// A reply that also has the binary form (`RpcTable::add_binary`).
pub(crate) trait BinaryReply {
    fn encode(&self) -> Vec<u8>;
}

fn put_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put_samples(out: &mut Vec<u8>, samples: &[f32]) {
    for sample in samples {
        out.extend_from_slice(&sample.to_le_bytes());
    }
}

/// A count as the format's `u32`: windows and samples are bounded far below it (`MAX_WATCHED_NODES`,
/// `NODE_WINDOW_FRAMES`).
fn count(len: usize) -> u32 {
    u32::try_from(len).unwrap_or(u32::MAX)
}

impl BinaryReply for MasterWindow {
    fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(8 + 4 * self.samples.len());
        put_u32(&mut out, self.sample_rate);
        put_u32(&mut out, count(self.samples.len()));
        put_samples(&mut out, &self.samples);
        out
    }
}

impl BinaryReply for Vec<NodeWindow> {
    fn encode(&self) -> Vec<u8> {
        let size = 4 + self.iter().map(|w| 24 + 4 * (w.input.len() + w.output.len())).sum::<usize>();
        let mut out = Vec::with_capacity(size);
        put_u32(&mut out, count(self.len()));
        for window in self {
            put_u32(&mut out, window.node);
            put_u32(&mut out, window.watch);
            out.extend_from_slice(&window.end.to_le_bytes());
            put_u32(&mut out, window.sample_rate);
            put_u32(&mut out, count(window.input.len()));
            put_samples(&mut out, &window.input);
            put_samples(&mut out, &window.output);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u32_at(bytes: &[u8], at: usize) -> u32 {
        u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
    }

    fn f32_at(bytes: &[u8], at: usize) -> f32 {
        f32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
    }

    #[test]
    fn a_master_window_is_its_rate_its_count_and_its_samples() {
        let bytes = MasterWindow { samples: vec![0.5, -0.25], sample_rate: 48_000 }.encode();
        assert_eq!(bytes.len(), 16);
        assert_eq!((u32_at(&bytes, 0), u32_at(&bytes, 4)), (48_000, 2));
        assert_eq!((f32_at(&bytes, 8), f32_at(&bytes, 12)), (0.5, -0.25));
    }

    #[test]
    fn node_windows_follow_one_another_with_the_input_before_the_output() {
        let window = |node, input: Vec<f32>, output| NodeWindow { node, watch: 3, end: (1 << 40) + 7, input, output, sample_rate: 44_100 };
        let bytes = vec![window(5, vec![1.0], vec![2.0]), window(9, vec![], vec![])].encode();
        assert_eq!(bytes.len(), 4 + (24 + 8) + 24);
        assert_eq!(u32_at(&bytes, 0), 2);
        assert_eq!((u32_at(&bytes, 4), u32_at(&bytes, 8)), (5, 3));
        assert_eq!(u64::from_le_bytes(bytes[12..20].try_into().unwrap()), (1 << 40) + 7);
        assert_eq!((u32_at(&bytes, 20), u32_at(&bytes, 24)), (44_100, 1));
        assert_eq!((f32_at(&bytes, 28), f32_at(&bytes, 32)), (1.0, 2.0));
        assert_eq!((u32_at(&bytes, 36), u32_at(&bytes, 56)), (9, 0), "the second window starts after the first one's samples");
    }
}
