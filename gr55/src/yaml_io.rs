use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use gr55_core::SystemArea;

pub fn load_system_area(path: &Path) -> Result<SystemArea> {
    let content =
        fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    serde_yaml::from_str(&content).with_context(|| format!("parsing {}", path.display()))
}

pub fn save_system_area(path: &Path, area: &SystemArea) -> Result<()> {
    let yaml = serde_yaml::to_string(area).context("serializing SystemArea")?;
    if path == Path::new("-") {
        print!("{yaml}");
    } else {
        fs::write(path, yaml).with_context(|| format!("writing {}", path.display()))?;
    }
    Ok(())
}
