//! Skill 加载与参数展开的共享入口（工具调用和 Slash 共用）。

use std::path::{Path, PathBuf};

use gray_matter::{engine::YAML, Matter};
use regex::{Captures, Regex};
use serde::Deserialize;

use super::{list_skills, SkillMetadata};

pub type SkillError = Box<dyn std::error::Error + Send + Sync>;

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct SkillFrontmatter {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub allowed_tools: Option<SkillToolList>,
    #[serde(default)]
    pub context: Option<String>,
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub arguments: Option<SkillArgumentNames>,
    #[serde(default)]
    pub disable_model_invocation: bool,
    #[serde(default = "default_user_invocable")]
    pub user_invocable: bool,
}

fn default_user_invocable() -> bool {
    true
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum SkillToolList {
    Text(String),
    List(Vec<String>),
}

impl SkillToolList {
    pub fn to_vec(&self) -> Vec<String> {
        match self {
            Self::List(tools) => tools.iter().map(|tool| tool.trim().to_string()).collect(),
            // 括号内的空格/逗号属于 Bash(...) 等权限表达式，不能拆成多个工具。
            Self::Text(text) => {
                let mut tools = Vec::new();
                let mut depth = 0usize;
                let mut current = String::new();
                for character in text.chars() {
                    match character {
                        '(' => depth += 1,
                        ')' => depth = depth.saturating_sub(1),
                        _ => {}
                    }
                    if depth == 0 && (character == ',' || character.is_whitespace()) {
                        if !current.is_empty() {
                            tools.push(std::mem::take(&mut current));
                        }
                    } else {
                        current.push(character);
                    }
                }
                if !current.is_empty() {
                    tools.push(current);
                }
                tools
            }
        }
        .into_iter()
        .filter(|tool| !tool.is_empty())
        .collect()
    }
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum SkillArgumentNames {
    Text(String),
    List(Vec<String>),
}

impl SkillArgumentNames {
    fn to_vec(&self) -> Vec<String> {
        let names = match self {
            Self::Text(text) => text.split_whitespace().map(str::to_string).collect(),
            Self::List(names) => names.clone(),
        };
        names
            .into_iter()
            .filter(|name| !name.trim().is_empty() && !name.chars().all(|c| c.is_ascii_digit()))
            .collect()
    }
}

#[derive(Debug)]
pub struct LoadedSkill {
    pub name: String,
    pub path: PathBuf,
    pub frontmatter: SkillFrontmatter,
    pub instruction: String,
}

/// 仅从发现目录的目录表定位技能；不把模型输入拼成文件路径。
pub fn find_skill<'a>(skills: &'a [SkillMetadata], requested: &str) -> Option<&'a SkillMetadata> {
    let name = requested.trim().strip_prefix('/').unwrap_or(requested.trim());
    if name.is_empty() || name.contains(['/', '\\']) {
        return None;
    }
    if let Some(skill) = skills.iter().find(|skill| skill.name == name) {
        return Some(skill);
    }
    if let Some(skill) = skills.iter().find(|skill| skill.name.eq_ignore_ascii_case(name)) {
        return Some(skill);
    }
    // 无命名空间时允许唯一的插件简称；显式限定名称不能误选另一插件。
    let mut matches = skills.iter().filter(|skill| {
        if let Some((_, basename)) = name.rsplit_once(':') {
            !skill.name.contains(':') && skill.name.eq_ignore_ascii_case(basename)
        } else {
            skill.name.rsplit(':').next().is_some_and(|base| base.eq_ignore_ascii_case(name))
        }
    });
    let matched = matches.next()?;
    matches.next().is_none().then_some(matched)
}

pub fn parse_skill(content: &str) -> Result<(SkillFrontmatter, String), SkillError> {
    let normalized = content.trim_start_matches('\u{feff}').replace("\r\n", "\n");
    let parsed: gray_matter::ParsedEntity = Matter::<YAML>::new()
        .parse(&normalized)
        .map_err(|error| format!("Invalid skill frontmatter: {error}"))?;
    let frontmatter = parsed
        .data
        .map(|data| data.deserialize::<SkillFrontmatter>())
        .transpose()
        .map_err(|error| format!("Invalid skill frontmatter: {error}"))?
        .unwrap_or_else(|| SkillFrontmatter {
            user_invocable: true,
            ..Default::default()
        });
    Ok((frontmatter, parsed.content))
}

