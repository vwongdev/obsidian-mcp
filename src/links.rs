/// Extracts link targets from a note's wikilinks, embeds and Markdown links, skipping fenced code
/// blocks and external URLs. Headings, block references and aliases are stripped from each target.
pub fn extract(content: &str) -> Vec<String> {
    let mut targets = Vec::new();
    let mut in_fence = false;
    for line in content.lines() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        targets.extend(delimited(line, "[[", "]]").map(|inner| strip(inner).to_string()));
        targets.extend(
            delimited(line, "](", ")")
                .map(|inner| inner.split(' ').next().unwrap_or_default())
                .filter(|target| !target.contains(':') && !target.starts_with('#'))
                .map(|target| strip(target).replace("%20", " ")),
        );
    }
    targets
}

/// Resolves a link target the way Obsidian does: a path from the vault root, then a path relative
/// to the linking note, then the shortest path ending in the target. Matching ignores case, and a
/// missing `.md` extension is implied.
pub fn resolve<'a>(target: &str, source: &'a str, files: &'a [String]) -> Option<&'a str> {
    if target.is_empty() {
        return Some(source);
    }
    let folder = source.rsplit_once('/').map_or("", |(folder, _)| folder);
    let candidates = [target.to_lowercase(), format!("{target}.md").to_lowercase()];
    files
        .iter()
        .filter_map(|file| {
            let file_lower = file.to_lowercase();
            candidates.iter().find_map(|candidate| {
                let rank = if file_lower == *candidate {
                    0
                } else if !folder.is_empty()
                    && file_lower == format!("{folder}/{candidate}").to_lowercase()
                {
                    1
                } else if file_lower.ends_with(&format!("/{candidate}")) {
                    2
                } else {
                    return None;
                };
                Some((rank, file.len(), file.as_str()))
            })
        })
        .min()
        .map(|(_, _, file)| file)
}

fn delimited<'a>(line: &'a str, open: &'a str, close: &'a str) -> impl Iterator<Item = &'a str> {
    let mut rest = line;
    std::iter::from_fn(move || {
        let start = rest.find(open)? + open.len();
        let length = rest[start..].find(close)?;
        let inner = &rest[start..start + length];
        rest = &rest[start + length + close.len()..];
        Some(inner)
    })
}

fn strip(target: &str) -> &str {
    let end = target.find(['#', '|', '^']).unwrap_or(target.len());
    target[..end].trim()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_links() {
        let content = "\
See [[Plan]], [[Projects/Goals#Q3|goals]] and ![[diagram.png]].
Also [notes](Meeting%20Notes.md \"title\"), [site](https://example.com), [top](#heading).
```
[[Not a link]]
```
[[#Local heading]]";

        assert_eq!(
            extract(content),
            [
                "Plan",
                "Projects/Goals",
                "diagram.png",
                "Meeting Notes.md",
                ""
            ]
        );
    }

    #[test]
    fn resolves_like_obsidian() {
        let files = [
            "Plan.md",
            "Archive/Old/Plan.md",
            "Projects/Plan.md",
            "Projects/Goals.md",
            "Archive/Goals.md",
            "assets/diagram.png",
        ]
        .map(String::from);
        let resolve = |target| resolve(target, "Projects/Index.md", &files);

        assert_eq!(resolve("plan"), Some("Plan.md"));
        assert_eq!(resolve("Goals"), Some("Projects/Goals.md"));
        assert_eq!(resolve("Old/Plan"), Some("Archive/Old/Plan.md"));
        assert_eq!(resolve("diagram.png"), Some("assets/diagram.png"));
        assert_eq!(resolve(""), Some("Projects/Index.md"));
        assert_eq!(resolve("Missing"), None);
    }
}
