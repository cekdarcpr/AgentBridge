use std::{fs, path::PathBuf};

use agentbridge_core::{project, AppError};
use serde::Serialize;
use serde_json::json;

use crate::{output, AgentAction};

/// docs/agent-instructions.md is the single source; everything above its `---` line is for people.
const DOC: &str = include_str!("../../../docs/agent-instructions.md");
const DOC_SEPARATOR: &str = "\n---\n";
/// Marks the block so running `agb agent init` again replaces it instead of adding a copy.
const START_MARKER: &str = "<!-- agentbridge:start -->";
const END_MARKER: &str = "<!-- agentbridge:end -->";
/// The instruction files agents read: Claude Code reads CLAUDE.md, Codex, Cursor and most others AGENTS.md.
const KNOWN_FILES: [&str; 2] = ["CLAUDE.md", "AGENTS.md"];
const NEW_FILE: &str = "AGENTS.md";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
enum Change {
    Created,
    Added,
    Updated,
    Unchanged,
}

impl Change {
    fn as_str(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Added => "added",
            Self::Updated => "updated",
            Self::Unchanged => "unchanged",
        }
    }
}

pub fn command(action: AgentAction, json: bool) -> Result<(), AppError> {
    match action {
        AgentAction::Instructions => {
            if json {
                output::print_ok(&json!({"text": instructions()}));
            } else {
                print!("{}", instructions());
            }
        }
        AgentAction::Init { files } => {
            let mut results = Vec::new();
            for path in targets(files) {
                let existing = match fs::read_to_string(&path) {
                    Ok(text) => Some(text),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                    Err(e) => return Err(e.into()),
                };
                let (text, change) = merge(existing.as_deref(), &block());
                if change != Change::Unchanged {
                    project::write_file(&path, text.as_bytes())?;
                }
                results.push((project::display_path(&path), change));
            }
            if json {
                let files: Vec<_> = results
                    .iter()
                    .map(|(file, change)| json!({"file": file, "change": change}))
                    .collect();
                output::print_ok(&json!({"files": files}));
            } else {
                for (file, change) in &results {
                    println!("{file}: {}", change.as_str());
                }
            }
        }
    }
    Ok(())
}

/// Line endings are normalized: a Windows checkout embeds the file with CRLF.
pub fn instructions() -> String {
    let doc = DOC.replace("\r\n", "\n");
    let text = doc
        .split_once(DOC_SEPARATOR)
        .map_or(doc.as_str(), |(_, text)| text);
    text.trim_start().to_string()
}

fn block() -> String {
    format!(
        "{START_MARKER}\n{}\n{END_MARKER}\n",
        instructions().trim_end()
    )
}

/// Explicit files, otherwise the instruction files the project already has, otherwise a new AGENTS.md.
fn targets(files: Vec<PathBuf>) -> Vec<PathBuf> {
    let root = project::root();
    if !files.is_empty() {
        return files
            .into_iter()
            .map(|f| if f.is_absolute() { f } else { root.join(f) })
            .collect();
    }
    let existing: Vec<_> = KNOWN_FILES
        .iter()
        .map(|name| root.join(name))
        .filter(|path| path.exists())
        .collect();
    if existing.is_empty() {
        vec![root.join(NEW_FILE)]
    } else {
        existing
    }
}

fn merge(existing: Option<&str>, block: &str) -> (String, Change) {
    let Some(text) = existing else {
        return (block.to_string(), Change::Created);
    };
    let markers = text
        .find(START_MARKER)
        .zip(text.find(END_MARKER))
        .filter(|(start, end)| start < end);
    if let Some((start, end)) = markers {
        let mut end = end + END_MARKER.len();
        if text[end..].starts_with("\r\n") {
            end += 2;
        } else if text[end..].starts_with('\n') {
            end += 1;
        }
        if &text[start..end] == block {
            return (text.to_string(), Change::Unchanged);
        }
        let merged = format!("{}{block}{}", &text[..start], &text[end..]);
        return (merged, Change::Updated);
    }
    let separator = match text {
        "" => "",
        t if t.ends_with("\n\n") => "",
        t if t.ends_with('\n') => "\n",
        _ => "\n\n",
    };
    (format!("{text}{separator}{block}"), Change::Added)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instructions_skip_the_note_for_people() {
        assert!(instructions().starts_with("## Creating assets with `agb`"));
        assert!(!instructions().contains("Copy everything below the line"));
    }

    #[test]
    fn creates_appends_and_replaces_the_block() {
        let block = block();
        assert_eq!(merge(None, &block), (block.clone(), Change::Created));

        let (added, change) = merge(Some("# My project\n"), &block);
        assert_eq!(change, Change::Added);
        assert_eq!(added, format!("# My project\n\n{block}"));

        assert_eq!(
            merge(Some(&added), &block),
            (added.clone(), Change::Unchanged)
        );

        let stale = format!("# My project\n\n{START_MARKER}\nold\n{END_MARKER}\n\n## Notes\n");
        let (updated, change) = merge(Some(&stale), &block);
        assert_eq!(change, Change::Updated);
        assert_eq!(updated, format!("# My project\n\n{block}\n## Notes\n"));
    }
}