pub fn load_skill(dirs: &[PathBuf], requested: &str, args: Option<&str>) -> Result<LoadedSkill, SkillError> {
    let skills = list_skills(dirs);
    let metadata = find_skill(&skills, requested)
        .ok_or_else(|| format!("Unknown or ambiguous skill: {}", requested.trim()))?;
    let content = std::fs::read_to_string(&metadata.path)
        .map_err(|error| format!("Failed to read skill {}: {error}", metadata.name))?;
    let (frontmatter, body) = parse_skill(&content)?;
    let skill_dir = metadata.path.parent().unwrap_or_else(|| Path::new("."));
    let skill_dir = skill_dir.to_string_lossy().replace('\\', "/");
    let names = frontmatter.arguments.as_ref().map(SkillArgumentNames::to_vec).unwrap_or_default();
    // CC 的 SkillTool 将省略的 args 也按空串传给 getPromptForCommand。
    let body = substitute_arguments(&body, args.unwrap_or_default(), &names);
    let body = body.replace("${CLAUDE_SKILL_DIR}", &skill_dir);
    Ok(LoadedSkill {
        name: metadata.name.clone(),
        path: metadata.path.clone(),
        frontmatter,
        instruction: format!("Base directory for this skill: {skill_dir}\n\n{body}"),
    })
}

/// 对齐 CC 的 shell-quote 参数分词：只分词，绝不展开变量或执行命令。
fn parse_arguments(args: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quote = None;
    let mut started = false;
    let mut characters = args.chars().peekable();
    while let Some(character) = characters.next() {
        if character == '\\' && quote != Some('\'') {
            let Some(next) = characters.next() else {
                return args.split_whitespace().map(str::to_string).collect();
            };
            // 双引号内只移除 shell 规定的转义符，Windows 路径中的 \d 等保持原样。
            if quote == Some('"') && !matches!(next, '$' | '`' | '"' | '\\' | '\n') {
                word.push('\\');
            }
            if next != '\n' {
                word.push(next);
            }
            started = true;
        } else if let Some(delimiter) = quote {
            if character == delimiter {
                quote = None;
            } else {
                word.push(character);
            }
        } else if character == '\'' || character == '"' {
            quote = Some(character);
            started = true;
        } else if character == '#' && !started {
            break;
        } else if character.is_whitespace() || "|&;()< >".contains(character) {
            if started {
                words.push(std::mem::take(&mut word));
                started = false;
            }
        } else {
            word.push(character);
            started = true;
        }
    }
    if quote.is_some() {
        return args.split_whitespace().map(str::to_string).collect();
    }
    if started {
        words.push(word);
    }
    words
}

pub fn substitute_arguments(content: &str, args: &str, names: &[String]) -> String {
    let parsed = parse_arguments(args);
    let mut result = content.to_string();
    for (index, name) in names.iter().enumerate() {
        let pattern = Regex::new(&format!(r"\${}", regex::escape(name)))
            .expect("转义后的参数名必须生成有效正则");
        let mut replaced = String::new();
        let mut cursor = 0;
        for matched in pattern.find_iter(&result) {
            if result[matched.end()..].chars().next().is_some_and(|next| {
                next == '[' || next.is_ascii_alphanumeric() || next == '_'
            }) {
                continue;
            }
            replaced.push_str(&result[cursor..matched.start()]);
            replaced.push_str(parsed.get(index).map(String::as_str).unwrap_or_default());
            cursor = matched.end();
        }
        replaced.push_str(&result[cursor..]);
        result = replaced;
    }
    let indexed = Regex::new(r"\$ARGUMENTS\[(\d+)\]").expect("固定正则必须有效");
    result = indexed.replace_all(&result, |capture: &Captures<'_>| {
        capture[1].parse::<usize>().ok().and_then(|index| parsed.get(index)).cloned().unwrap_or_default()
    }).into_owned();
    let shorthand = Regex::new(r"\$(\d+)\b").expect("固定正则必须有效");
    result = shorthand.replace_all(&result, |capture: &Captures<'_>| {
        capture[1].parse::<usize>().ok().and_then(|index| parsed.get(index)).cloned().unwrap_or_default()
    }).into_owned();
    result = result.replace("$ARGUMENTS", args);
    if result == content && !args.is_empty() {
        result.push_str("\n\nARGUMENTS: ");
        result.push_str(args);
    }
    result
}

#[cfg(test)]
#[path = "invocation_test.rs"]
mod tests;
