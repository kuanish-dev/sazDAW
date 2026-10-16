//! `patterns.*` and `drums.build` methods: thin adapters over `PatternLibrary` and `DrumPartControl`.
//! A pattern is returned with a grid string per drum (`"X... x... X... x..."`) so both humans and AI can read it.

use daw_contracts::{DrumPartRequest, DrumPattern, PatternRole};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::RpcTable;

#[derive(Deserialize, JsonSchema)]
struct Filter {
    style: Option<String>,
    role: Option<PatternRole>,
}

#[derive(Deserialize, JsonSchema)]
struct Id {
    id: String,
}

#[derive(Serialize)]
struct LaneDto {
    lane: String,
    grid: String,
}

#[derive(Serialize)]
struct PatternDto {
    id: String,
    name: String,
    style: String,
    role: PatternRole,
    bars: u32,
    description: String,
    lanes: Vec<LaneDto>,
}

fn dto(pattern: DrumPattern) -> PatternDto {
    let steps = pattern.steps();
    PatternDto {
        lanes: pattern.lanes.iter().map(|l| LaneDto { lane: l.lane.clone(), grid: l.grid(steps) }).collect(),
        id: pattern.id,
        name: pattern.name,
        style: pattern.style,
        role: pattern.role,
        bars: pattern.bars,
        description: pattern.description,
    }
}

pub(crate) fn register(table: &mut RpcTable) {
    // Without params, all patterns; `{ style, role }` narrows the list.
    table.add(
        "patterns.list",
        "Lists the drum pattern library: grooves, fills, breaks, intros, endings (with grids). Filters: style and role.",
        |c, p: Option<Filter>| {
            let (style, role) = p.map_or((None, None), |f| (f.style, f.role));
            let all = c.patterns.patterns();
            Ok(all
                .into_iter()
                .filter(|pattern| style.as_ref().is_none_or(|s| *s == pattern.style) && role.is_none_or(|r| r == pattern.role))
                .map(dto)
                .collect::<Vec<_>>())
        },
    );
    table.add("patterns.get", "Returns one drum pattern by id.", |c, p: Id| c.patterns.pattern(&p.id).map(dto));
    table.add("drums.build", "Builds a drum part from patterns in sequence: drum tracks and clips in one undo step. What plays each drum is set by lanes (a built-in sound, your track, your sample, silence).", |c, p: DrumPartRequest| c.drum_part.build(p));
    // Preview: the project doesn't change; it loops until `drums.stopPreview` is called.
    table.add(
        "drums.preview",
        "Previews a drum part in a loop without changing the project (same request as drums.build; up to 16 bars).",
        |c, p: DrumPartRequest| c.drum_preview.play(p),
    );
    table.add("drums.stopPreview", "Stops the drum part preview.", |c, (): ()| Ok(c.drum_preview.stop()));
}
