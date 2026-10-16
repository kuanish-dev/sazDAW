//! Convolving a stereo stream with the linear-phase kernel: overlap-save in blocks of `len` frames on a `2·len`-point FFT.
//! The two channels ride one complex FFT (left + i·right): the kernel is real, so the real part of the result is the
//! filtered left and the imaginary part the filtered right. Frames come in one by one; each completed block of `len`
//! frames is transformed, multiplied by the kernel's spectrum and transformed back, and its last `len` outputs (the part
//! a circular convolution gets right) are played during the next block. So the output lags the input by `len` frames
//! of blocking plus the kernel's own `len / 2`: `latency`. A new kernel offered on the audio thread (`offer`) takes over
//! at the next block, crossfaded across it from the old one, so a settings change never clicks. Everything is allocated
//! in `new`; `push`, `offer` and `clear` only use what is there.

use super::design::quarter_turns;
use crate::fft::{Complex, Fft};

pub struct Convolver {
    len: usize,
    fft: Fft,
    /// The kernel's spectrum (`R(k)` with the delay put back) and the one waiting to take over.
    current: Vec<Complex>,
    next: Vec<Complex>,
    pending: bool,
    /// The last two blocks of input: the previous one, then the one being filled.
    input: Vec<Complex>,
    /// Working buffers of a block: with the current kernel and, while switching, with the next one.
    work: Vec<Complex>,
    switching: Vec<Complex>,
    /// The output of the last block, played frame by frame while the next block fills.
    ready: Vec<Complex>,
    position: usize,
}

impl Convolver {
    /// A convolver for kernels of `len` frames, passing nothing until a kernel is installed.
    pub fn new(len: usize) -> Self {
        Self {
            len,
            fft: Fft::new(2 * len),
            current: vec![Complex::ZERO; 2 * len],
            next: vec![Complex::ZERO; 2 * len],
            pending: false,
            input: vec![Complex::ZERO; 2 * len],
            work: vec![Complex::ZERO; 2 * len],
            switching: vec![Complex::ZERO; 2 * len],
            ready: vec![Complex::ZERO; len],
            position: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    /// How far the output lags the input, frames: a block of buffering and half the kernel.
    pub fn latency(&self) -> usize {
        self.len + self.len / 2
    }

    /// Spreads a design (`R(k)`, k = 0..=len) over the `2·len` grid with the delay of half the kernel put back.
    fn spread(len: usize, response: &[f32], into: &mut [Complex]) {
        for (k, slot) in into.iter_mut().enumerate() {
            let mirrored = if k <= len { k } else { 2 * len - k };
            *slot = quarter_turns(k).conj().scale(response[mirrored]);
        }
    }

    /// Uses the kernel `response` from now on, at once: for a convolver that has played nothing since (after `new`, or
    /// before `clear`).
    pub fn install(&mut self, response: &[f32]) {
        Self::spread(self.len, response, &mut self.current);
        self.pending = false;
    }

    /// Takes over the kernel `response` at the next block, crossfading from the current one across that block.
    pub fn offer(&mut self, response: &[f32]) {
        Self::spread(self.len, response, &mut self.next);
        self.pending = true;
    }

    /// Whether an offered kernel is still waiting for its block.
    pub fn is_switching(&self) -> bool {
        self.pending
    }

    /// Forgets the sound: silence in, silence out from now on. A waiting kernel takes over at once.
    pub fn clear(&mut self) {
        if self.pending {
            std::mem::swap(&mut self.current, &mut self.next);
            self.pending = false;
        }
        self.input.fill(Complex::ZERO);
        self.ready.fill(Complex::ZERO);
        self.position = 0;
    }

    /// One frame in, the frame `latency` earlier out.
    pub fn push(&mut self, left: f32, right: f32) -> (f32, f32) {
        self.input[self.len + self.position] = Complex::new(left, right);
        let out = self.ready[self.position];
        self.position += 1;
        if self.position == self.len {
            self.block();
            self.position = 0;
        }
        (out.re, out.im)
    }

    fn block(&mut self) {
        let len = self.len;
        self.work.copy_from_slice(&self.input);
        self.fft.forward(&mut self.work);
        if self.pending {
            for ((out, x), h) in self.switching.iter_mut().zip(&self.work).zip(&self.next) {
                *out = *x * *h;
            }
            self.fft.inverse(&mut self.switching);
        }
        for (x, h) in self.work.iter_mut().zip(&self.current) {
            *x = *x * *h;
        }
        self.fft.inverse(&mut self.work);
        if self.pending {
            for (i, out) in self.ready.iter_mut().enumerate() {
                let fade = (i + 1) as f32 / len as f32;
                *out = self.work[len + i].scale(1.0 - fade) + self.switching[len + i].scale(fade);
            }
            std::mem::swap(&mut self.current, &mut self.next);
            self.pending = false;
        } else {
            self.ready.copy_from_slice(&self.work[len..]);
        }
        self.input.copy_within(len.., 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_flat_kernel_is_a_delay_of_its_latency() {
        let mut convolver = Convolver::new(64);
        convolver.install(&[1.0; 65]);
        let mut out = Vec::new();
        for i in 0..400 {
            let x = if i == 3 { 1.0 } else { 0.0 };
            out.push(convolver.push(x, -x));
        }
        let at = 3 + convolver.latency();
        for (i, (left, right)) in out.iter().enumerate() {
            let expected = if i == at { 1.0 } else { 0.0 };
            assert!((left - expected).abs() < 1e-5 && (right + expected).abs() < 1e-5, "frame {i}: {left} {right}");
        }
    }

    #[test]
    fn an_offered_kernel_crossfades_in_over_one_block() {
        let mut convolver = Convolver::new(64);
        convolver.install(&[1.0; 65]);
        for _ in 0..200 {
            convolver.push(1.0, 1.0);
        }
        convolver.offer(&[0.5; 65]);
        let levels: Vec<f32> = (0..400).map(|_| convolver.push(1.0, 1.0).0).collect();
        assert!((levels[0] - 1.0).abs() < 1e-4 && (levels[399] - 0.5).abs() < 1e-4);
        assert!(levels.windows(2).all(|w| (w[1] - w[0]).abs() < 0.02), "no step on the way");
    }
}
