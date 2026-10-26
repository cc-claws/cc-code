use std::path::{Path, PathBuf};

use super::invocation::parse_skill;

/// Skill 元数据（来自 SKILL.md frontmatter）
#[derive(Debug, Clone)]
pub struct SkillMetadata {
    pub name: String,
    pub description: String,
    pub path: PathBuf,
}

/// 加载单个 SKILL.md，解析 frontmatter，返回元数据
pub fn load_skill_metadata(path: &Path) -> Option<SkillMetadata> {
    let content = std::fs::read_to_string(path).ok()?;
    let (frontmatter, _) = parse_skill(&content).ok()?;
    let name = frontmatter.name.filter(|name| !name.trim().is_empty()).or_else(|| {
        path.parent()?.file_name()?.to_str().map(str::to_string)
    })?;

    Some(SkillMetadata {
        name,
        description: frontmatter.description.unwrap_or_default(),
        path: path.to_path_buf(),
    })
}

/// 扫描多个目录，返回所有可用 skill 元数据
///
/// 同名 skill 以先出现的为准（dirs 中靠前的目录优先）。
pub fn list_skills(dirs: &[PathBuf]) -> Vec<SkillMetadata> {
    let mut seen_names = std::collections::HashSet::new();
    let mut skills = Vec::new();

    for dir in dirs {
        if !dir.is_dir() {
            continue;
        }

        // 遍历直接子目录，每个子目录中寻找 SKILL.md
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => continue,
        };

        let mut dir_skills: Vec<SkillMetadata> = entries
            .filter_map(|entry| {
                let entry = entry.ok()?;
                let path = entry.path();
                if path.is_dir() {
                    let skill_file = path.join("SKILL.md");
                    if skill_file.is_file() {
                        return load_skill_metadata(&skill_file);
                    }
                } else if path.is_file()
                    && path.file_name().map(|n| n == "SKILL.md").unwrap_or(false)
                {
                    return load_skill_metadata(&path);
                }
                None
            })
            .collect();

        // 按名称排序保持稳定顺序
        dir_skills.sort_by(|a, b| a.name.cmp(&b.name));

        for skill in dir_skills {
            if seen_names.insert(skill.name.to_lowercase()) {
                skills.push(skill);
            }
        }
    }

    skills
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    include!("loader_test.rs");
}
