//! Drum patterns: ready-made figures (grooves, fills, breaks) from which a human or an AI builds a drum part.
//!
//! A pattern is a figure on a grid of 16th steps (`STEP_TICKS`), one line per drum (a "lane": `kick`, `snare`, `hat`…).
//! A line reads like on paper: `X` is an accented hit, `x` a normal one, `o` a quiet ("ghost") one, `.` a rest; spaces
//! and `|` are ignored for readability: `"X... x... X... x..."`. A part is a sequence of patterns played back to back
//! (`DrumPartControl::build`), each of which can be repeated.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::codes::patterns as codes;
use crate::{BAR_TICKS, ClipId, ContractError, STEP_TICKS, SampleId, TrackId};

/// 16th steps per bar.
pub const STEPS_PER_BAR: u32 = BAR_TICKS / STEP_TICKS;

/// Hit velocity for each grid symbol.
pub const VELOCITY_STRONG: u8 = 115;
pub const VELOCITY_NORMAL: u8 = 92;
pub const VELOCITY_GHOST: u8 = 52;

/// Symbol thresholds when turning a velocity back into a symbol: halfway between neighbouring levels, so a foreign
/// velocity (for example, from a MIDI file) maps to the nearest symbol.
const STRONG_FROM: u8 = VELOCITY_NORMAL + (VELOCITY_STRONG - VELOCITY_NORMAL) / 2;
const NORMAL_FROM: u8 = VELOCITY_GHOST + (VELOCITY_NORMAL - VELOCITY_GHOST) / 2;

/// What the pattern is for in a song.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PatternRole {
    /// The main rhythm: played for a long time, repeated.
    Groove,
    /// A short filler before a section change (usually the last bar or half bar).
    Fill,
    /// A stop or a strongly thinned rhythm, a "drop" before the groove returns.
    Break,
    /// A way to start (count-in, build-up).
    Intro,
    /// A way to finish (final hit, fade-out).
    Ending,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DrumHit {
    /// Index of the 16th step from the pattern start.
    pub step: u32,
    /// 1..=127
    pub velocity: u8,
}

/// Hits of one drum (`lane` is the drum kind: `kick`, `snare`, `hat`, `tom_hi`…).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct LaneHits {
    pub lane: String,
    pub hits: Vec<DrumHit>,
}

impl LaneHits {
    /// Parses a grid line; `steps` is how many steps the line must have (spaces and `|` do not count).
    pub fn from_grid(lane: &str, grid: &str, steps: u32) -> Result<Self, ContractError> {
        let mut hits = Vec::new();
        let mut step = 0u32;
        for symbol in grid.chars().filter(|c| !matches!(c, ' ' | '|')) {
            let velocity = match symbol {
                'X' => Some(VELOCITY_STRONG),
                'x' => Some(VELOCITY_NORMAL),
                'o' => Some(VELOCITY_GHOST),
                '.' | '-' => None,
                other => {
                    return Err(codes::BAD_GRID_SYMBOL.with("lane", lane).with("symbol", other));
                }
            };
            if let Some(velocity) = velocity {
                hits.push(DrumHit { step, velocity });
            }
            step += 1;
        }
        if step != steps {
            return Err(codes::GRID_LENGTH_MISMATCH.with("lane", lane).with("steps", step).with("expected", steps));
        }
        Ok(Self { lane: lane.to_string(), hits })
    }

    /// The grid as a line: steps grouped by four, bars separated by `|`.
    pub fn grid(&self, steps: u32) -> String {
        let mut text = String::new();
        for step in 0..steps {
            if step > 0 && step % STEPS_PER_BAR == 0 {
                text.push_str(" | ");
            } else if step > 0 && step % 4 == 0 {
                text.push(' ');
            }
            let symbol = match self.hits.iter().find(|h| h.step == step).map(|h| h.velocity) {
                None => '.',
                Some(v) if v >= STRONG_FROM => 'X',
                Some(v) if v >= NORMAL_FROM => 'x',
                Some(_) => 'o',
            };
            text.push(symbol);
        }
        text
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DrumPattern {
    /// Stable identifier (`rock_basic`, `fill_tom_cascade`) used to refer to the pattern in a part.
    pub id: String,
    pub name: String,
    /// Genre or group: `rock`, `house`, `hiphop`, `fills`…
    pub style: String,
    pub role: PatternRole,
    /// Length in bars.
    pub bars: u32,
    /// One sentence: how it sounds and when to use it.
    pub description: String,
    pub lanes: Vec<LaneHits>,
}

impl DrumPattern {
    pub fn steps(&self) -> u32 {
        self.bars * STEPS_PER_BAR
    }
}

/// Library of ready-made patterns. Job: know which figures exist.
pub trait PatternLibrary: Send + Sync {
    /// All patterns in a stable order (by genre and role).
    fn patterns(&self) -> Vec<DrumPattern>;
    /// A pattern by id; an unknown id is an `ErrorKind::Invalid` error listing similar ids.
    fn pattern(&self, id: &str) -> Result<DrumPattern, ContractError>;
}

fn once() -> u32 {
    1
}

/// One slot of a part: a pattern and how many times in a row to play it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PatternSlot {
    pub pattern: String,
    /// 1.. (once by default).
    #[serde(default = "once")]
    pub repeat: u32,
}

/// What plays a pattern's drum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum LaneTarget {
    /// A track with this built-in drum if one exists; otherwise it is created.
    #[default]
    Auto,
    /// The drum is silent: its hits are left out of the part (for example, no crash).
    Skip,
    /// Your track: notes go onto it (at `pitch`, by default the track's step-input pitch; for a sampler this is the
    /// root note at which the sound plays unchanged).
    Track {
        id: TrackId,
        #[serde(default)]
        pitch: Option<u8>,
    },
    /// Your sample from the project: a sampler track with this sound is created for the drum.
    Sample { id: SampleId },
}

