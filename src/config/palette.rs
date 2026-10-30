use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const DEFAULT_COLORS: [[u8; 4]; 6] = [
    [187, 255, 139, 255],
    [155, 216, 244, 255],
    [248, 181, 139, 255],
    [226, 255, 189, 255],
    [255, 255, 255, 255],
    [255, 177, 192, 255],
];

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Palette {
    pub colors: [[u8; 4]; 6],
    pub selected: usize,
}
impl Default for Palette {
    fn default() -> Self {
        Self {
            colors: DEFAULT_COLORS,
            selected: 0,
        }
    }
}
impl Palette {
    pub fn load(path: &Path) -> Result<Self> {
        let mut palette: Self = toml::from_str(&std::fs::read_to_string(path)?)?;
        palette.selected = palette.selected.min(5);
        Ok(palette)
    }
    pub fn save(&self, path: &Path) -> Result<()> {
        let parent = path
            .parent()
            .context("Palette path needs a parent directory")?;
        std::fs::create_dir_all(parent)?;
        let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
        std::fs::write(&temporary, toml::to_string_pretty(self)?)?;
        std::fs::rename(temporary, path)?;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn palette_round_trip_keeps_all_slots() {
        let path = std::env::temp_dir().join(format!(
            "hyprannotations-palette-{}.toml",
            std::process::id()
        ));
        let mut palette = Palette::default();
        palette.colors[1] = [12, 34, 56, 255];
        palette.colors[5] = [98, 76, 54, 255];
        palette.selected = 5;
        palette.save(&path).unwrap();
        let restored = Palette::load(&path).unwrap();
        assert_eq!(restored.colors, palette.colors);
        assert_eq!(restored.selected, 5);
        std::fs::remove_file(path).unwrap();
    }
}
