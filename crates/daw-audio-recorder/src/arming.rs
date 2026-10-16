//! Record arming and per-track input choice: session state (not saved in the project). Every change goes to the engine
//! as a whole (`InputMonitor::set_track_inputs`): from it the engine opens the input and monitors it on the tracks.

use daw_contracts::{InputChannels, InputLevel, TrackId, TrackInput, TrackRecordState};

#[derive(Default)]
pub(crate) struct Arming {
    /// Tracks that are armed or have a non-default input.
    inputs: Vec<TrackInput>,
}

impl Arming {
    fn entry(&mut self, track: TrackId) -> &mut TrackInput {
        if !self.inputs.iter().any(|input| input.track == track) {
            self.inputs.push(TrackInput { track, armed: false, channels: InputChannels::Stereo });
        }
        let index = self.inputs.iter().position(|input| input.track == track).unwrap_or(self.inputs.len() - 1);
        &mut self.inputs[index]
    }

    pub fn set_armed(&mut self, track: TrackId, armed: bool) {
        self.entry(track).armed = armed;
        self.forget_defaults();
    }

    pub fn set_input(&mut self, track: TrackId, channels: InputChannels) {
        self.entry(track).channels = channels;
        self.forget_defaults();
    }

    /// Default entries are not needed: the engine table stays small.
    fn forget_defaults(&mut self) {
        self.inputs.retain(|input| input.armed || input.channels != InputChannels::Stereo);
    }

    /// Drops tracks that are gone (removed or no longer audio).
    pub fn keep_only(&mut self, alive: impl Fn(TrackId) -> bool) {
        self.inputs.retain(|input| alive(input.track));
    }

    pub fn inputs(&self) -> &[TrackInput] {
        &self.inputs
    }

    /// Armed tracks and their channels, in arming order.
    pub fn armed(&self) -> Vec<(TrackId, InputChannels)> {
        self.inputs.iter().filter(|input| input.armed).map(|input| (input.track, input.channels)).collect()
    }

    /// State for the interface: the input level follows each track channels.
    pub fn states(&self, level: InputLevel) -> Vec<TrackRecordState> {
        self.inputs
            .iter()
            .map(|input| {
                let (left, right) = input.channels.sides(level.left, level.right);
                TrackRecordState { track: input.track, armed: input.armed, input: input.channels, left, right }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_forgotten_and_levels_follow_the_channels() {
        let mut arming = Arming::default();
        arming.set_armed(3, true);
        arming.set_input(4, InputChannels::Right);
        arming.set_input(3, InputChannels::Left);
        assert_eq!(arming.armed(), [(3, InputChannels::Left)]);
        let states = arming.states(InputLevel { left: 0.5, right: 0.25 });
        assert_eq!((states[0].left, states[0].right), (0.5, 0.5));
        assert_eq!((states[1].left, states[1].right), (0.25, 0.25));
        arming.set_input(4, InputChannels::Stereo);
        arming.set_armed(3, false);
        assert_eq!(arming.inputs(), [TrackInput { track: 3, armed: false, channels: InputChannels::Left }]);
        arming.keep_only(|track| track != 3);
        assert!(arming.inputs().is_empty());
    }
}
