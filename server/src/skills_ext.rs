//! MCP Skills extension (`io.modelcontextprotocol/skills`, SEP-2640).
//! Serves `skills/list` and `skills/get` over the same skill files as Resources.

use include_dir::{Dir, File};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::skill_resources::{SKILLS_DIR, SKILLS_URI_PREFIX};

pub const SKILLS_EXTENSION_ID: &str = "io.modelcontextprotocol/skills";

/// Default listing / get cache hint (5 minutes).
const TTL_MS: u64 = 300_000;

/// Capability settings object (no `directoryRead` — we do not implement directory read).
pub fn skills_extension_settings() -> serde_json::Map<String, Value> {
    serde_json::Map::new()
}

pub fn list_skills_result() -> Result<Value, String> {
    let skills = all_skill_entries()?;
    Ok(json!({
        "resultType": "complete",
        "skills": skills,
        "ttlMs": TTL_MS,
        "cacheScope": "public",
    }))
}

pub fn get_skill_result(uri: &str) -> Result<Value, String> {
    let skill = skill_entry_by_uri(uri).ok_or_else(|| format!("unknown skill uri: {uri}"))?;
    Ok(json!({
        "resultType": "complete",
        "skill": skill,
        "ttlMs": TTL_MS,
        "cacheScope": "public",
    }))
}

fn all_skill_entries() -> Result<Vec<Value>, String> {
    let mut names: Vec<String> = Vec::new();
    collect_skill_root_names(&SKILLS_DIR, "", &mut names);
    names.sort();
    names.dedup();
    names
        .into_iter()
        .map(|name| skill_entry_for_root(&name))
        .collect()
}

/// Top-level dirs that contain `SKILL.md` (skill roots).
fn collect_skill_root_names(dir: &Dir<'_>, prefix: &str, out: &mut Vec<String>) {
    let has_skill_md = dir
        .files()
        .any(|f| f.path().file_name().and_then(|s| s.to_str()) == Some("SKILL.md"));
    if has_skill_md && !prefix.is_empty() && !prefix.contains('/') {
        out.push(prefix.to_string());
        return;
    }
    for child in dir.dirs() {
        let name = child
            .path()
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        let child_prefix = if prefix.is_empty() {
            name.to_string()
        } else {
            format!("{prefix}/{name}")
        };
        collect_skill_root_names(child, &child_prefix, out);
    }
}

fn skill_entry_by_uri(uri: &str) -> Option<Value> {
    let skill_md = uri.strip_suffix("/SKILL.md")?;
    let rel_root = skill_md.strip_prefix(SKILLS_URI_PREFIX)?;
    if rel_root.is_empty() || rel_root.contains("..") || rel_root.contains('/') {
        // Only top-level skill dirs for now (no nested skills).
        return None;
    }
    skill_entry_for_root(rel_root).ok()
}

fn skill_entry_for_root(skill_name: &str) -> Result<Value, String> {
    let dir = SKILLS_DIR
        .get_dir(skill_name)
        .ok_or_else(|| format!("skill dir missing: {skill_name}"))?;
    let skill_md = dir
        .files()
        .find(|f| f.path().file_name().and_then(|s| s.to_str()) == Some("SKILL.md"))
        .ok_or_else(|| format!("SKILL.md missing: {skill_name}"))?;
    let text = skill_md
        .contents_utf8()
        .ok_or_else(|| format!("SKILL.md not utf8: {skill_name}"))?;
    let frontmatter = parse_frontmatter(text)?;
    let name = frontmatter
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("frontmatter.name missing: {skill_name}"))?;
    if name != skill_name {
        return Err(format!("skill dir {skill_name} != frontmatter.name {name}"));
    }
    if frontmatter
        .get("description")
        .and_then(|v| v.as_str())
        .is_none()
    {
        return Err(format!("frontmatter.description missing: {skill_name}"));
    }

    let uri = format!("{SKILLS_URI_PREFIX}{skill_name}/SKILL.md");
    let mut resources = Vec::new();
    collect_skill_resources(dir, skill_name, "", &mut resources);
    resources.sort_by(|a, b| {
        a.get("uri")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .cmp(b.get("uri").and_then(|v| v.as_str()).unwrap_or(""))
    });

    Ok(json!({
        "uri": uri,
        "frontmatter": frontmatter,
        "resources": resources,
    }))
}

fn collect_skill_resources(dir: &Dir<'_>, skill_name: &str, prefix: &str, out: &mut Vec<Value>) {
    for file in dir.files() {
        push_skill_resource(file, skill_name, prefix, out);
    }
    for child in dir.dirs() {
        let name = child
            .path()
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        let child_prefix = if prefix.is_empty() {
            name.to_string()
        } else {
            format!("{prefix}/{name}")
        };
        collect_skill_resources(child, skill_name, &child_prefix, out);
    }
}

