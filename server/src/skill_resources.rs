//! Agent Skills (`skills/`) exposed as MCP Resources.

use include_dir::{include_dir, Dir, File};
use rmcp::model::Resource;

/// URI prefix for embedded skill files (not an HTTP download URL).
pub const SKILLS_URI_PREFIX: &str = "https://sql-mcp.local/skills/";

static SKILLS_DIR: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/../skills");

pub fn list_skill_resources() -> Vec<Resource> {
    let mut out = Vec::new();
    collect_files(&SKILLS_DIR, "", &mut out);
    out.sort_by(|a, b| a.uri.cmp(&b.uri));
    out
}

pub fn read_skill_resource(uri: &str) -> Option<(&'static str, &'static str)> {
    let rel = uri.strip_prefix(SKILLS_URI_PREFIX)?;
    if rel.is_empty() || rel.contains("..") {
        return None;
    }
    let file = SKILLS_DIR.get_file(rel)?;
    let text = file.contents_utf8()?;
    Some((mime_for_path(rel), text))
}

fn collect_files(dir: &Dir<'_>, prefix: &str, out: &mut Vec<Resource>) {
    for file in dir.files() {
        push_file(file, prefix, out);
    }
    for child in dir.dirs() {
        let name = child.path().file_name().and_then(|s| s.to_str()).unwrap_or("");
        let child_prefix = if prefix.is_empty() {
            name.to_string()
        } else {
            format!("{prefix}/{name}")
        };
        collect_files(child, &child_prefix, out);
    }
}

fn push_file(file: &File<'_>, prefix: &str, out: &mut Vec<Resource>) {
    let name = file.path().file_name().and_then(|s| s.to_str()).unwrap_or("");
    let rel = if prefix.is_empty() {
        name.to_string()
    } else {
        format!("{prefix}/{name}")
    };
    let uri = format!("{SKILLS_URI_PREFIX}{rel}");
    let mime = mime_for_path(&rel);
    let bytes = file.contents();
    out.push(
        Resource::new(uri, rel.clone())
            .with_description(format!("Agent Skill file: {rel}"))
            .with_mime_type(mime)
            .with_size(bytes.len() as u64),
    );
}

fn mime_for_path(path: &str) -> &'static str {
    if path.ends_with(".schema.json") {
        "application/schema+json"
    } else if path.ends_with(".json") {
        "application/json"
    } else if path.ends_with(".md") {
        "text/markdown"
    } else {
        "application/octet-stream"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_three_skill_roots() {
        let list = list_skill_resources();
        assert!(list.len() >= 3, "got {} resources", list.len());
        let uris: Vec<_> = list.iter().map(|r| r.uri.as_str()).collect();
        assert!(uris.iter().any(|u| u.ends_with("semantic-register/SKILL.md")));
        assert!(uris
            .iter()
            .any(|u| u.ends_with("semantic-table-view/SKILL.md")));
        assert!(uris
            .iter()
            .any(|u| u.ends_with("semantic-cube-metric-segment/SKILL.md")));
    }

    #[test]
    fn reads_register_skill_md() {
        let uri = format!("{SKILLS_URI_PREFIX}semantic-register/SKILL.md");
        let (mime, text) = read_skill_resource(&uri).expect("read");
        assert_eq!(mime, "text/markdown");
        assert!(text.contains("name: semantic-register"));
    }

    #[test]
    fn rejects_unknown_and_traversal() {
        assert!(read_skill_resource(&format!("{SKILLS_URI_PREFIX}nope/SKILL.md")).is_none());
        assert!(read_skill_resource(&format!("{SKILLS_URI_PREFIX}../Cargo.toml")).is_none());
    }
}
