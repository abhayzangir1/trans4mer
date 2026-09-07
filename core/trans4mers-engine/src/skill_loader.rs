use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
pub struct SkillDefinition {
    pub name: String,
    pub description: String,
    pub version: String,
    pub steps: Vec<SkillStep>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SkillStep {
    pub name: String,
    pub instruction: String,
}

#[derive(Debug, Clone, Default)]
pub struct SkillCatalog {
    skills: HashMap<String, SkillDefinition>,
}

impl SkillCatalog {
    pub fn load_from_dir<P: AsRef<Path>>(dir: P) -> std::io::Result<Self> {
        let mut skills = HashMap::new();
        let path = dir.as_ref();
        if path.exists() && path.is_dir() {
            for entry in std::fs::read_dir(path)? {
                let entry = entry?;
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("toml") {
                    let content = std::fs::read_to_string(&path)?;
                    if let Ok(skill) = toml::from_str::<SkillDefinition>(&content) {
                        skills.insert(skill.name.clone(), skill);
                    } else {
                        tracing::warn!("Failed to parse skill TOML file: {:?}", path);
                    }
                }
            }
        } else {
            tracing::warn!("Skill directory not found or not a directory: {:?}", path);
        }
        Ok(Self { skills })
    }

    pub fn get_skill(&self, name: &str) -> Option<&SkillDefinition> {
        self.skills.get(name)
    }

    pub fn get_all_skills(&self) -> Vec<&SkillDefinition> {
        self.skills.values().collect()
    }
}
