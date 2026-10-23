//! `PatternBank`: "know which drum patterns exist". It parses the beat table (`data.rs`) once on creation.

use daw_contracts::codes::patterns;
use daw_contracts::{AppLog, ContractError, DrumPattern, LaneHits, LogEntry, PatternLibrary, STEPS_PER_BAR};

use crate::data::{Def, PATTERNS};

pub struct PatternBank {
    entries: Vec<Entry>,
}

/// A parsed table row. Name and description stay bilingual and are picked per request, so a language change shows up
/// on the next `patterns()` without rebuilding the bank.
struct Entry {
    def: &'static Def,
    lanes: Vec<LaneHits>,
}

impl PatternBank {
    /// Parses the built-in table. An error in the table is a programming error caught by `every_builtin_pattern_is_valid`;
    /// in the running app such a pattern is skipped with a log entry rather than failing the start.
    pub fn builtin(log: &dyn AppLog) -> Self {
        let entries = PATTERNS
            .iter()
            .filter_map(|def| {
                parse(def)
                    .map_err(|error| log.record(LogEntry::error("patterns", format!("built-in pattern \"{}\" skipped: {error}", def.id))))
                    .ok()
            })
            .collect();
        Self { entries }
    }
}

fn parse(def: &'static Def) -> Result<Entry, ContractError> {
    let steps = def.bars * STEPS_PER_BAR;
    let lanes = def.lanes.iter().map(|(lane, grid)| LaneHits::from_grid(lane, grid, steps)).collect::<Result<Vec<_>, _>>()?;
    Ok(Entry { def, lanes })
}

impl Entry {
    fn pattern(&self) -> DrumPattern {
        let def = self.def;
        DrumPattern {
            id: def.id.into(),
            name: def.name.get().into(),
            style: def.style.into(),
            role: def.role,
            bars: def.bars,
            description: def.description.get().into(),
            lanes: self.lanes.clone(),
        }
    }
}

impl PatternLibrary for PatternBank {
    fn patterns(&self) -> Vec<DrumPattern> {
        self.entries.iter().map(Entry::pattern).collect()
    }

    fn pattern(&self, id: &str) -> Result<DrumPattern, ContractError> {
        self.entries.iter().find(|e| e.def.id == id).map(Entry::pattern).ok_or_else(|| {
            let stem = id.split('_').next().unwrap_or(id);
            let similar: Vec<&str> = self.entries.iter().map(|e| e.def.id).filter(|known| known.contains(stem)).take(8).collect();
            if similar.is_empty() {
                patterns::NOT_FOUND.with("id", id)
            } else {
                patterns::NOT_FOUND_SIMILAR.with("id", id).with("similar", similar.join(", "))
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use daw_contracts::PatternRole;

    use super::*;

    /// Drums the built-in drum instruments can play (`daw-drums`).
    const DRUM_KINDS: [&str; 11] = ["kick", "snare", "hat", "clap", "openhat", "tom_hi", "tom_mid", "tom_lo", "crash", "ride", "rim"];

    #[test]
    fn every_builtin_pattern_is_valid() {
        let bank = PatternBank::builtin(&crate::testing::NoLog);
        assert_eq!(bank.entries.len(), PATTERNS.len(), "every table row parses without errors");
        for def in PATTERNS {
            let steps = def.bars * STEPS_PER_BAR;
            assert!((1..=2).contains(&def.bars), "{}: 1–2 bars long", def.id);
            for text in [def.name, def.description] {
                assert!(!text.en.is_empty() && !text.ru.is_empty(), "{}: name and description in both languages", def.id);
            }
            let mut seen = HashSet::new();
            for (lane, grid) in def.lanes {
                assert!(DRUM_KINDS.contains(lane), "{}: unknown drum \"{lane}\"", def.id);
                assert!(seen.insert(*lane), "{}: drum \"{lane}\" is listed twice", def.id);
                let hits = LaneHits::from_grid(lane, grid, steps).unwrap_or_else(|e| panic!("{}: {e}", def.id));
                assert!(!hits.hits.is_empty(), "{}: drum \"{lane}\" has no hits", def.id);
            }
        }
    }

    #[test]
    fn ids_are_unique_and_machine_friendly() {
        let ids: Vec<&str> = PATTERNS.iter().map(|d| d.id).collect();
        let unique: HashSet<&&str> = ids.iter().collect();
        assert_eq!(unique.len(), ids.len(), "ids repeat");
        assert!(ids.iter().all(|id| id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')));
    }

    #[test]
    fn the_library_is_big_enough_and_covers_every_role() {
        let bank = PatternBank::builtin(&crate::testing::NoLog);
        let patterns = bank.patterns();
        let count = |role| patterns.iter().filter(|p| p.role == role).count();
        assert!(count(PatternRole::Groove) >= 20, "too few grooves");
        assert!(count(PatternRole::Fill) >= 8, "too few fills");
        assert!(count(PatternRole::Break) >= 4 && count(PatternRole::Intro) >= 3 && count(PatternRole::Ending) >= 2);
        assert!(patterns.iter().any(|p| p.bars == 2), "there are two-bar patterns too");
    }

    #[test]
    fn fills_are_one_bar_so_they_can_replace_the_last_bar_of_a_phrase() {
        let bank = PatternBank::builtin(&crate::testing::NoLog);
        assert!(bank.patterns().iter().filter(|p| p.role == PatternRole::Fill).all(|p| p.bars == 1));
    }

    #[test]
    fn unknown_pattern_gets_a_helpful_error() {
        let bank = PatternBank::builtin(&crate::testing::NoLog);
        let error = bank.pattern("rock_nonsense").unwrap_err();
        assert!(error.is(patterns::NOT_FOUND_SIMILAR), "{error}");
        assert!(error.to_string().contains("rock_nonsense") && error.to_string().contains("rock_basic"), "{error}");
        assert!(bank.pattern("zzz").unwrap_err().is(patterns::NOT_FOUND));
        assert_eq!(bank.pattern("rock_basic").unwrap().name, "Rock: basic");
    }
}
