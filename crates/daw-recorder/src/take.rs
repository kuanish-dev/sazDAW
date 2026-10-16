//! The take per track: which clip each track writes to, which pass it is on and which notes the take already recorded
//! (the next pass replaces them). Bookkeeping only; the recorder makes the project edits.

use std::collections::{BTreeMap, BTreeSet};

use daw_contracts::{Clip, ClipContent, ClipId, ControllerLane, NoteId, TrackId};

/// Where the take writes on one track.
pub(crate) struct Take {
    pub clip: ClipId,
    /// Recording loop length: the clip content at the start.
    pub loop_ticks: u32,
    /// Added to the transport tick to get the time inside the clip content.
    pub shift: f64,
    /// Clip controller lanes at the start: the recorded span replaces their points only inside itself.
    pub controllers: Vec<ControllerLane>,
    /// The pass being played now.
    pass: Option<u64>,
    /// Notes recorded in this take: (pass, id). Notes of earlier passes are replaced.
    recorded: Vec<(u64, NoteId)>,
}

#[cfg(test)]
impl Take {
    /// A take in clip `clip` with lanes `controllers`, from zero, one bar per pass.
    pub fn for_tests(clip: ClipId, controllers: Vec<ControllerLane>) -> Self {
        Self { clip, loop_ticks: daw_contracts::BAR_TICKS, shift: 0.0, controllers, pass: None, recorded: Vec::new() }
    }
}

#[derive(Default)]
pub(crate) struct TrackTakes {
    takes: BTreeMap<TrackId, Take>,
    /// Tracks for which no clip was found or created: their notes are skipped in this take.
    dropped: BTreeSet<TrackId>,
}

/// The track note clip that tick `at` falls under.
pub(crate) fn clip_under(clips: Vec<Clip>, track: TrackId, at: f64) -> Option<Clip> {
    clips.into_iter().find(|clip| {
        clip.track == track
            && matches!(clip.content, ClipContent::Notes(_))
            && f64::from(clip.start_tick) <= at
            && at < f64::from(clip.start_tick) + f64::from(clip.length_ticks)
    })
}

impl TrackTakes {
    pub fn clear(&mut self) {
        self.takes.clear();
        self.dropped.clear();
    }

    pub fn get(&self, track: TrackId) -> Option<&Take> {
        self.takes.get(&track)
    }

    pub fn is_bound(&self, track: TrackId) -> bool {
        self.takes.contains_key(&track)
    }

    pub fn is_dropped(&self, track: TrackId) -> bool {
        self.dropped.contains(&track)
    }

    /// Nowhere to write on the track: its notes are skipped in this take.
    pub fn drop_track(&mut self, track: TrackId) {
        self.dropped.insert(track);
    }

    /// The track writes into `clip`; `false` means it is not a note clip and the track is skipped.
    pub fn bind(&mut self, track: TrackId, clip: &Clip) -> bool {
        let ClipContent::Notes(content) = &clip.content else {
            self.drop_track(track);
            return false;
        };
        let take = Take {
            clip: clip.id,
            loop_ticks: content.loop_ticks.max(1),
            shift: f64::from(clip.offset_ticks) - f64::from(clip.start_tick),
            controllers: content.controllers.clone(),
            pass: None,
            recorded: Vec::new(),
        };
        self.takes.insert(track, take);
        true
    }

    /// Whether a note on pass `pass` opens a new pass (then accumulated notes must be saved before replacing earlier passes).
    pub fn starts_new_pass(&self, track: TrackId, pass: u64) -> bool {
        self.takes.get(&track).and_then(|take| take.pass).is_some_and(|current| pass > current)
    }

    /// A note on pass `pass`. The first note of a track sets the current pass (the take may start mid-way through clip
    /// repeats); the first note of a new pass returns the clip and the notes of earlier passes to remove (`Some`);
    /// otherwise `None`.
    pub fn enter_pass(&mut self, track: TrackId, pass: u64) -> Option<(ClipId, Vec<NoteId>)> {
        let take = self.takes.get_mut(&track)?;
        match take.pass {
            Some(current) if pass > current => {
                take.pass = Some(pass);
                let (old, keep): (Vec<_>, Vec<_>) = take.recorded.iter().copied().partition(|(p, _)| *p < pass);
                take.recorded = keep;
                Some((take.clip, old.into_iter().map(|(_, id)| id).collect()))
            }
            Some(_) => None,
            None => {
                take.pass = Some(pass);
                None
            }
        }
    }

    /// Notes landed in the track clip: remember their passes so the next pass replaces them.
    pub fn record(&mut self, track: TrackId, passes: impl IntoIterator<Item = u64>, ids: impl IntoIterator<Item = NoteId>) {
        if let Some(take) = self.takes.get_mut(&track) {
            take.recorded.extend(passes.into_iter().zip(ids));
        }
    }

    /// How many notes the take recorded.
    pub fn note_count(&self) -> usize {
        self.takes.values().map(|take| take.recorded.len()).sum()
    }
}