fn push_skill_resource(file: &File<'_>, skill_name: &str, prefix: &str, out: &mut Vec<Value>) {
    let name = file
        .path()
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    let rel = if prefix.is_empty() {
        name.to_string()
    } else {
        format!("{prefix}/{name}")
    };
    let uri = format!("{SKILLS_URI_PREFIX}{skill_name}/{rel}");
    let bytes = file.contents();
    out.push(json!({
        "uri": uri,
        "digest": sha256_digest(bytes),
        "size": bytes.len() as u64,
    }));
}

fn sha256_digest(bytes: &[u8]) -> String {
    let hash = Sha256::digest(bytes);
    let mut out = String::from("sha256:");
    for b in hash {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

fn parse_frontmatter(skill_md: &str) -> Result<Value, String> {
    let trimmed = skill_md.trim_start_matches('\u{feff}');
    let rest = trimmed
        .strip_prefix("---\n")
        .or_else(|| trimmed.strip_prefix("---\r\n"))
        .ok_or_else(|| "SKILL.md missing opening frontmatter ---".to_string())?;
    let end = rest
        .find("\n---\n")
        .or_else(|| rest.find("\n---\r\n"))
        .or_else(|| rest.find("\r\n---\r\n"))
        .ok_or_else(|| "SKILL.md missing closing frontmatter ---".to_string())?;
    let yaml = &rest[..end];
    let value: Value = serde_yaml::from_str(yaml).map_err(|e| format!("frontmatter yaml: {e}"))?;
    if !value.is_object() {
        return Err("frontmatter must be a mapping".into());
    }
    Ok(value)
}

/// Handle `skills/list` / `skills/get` custom methods. Returns `None` if not a skills method.
pub fn handle_skills_custom(method: &str, params: Option<&Value>) -> Option<Result<Value, String>> {
    match method {
        "skills/list" => Some(list_skills_result()),
        "skills/get" => {
            let uri = params
                .and_then(|p| p.get("uri"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if uri.is_empty() {
                return Some(Err("skills/get requires params.uri".into()));
            }
            Some(get_skill_result(uri))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_skills_with_digests() {
        let result = list_skills_result().unwrap();
        assert_eq!(result["resultType"], "complete");
        assert_eq!(result["cacheScope"], "public");
        let skills = result["skills"].as_array().unwrap();
        assert_eq!(skills.len(), 5);
        let names: std::collections::BTreeMap<_, _> = skills
            .iter()
            .map(|s| (s["frontmatter"]["name"].as_str().unwrap().to_string(), s))
            .collect();
        assert!(names.contains_key("semantic-register"));
        assert!(names.contains_key("semantic-table-view"));
        assert!(names.contains_key("semantic-cube-metric-segment"));
        assert!(names.contains_key("semantic-from-qa"));
        assert!(names.contains_key("semantic-from-eval"));

        let reg = names["semantic-register"];
        assert_eq!(
            reg["uri"],
            "https://sql-mcp.local/skills/semantic-register/SKILL.md"
        );
        let resources = reg["resources"].as_array().unwrap();
        assert!(resources.len() >= 2);
        let skill_md = resources
            .iter()
            .find(|r| r["uri"] == reg["uri"])
            .expect("SKILL.md in resources");
        let digest = skill_md["digest"].as_str().unwrap();
        assert!(digest.starts_with("sha256:"));
        assert_eq!(digest.len(), "sha256:".len() + 64);
        assert!(skill_md["size"].as_u64().unwrap() > 0);
    }

    #[test]
    fn get_by_uri() {
        let uri = "https://sql-mcp.local/skills/semantic-register/SKILL.md";
        let result = get_skill_result(uri).unwrap();
        assert_eq!(result["skill"]["frontmatter"]["name"], "semantic-register");
        assert!(get_skill_result("https://sql-mcp.local/skills/nope/SKILL.md").is_err());
    }

    #[test]
    fn handle_dispatch() {
        let list = handle_skills_custom("skills/list", None).unwrap().unwrap();
        assert_eq!(list["skills"].as_array().unwrap().len(), 5);
        let get = handle_skills_custom(
            "skills/get",
            Some(&json!({"uri": "https://sql-mcp.local/skills/semantic-table-view/SKILL.md"})),
        )
        .unwrap()
        .unwrap();
        assert_eq!(get["skill"]["frontmatter"]["name"], "semantic-table-view");
        assert!(handle_skills_custom("tools/list", None).is_none());
    }
}
