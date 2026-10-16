//! Layout of a project folder.
//!
//! ```text
//! My track/
//!   My track.dawproj   — the project itself
//!   samples/           — copies of the project's sound files
//!   autosave/          — autosave (gone after a normal exit)
//!   exports/           — renders and stems by default (the interface builds the path)
//! ```

use std::path::{Path, PathBuf};

pub(crate) struct ProjectLayout {
    root: PathBuf,
}

impl ProjectLayout {
    /// From the path of a `.dawproj` file; its parent is the project folder.
    pub(crate) fn from_file(file: &Path) -> Self {
        let root = file.parent().filter(|p| !p.as_os_str().is_empty()).map_or_else(|| PathBuf::from("."), Path::to_path_buf);
        Self { root }
    }

    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    pub(crate) fn samples_dir(&self) -> PathBuf {
        self.root.join("samples")
    }

    pub(crate) fn autosave_file(&self) -> PathBuf {
        self.root.join("autosave/autosave.dawproj")
    }
}

/// A name fit for a file: no path separators or special characters.
pub(crate) fn sanitize_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control() { '_' } else { c })
        .collect();
    let trimmed = cleaned.trim().trim_matches('.').trim();
    if trimmed.is_empty() { daw_contracts::tr!("Project", "Проект") } else { trimmed.to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_places_everything_next_to_the_project_file() {
        let layout = ProjectLayout::from_file(Path::new("/music/Track/Track.dawproj"));
        assert_eq!(layout.root(), Path::new("/music/Track"));
        assert_eq!(layout.samples_dir(), Path::new("/music/Track/samples"));
        assert_eq!(layout.autosave_file(), Path::new("/music/Track/autosave/autosave.dawproj"));
        assert_eq!(ProjectLayout::from_file(Path::new("a.dawproj")).root(), Path::new("."));
    }

    #[test]
    fn names_are_made_safe() {
        // Non-ASCII names pass through unchanged.
        assert_eq!(sanitize_name("Мой/трек: 1?"), "Мой_трек_ 1_");
        assert_eq!(sanitize_name("  .. "), "Project");
        assert_eq!(sanitize_name(""), "Project");
    }
}
