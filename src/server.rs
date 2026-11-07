use std::sync::Arc;

use chrono::{DateTime, SecondsFormat, Utc};
use rmcp::{
    ServerHandler,
    handler::server::wrapper::Parameters,
    model::{Implementation, ServerCapabilities, ServerConfig},
    schemars::{self, JsonSchema},
    tool, tool_handler, tool_router,
};
use serde::Deserialize;
use tokio::task;

use crate::vault::{self, Vault};

const DEFAULT_SEARCH_LIMIT: usize = 50;
const MAX_SEARCH_LIMIT: usize = 200;

#[derive(Clone)]
pub struct ObsidianServer {
    vault: Arc<Vault>,
}

#[derive(Deserialize, JsonSchema)]
pub struct ListParams {
    /// Folder relative to the vault root; omit to list the whole vault.
    #[serde(default)]
    folder: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct PathParams {
    /// Note path relative to the vault root, such as `Projects/Plan.md`.
    path: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct PathsParams {
    /// Note paths relative to the vault root, such as `Projects/Plan.md`.
    paths: Vec<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct SearchParams {
    /// Case-insensitive text to find in note paths and contents.
    query: String,
    /// Maximum number of matches to return.
    limit: Option<usize>,
}

#[derive(Deserialize, JsonSchema)]
pub struct WriteParams {
    /// Note path relative to the vault root, such as `Projects/Plan.md`.
    path: String,
    /// Markdown content to write.
    content: String,
}

#[tool_router]
impl ObsidianServer {
    pub fn new(vault: Arc<Vault>) -> Self {
        Self { vault }
    }

    #[tool(
        description = "List note paths in the vault or in one folder, recursively, skipping dot folders such as `.trash`, each with its last modification time in UTC."
    )]
    async fn list_notes(
        &self,
        Parameters(ListParams { folder }): Parameters<ListParams>,
    ) -> Result<String, String> {
        self.run(move |vault| {
            let lines: Vec<_> = vault
                .files(&folder)?
                .into_iter()
                .map(|file| {
                    let modified = DateTime::<Utc>::from(file.modified)
                        .to_rfc3339_opts(SecondsFormat::Secs, true);
                    format!("{}\t{modified}", file.path)
                })
                .collect();
            Ok(lines.join("\n"))
        })
        .await
    }

    #[tool(description = "Read the content of a note.")]
    async fn read_note(
        &self,
        Parameters(PathParams { path }): Parameters<PathParams>,
    ) -> Result<String, String> {
        self.run(move |vault| vault.read(&path)).await
    }

    #[tool(
        description = "Read several notes at once. Each note is headed `==> path <==`; a note that cannot be read shows its error in place of content."
    )]
    async fn read_notes(
        &self,
        Parameters(PathsParams { paths }): Parameters<PathsParams>,
    ) -> Result<String, String> {
        self.run(move |vault| Ok(read_all(vault, &paths))).await
    }

    #[tool(
        description = "Search notes for text, case-insensitively. Returns matching paths and `path:line: text` matches."
    )]
    async fn search_notes(
        &self,
        Parameters(SearchParams { query, limit }): Parameters<SearchParams>,
    ) -> Result<String, String> {
        let limit = limit.unwrap_or(DEFAULT_SEARCH_LIMIT).min(MAX_SEARCH_LIMIT);
        self.run(move |vault| search(vault, &query, limit)).await
    }

    #[tool(
        description = "Create a new note inside the writable folder. Fails if the note already exists."
    )]
    async fn create_note(
        &self,
        Parameters(WriteParams { path, content }): Parameters<WriteParams>,
    ) -> Result<String, String> {
        self.run(move |vault| {
            vault.create(&path, &content)?;
            Ok(format!("Created {path}"))
        })
        .await
    }

    #[tool(
        description = "Append content to the end of an existing note inside the writable folder."
    )]
    async fn append_note(
        &self,
        Parameters(WriteParams { path, content }): Parameters<WriteParams>,
    ) -> Result<String, String> {
        self.run(move |vault| {
            vault.append(&path, &content)?;
            Ok(format!("Appended to {path}"))
        })
        .await
    }

    #[tool(description = "Create or overwrite a note inside the writable folder.")]
    async fn write_ai_note(
        &self,
        Parameters(WriteParams { path, content }): Parameters<WriteParams>,
    ) -> Result<String, String> {
        self.run(move |vault| {
            vault.replace(&path, &content)?;
            Ok(format!("Wrote {path}"))
        })
        .await
    }

    async fn run<T: Send + 'static>(
        &self,
        operation: impl FnOnce(&Vault) -> vault::Result<T> + Send + 'static,
    ) -> Result<T, String> {
        let vault = self.vault.clone();
        task::spawn_blocking(move || operation(&vault))
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())
    }
}

#[tool_handler]
impl ServerHandler for ObsidianServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new(
                env!("CARGO_PKG_NAME"),
                env!("CARGO_PKG_VERSION"),
            ))
            .with_instructions(format!(
                "Access to the notes in an Obsidian vault. Notes can be listed and read anywhere, \
                 but only created, appended to and overwritten inside the writable folder, {}/.",
                self.vault.write_folder().display()
            ))
    }
}

fn read_all(vault: &Vault, paths: &[String]) -> String {
    let notes: Vec<_> = paths
        .iter()
        .map(|path| {
            let content = vault.read(path).unwrap_or_else(|e| format!("error: {e}"));
            format!("==> {path} <==\n{content}")
        })
        .collect();
    notes.join("\n\n")
}

fn search(vault: &Vault, query: &str, limit: usize) -> vault::Result<String> {
    let query = query.to_lowercase();
    let mut matches = Vec::new();
    for vault::File { path, .. } in vault.files("")? {
        if matches.len() >= limit {
            break;
        }
        if path.to_lowercase().contains(&query) {
            matches.push(path.clone());
        }
        if !path.ends_with(".md") {
            continue;
        }
        let content = vault.read(&path)?;
        let lines = content
            .lines()
            .enumerate()
            .filter(|(_, line)| line.to_lowercase().contains(&query))
            .map(|(number, line)| format!("{path}:{}: {}", number + 1, line.trim()));
        matches.extend(lines);
    }
    matches.truncate(limit);
    Ok(matches.join("\n"))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    #[test]
    fn reads_notes_past_failures() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("a.md"), "a").unwrap();
        fs::write(dir.path().join("b.md"), "b").unwrap();
        let vault = Vault::open(dir.path(), "AI").unwrap();
        let paths = ["a.md", "missing.md", "b.md"].map(String::from);

        let output = read_all(&vault, &paths);

        assert!(output.starts_with("==> a.md <==\na\n\n==> missing.md <==\nerror: "));
        assert!(output.ends_with("\n\n==> b.md <==\nb"));
    }

    #[tokio::test]
    async fn overwrites_only_inside_write_folder() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("note.md"), "original").unwrap();
        let server = ObsidianServer::new(Arc::new(Vault::open(dir.path(), "AI").unwrap()));
        let write = |path: &str| {
            Parameters(WriteParams {
                path: path.into(),
                content: "new".into(),
            })
        };

        assert!(server.write_ai_note(write("note.md")).await.is_err());
        assert!(server.write_ai_note(write("AI/note.md")).await.is_ok());
        assert_eq!(
            fs::read_to_string(dir.path().join("note.md")).unwrap(),
            "original"
        );
    }
}
