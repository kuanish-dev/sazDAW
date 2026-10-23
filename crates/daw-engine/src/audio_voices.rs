//! Audio clip voices: started when a clip enters a segment, they play the clip sound (with rate conversion, gain,
//! fades and crossfades with overlapped neighbours) into the `Audio` node buffer of their track and hand the sound
//! over to be freed when done. A clip fitted to the tempo or shifted in pitch plays the processed sound from the
//! snapshot (`EngineSnapshot::clip_audio`, computed outside the audio thread); when it changes (a new tempo), the voice
//! moves to the new sound at the same clip position. A take comp (`Clip::take_span`) gets one voice per part: adjacent
//! parts overlap slightly, and at the seam one voice fades out while the other comes in with an equal-power crossfade
//! (like overlapped clips).

use std::f64::consts::FRAC_PI_2;
use std::sync::Arc;

use daw_contracts::{BlockTiming, Clip, ClipContent, ClipId, EngineSnapshot, FadeShape, SampleData, SampleId, TakeSpan, TrackId};

use crate::link::{Garbage, GarbageBin};
use crate::mixer::GraphMixer;

const MAX_VOICES: usize = 256;

struct AudioVoice {
    clip: ClipId,
    /// The comp part number (`Clip::take_span`; 0 without a comp) and its take.
    part: usize,
    take: SampleId,
    data: Arc<SampleData>,
    track: TrackId,
    /// The position in the sound (frames of `data`).
    position: f64,
    /// The segment frame from which the voice starts sounding.
    start_frame: usize,
    gain: f32,
    clip_start: f64,
    clip_end: f64,
    fade_in: f64,
    fade_out: f64,
    fade_in_shape: FadeShape,
    fade_out_shape: FadeShape,
    /// Crossfades with overlapped neighbours on the track (ticks): equal-power, as long as the overlap.
    crossfade_in: f64,
    crossfade_out: f64,
    /// Where the voice sounds (the comp part with its transitions) and the take transitions at its edges (ticks).
    span: TakeSpan,
    /// The sound or the part has ended (or the track has no node): the voice leaves after the segment.
    finished: bool,
}

pub struct AudioVoices {
    voices: Vec<AudioVoice>,
}

impl AudioVoices {
    pub fn new() -> Self {
        Self { voices: Vec::with_capacity(MAX_VOICES) }
    }

    /// Cuts off all voices. Their sounds (a voice may be the last owner, and that is tens of MB) are freed on the control
    /// side, not here.
    pub fn drop_all(&mut self, bin: &mut GarbageBin) {
        for voice in self.voices.drain(..) {
            bin.throw(Garbage::Sample(voice.data));
        }
    }

    /// Starts voices for the clip parts that sound in the segment and are not playing yet; a voice whose sound changed in
    /// the snapshot (processing for a new tempo is ready) moves to the new sound. If the comp changed, playing voices
    /// follow their parts, and the voice of a part that is gone (or now has another take) leaves.
    pub fn start(&mut self, snapshot: &EngineSnapshot, timing: &BlockTiming, sample_rate: u32, bin: &mut GarbageBin) {
        let (start, end) = (timing.start_tick, timing.end_tick());
        let map = &snapshot.project.tempo_map;
        for clip in &snapshot.project.clips {
            let ClipContent::Audio { .. } = clip.content else {
                continue;
            };
            let clip_start = f64::from(clip.start_tick);
            let clip_end = clip_start + f64::from(clip.length_ticks);
            if clip_end <= start || clip_start >= end {
                continue;
            }
            for voice in self.voices.iter_mut().filter(|v| v.clip == clip.id) {
                match clip.take_span(voice.part, map) {
                    Some(span) if span.take == voice.take => voice.span = span,
                    _ => voice.finished = true,
                }
            }
            for part in 0..clip.take_span_count() {
                let Some(span) = clip.take_span(part, map) else { continue };
                if span.end <= start || span.start >= end {
                    continue;
                }
                self.start_part(snapshot, clip, part, span, timing, sample_rate, bin);
            }
        }
    }