/// Sound choice for one drum of the part.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct LaneAssignment {
    /// The pattern's drum (`kick`, `snare`, `hat`…).
    pub lane: String,
    pub target: LaneTarget,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DrumPartRequest {
    /// Bar where the part starts (0-based, per the tempo map: bars differ in length across time signature changes).
    #[serde(default)]
    pub start_bar: u32,
    pub slots: Vec<PatternSlot>,
    /// What to do with clips already in the way: `false` refuses with a readable error, `true` removes them.
    #[serde(default)]
    pub replace: bool,
    /// What plays each drum; drums not listed here are `Auto`. Several drums can go to one track: their hits merge
    /// into one clip. A drum that is not in the part is an error (most likely a typo).
    #[serde(default)]
    pub lanes: Vec<LaneAssignment>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DrumPartReport {
    /// How many bars the part took.
    pub bars: u32,
    /// Created clips (one per drum per part slot).
    pub clips: Vec<ClipId>,
    /// Drum tracks the part plays on (missing ones are created).
    pub tracks: Vec<TrackId>,
    /// Names of the created tracks.
    pub created_tracks: Vec<String>,
}

/// Drum part building. Job: turn a sequence of patterns into note clips on drum tracks in one undo step. Either the
/// whole part is built or the project stays as it was.
pub trait DrumPartControl: Send + Sync {
    fn build(&self, request: DrumPartRequest) -> Result<DrumPartReport, ContractError>;
}

/// Drum part preview. Job: play the part as it will sound once built (same patterns, same drum sounds) WITHOUT
/// changing the project. Loops until `stop` is called; a new `play` replaces the previous one. The request's
/// `start_bar` and `replace` are ignored (nothing is placed in the project). A part longer than 16 bars is refused.
pub trait DrumPartPreview: Send + Sync {
    fn play(&self, request: DrumPartRequest) -> Result<(), ContractError>;
    fn stop(&self);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lane_targets_have_a_friendly_json_shape() {
        let request: DrumPartRequest = serde_json::from_str(
            r#"{"slots":[{"pattern":"rock_basic"}],"lanes":[
                {"lane":"kick","target":{"type":"track","id":5}},
                {"lane":"snare","target":{"type":"track","id":6,"pitch":38}},
                {"lane":"hat","target":{"type":"sample","id":2}},
                {"lane":"crash","target":{"type":"skip"}},
                {"lane":"clap","target":{"type":"auto"}}]}"#,
        )
        .unwrap();
        let targets: Vec<LaneTarget> = request.lanes.iter().map(|l| l.target).collect();
        assert_eq!(
            targets,
            [
                LaneTarget::Track { id: 5, pitch: None },
                LaneTarget::Track { id: 6, pitch: Some(38) },
                LaneTarget::Sample { id: 2 },
                LaneTarget::Skip,
                LaneTarget::Auto,
            ]
        );
        let plain: DrumPartRequest = serde_json::from_str(r#"{"slots":[{"pattern":"rock_basic"}]}"#).unwrap();
        assert!(plain.lanes.is_empty() && !plain.replace && plain.start_bar == 0 && plain.slots[0].repeat == 1);
    }

    #[test]
    fn grid_round_trips_with_accents_and_ignores_spacing() {
        let lane = LaneHits::from_grid("snare", "....X... ..o.x...", 16).unwrap();
        assert_eq!(lane.hits.iter().map(|h| (h.step, h.velocity)).collect::<Vec<_>>(), [(4, 115), (10, 52), (12, 92)]);
        assert_eq!(lane.grid(16), ".... X... ..o. x...");
        assert_eq!(LaneHits::from_grid("snare", &lane.grid(16), 16).unwrap(), lane);
    }

    #[test]
    fn a_wrong_length_or_symbol_is_a_readable_error() {
        let short = LaneHits::from_grid("kick", "x...", 16).unwrap_err().to_string();
        assert!(short.contains("kick") && short.contains("4 steps") && short.contains("16"), "{short}");
        let bad = LaneHits::from_grid("kick", "x..?", 4).unwrap_err().to_string();
        assert!(bad.contains('?'), "{bad}");
    }

    #[test]
    fn two_bar_grids_are_split_by_a_bar_line() {
        let lane = LaneHits::from_grid("kick", &format!("{}{}", "x...".repeat(4), "X...".repeat(4)), 32).unwrap();
        assert_eq!(lane.grid(32), "x... x... x... x... | X... X... X... X...");
    }
}
