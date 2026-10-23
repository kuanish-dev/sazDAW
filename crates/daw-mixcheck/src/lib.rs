//! Mix check (`MixChecker`, contract `MixCheck`): "find what makes the project sound different from what was
//! intended". It is mainly for AI: it cannot hear the result and without a check would not know the bass is silent.
//!
//! Layout: the rules are pure functions over the project, sound levels and kind information (`rules.rs`, thresholds
//! and heuristics are described there); this file only gathers the input: track and master levels per bar
//! (`MixAnalysis`), kind parameters from the catalogs, disabled and missing plugins. A new rule = a function in
//! `rules.rs` + a line in `MixChecker::check` + a test in `tests.rs`.

mod rules;
#[cfg(test)]
mod tests;

use std::collections::BTreeSet;
use std::sync::Arc;

use daw_contracts::{
    AppLog, ContractError, EffectCatalog, Finding, InstrumentCatalog, LevelReport, LevelRequest, LevelTargets, LogEntry, MixAnalysis,
    MixCheck, NodeKind, PluginScanner, Project, Severity,
};

use rules::Kinds;

pub struct MixChecker {
    analysis: Arc<dyn MixAnalysis>,
    instruments: Arc<dyn InstrumentCatalog>,
    effects: Arc<dyn EffectCatalog>,
    plugins: Arc<dyn PluginScanner>,
    log: Arc<dyn AppLog>,
}

impl MixChecker {
    pub fn new(
        analysis: Arc<dyn MixAnalysis>,
        instruments: Arc<dyn InstrumentCatalog>,
        effects: Arc<dyn EffectCatalog>,
        plugins: Arc<dyn PluginScanner>,
        log: Arc<dyn AppLog>,
    ) -> Self {
        Self { analysis, instruments, effects, plugins, log }
    }

    /// Kind parameters from the catalogs and project kinds that cannot be created (disabled or missing).
    fn kinds(&self, project: &Project) -> Kinds {
        let params = |kinds: Vec<daw_contracts::KindInfo>| kinds.into_iter().map(|k| (k.kind.to_string(), k.params)).collect();
        let disabled: BTreeSet<String> = self.plugins.installed().into_iter().filter(|p| p.disabled).map(|p| p.kind).collect();
        let mut missing = BTreeSet::new();
        for spec in project.tracks.iter().filter_map(|t| t.sounding_instrument()) {
            if self.instruments.defaults(&spec.kind).is_err() {
                missing.insert(spec.kind.clone());
            }
        }
        for node in &project.graph.nodes {
            if let NodeKind::Effect { effect, .. } = &node.kind
                && self.effects.defaults(effect).is_err()
            {
                missing.insert(effect.clone());
            }
        }
        Kinds { instruments: params(self.instruments.kinds()), effects: params(self.effects.kinds()), disabled, missing }
    }

    /// Track and master levels per bar.
    fn levels(&self, project: &Project, from: Option<u32>, to: Option<u32>) -> Result<(LevelReport, LevelReport), ContractError> {
        let request = |per| LevelRequest { from_tick: from, to_tick: to, per, resolution_bars: Some(1) };
        Ok((self.analysis.levels(project, &request(LevelTargets::Track))?, self.analysis.levels(project, &request(LevelTargets::Master))?))
    }
}

impl MixCheck for MixChecker {
    fn check(&self, project: &Project, from_tick: Option<u32>, to_tick: Option<u32>) -> Vec<Finding> {
        let kinds = self.kinds(project);
        let mut findings = Vec::new();
        findings.extend(rules::plugins(project, &kinds));
        findings.extend(rules::mute_and_solo(project));
        findings.extend(rules::suspicious_params(project, &kinds));
        findings.extend(rules::routing(project));
        findings.extend(rules::automation_ranges(project, &kinds));
        if !project.clips.is_empty() {
            match self.levels(project, from_tick, to_tick) {
                Ok((tracks, master)) => {
                    findings.extend(rules::track_sound(project, &tracks));
                    findings.extend(rules::master_sound(project, &master));
                }
                Err(error) => {
                    self.log.record(LogEntry::warn("analysis", format!("mix check: the sound was not rendered: {error}")));
                    findings.push(Finding::new(Severity::Error, "renderFailed", format!("The project sound was not rendered: {error}.")));
                }
            }
        }
        findings.sort_by_key(|f| f.severity);
        findings
    }
}