    /// The voice of clip part `part`, if it is not playing this sound yet.
    #[allow(clippy::too_many_arguments, reason = "all of this is one part of one segment; a struct just for the call would be noise")]
    fn start_part(
        &mut self,
        snapshot: &EngineSnapshot,
        clip: &Clip,
        part: usize,
        span: TakeSpan,
        timing: &BlockTiming,
        sample_rate: u32,
        bin: &mut GarbageBin,
    ) {
        let Some((data, origin)) = Self::sound(snapshot, clip, span.take) else { return };
        let playing = self.voices.iter().position(|v| v.clip == clip.id && v.part == part && !v.finished);
        if let Some(index) = playing {
            if Arc::ptr_eq(&self.voices[index].data, data) {
                return;
            }
            bin.throw(Garbage::Sample(self.voices.swap_remove(index).data));
        }
        if self.voices.len() >= MAX_VOICES {
            return;
        }
        let map = &snapshot.project.tempo_map;
        let clip_start = f64::from(clip.start_tick);
        let ratio = f64::from(data.sample_rate) / f64::from(sample_rate);
        let (start_frame, from_tick) =
            if span.start >= timing.start_tick { (timing.frame_at(span.start), span.start) } else { (0, timing.start_tick) };
        let position = origin + map.frames_between(clip_start, from_tick, sample_rate) * ratio;
        if position as usize + 1 >= data.frame_count() {
            return;
        }
        let (crossfade_in, crossfade_out) = clip.crossfades(&snapshot.project.clips);
        self.voices.push(AudioVoice {
            clip: clip.id,
            part,
            take: span.take,
            data: Arc::clone(data),
            track: clip.track,
            position,
            start_frame,
            gain: clip.gain,
            clip_start,
            clip_end: clip_start + f64::from(clip.length_ticks),
            fade_in: f64::from(clip.fade_in_ticks),
            fade_out: f64::from(clip.fade_out_ticks),
            fade_in_shape: clip.fade_in_shape,
            fade_out_shape: clip.fade_out_shape,
            crossfade_in: f64::from(crossfade_in),
            crossfade_out: f64::from(crossfade_out),
            span,
            finished: false,
        });
    }

    /// What take `take` of the clip plays: the sound from `clip_audio` (for a clip fitted to tempo or pitch) or the sample
    /// itself, and the frame of that sound at the clip start. No sound (still being computed) means `None`: the part is
    /// silent rather than out of time.
    fn sound<'a>(snapshot: &'a EngineSnapshot, clip: &Clip, take: SampleId) -> Option<(&'a Arc<SampleData>, f64)> {
        if clip.has_sound_processing() {
            return snapshot.clip_audio.get(&(clip.id, take)).map(|audio| (&audio.data, audio.start_frame));
        }
        snapshot.samples.get(&take).map(|data| (data, clip.offset_frames as f64))
    }

    /// Adds the voices' sound to the `Audio` node buffers; finished voices leave.
    pub fn render(&mut self, mixer: &mut GraphMixer, timing: &BlockTiming, frames: usize, sample_rate: u32, bin: &mut GarbageBin) {
        for voice in self.voices.iter_mut().filter(|v| !v.finished) {
            match mixer.audio_buffer(voice.track, frames) {
                Some(buffer) => voice.render(buffer, timing, frames, sample_rate),
                None => voice.finished = true,
            }
        }
        for voice in self.voices.extract_if(.., |voice| voice.finished) {
            bin.throw(Garbage::Sample(voice.data));
        }
    }
}

impl AudioVoice {
    fn render(&mut self, buffer: &mut [f32], timing: &BlockTiming, frames: usize, sample_rate: u32) {
        let ratio = f64::from(self.data.sample_rate) / f64::from(sample_rate);
        let frame_count = self.data.frame_count();
        for frame in self.start_frame..frames {
            let tick = timing.start_tick + frame as f64 * timing.ticks_per_frame;
            if self.position as usize + 1 >= frame_count || tick >= self.clip_end || tick >= self.span.end {
                self.finished = true;
                break;
            }
            let (left, right) = self.data.stereo_lerp(self.position);
            let gain = self.gain * self.fade_at(tick);
            buffer[frame * 2] += left * gain;
            buffer[frame * 2 + 1] += right * gain;
            self.position += ratio;
        }
        self.start_frame = 0;
    }

    /// The fade (in the clip's shapes) and crossfade gain at the clip start and end at moment `tick`.
    fn fade_at(&self, tick: f64) -> f32 {
        let mut gain = 1.0;
        let local = (tick - self.clip_start).max(0.0);
        if self.fade_in > 0.0 && local < self.fade_in {
            gain *= self.fade_in_shape.gain(local / self.fade_in);
        }
        let remaining = self.clip_end - tick;
        if self.fade_out > 0.0 && remaining < self.fade_out {
            gain *= self.fade_out_shape.gain(remaining / self.fade_out);
        }
        if self.crossfade_in > 0.0 && local < self.crossfade_in {
            gain *= (local / self.crossfade_in * FRAC_PI_2).sin();
        }
        if self.crossfade_out > 0.0 && remaining < self.crossfade_out {
            gain *= (remaining / self.crossfade_out * FRAC_PI_2).sin();
        }
        let into_span = tick - self.span.start;
        if self.span.fade_in > 0.0 && into_span < self.span.fade_in {
            gain *= (into_span.max(0.0) / self.span.fade_in * FRAC_PI_2).sin();
        }
        let span_left = self.span.end - tick;
        if self.span.fade_out > 0.0 && span_left < self.span.fade_out {
            gain *= (span_left.max(0.0) / self.span.fade_out * FRAC_PI_2).sin();
        }
        gain as f32
    }
}
