use std::sync::Arc;

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

const OVERWRITE_FOLDER: &str = "AI";
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
        description = "List note paths in the vault or in one folder, recursively, skipping dot folders such as `.trash`."
    )]
    async fn list_notes(
        &self,
        Parameters(ListParams { folder }): Parameters<ListParams>,
    ) -> Result<String, String> {
        self.run(move |vault| Ok(vault.files(&folder)?.join("\n")))
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
        description = "Search notes for text, case-insensitively. Returns matching paths and `path:line: text` matches."
    )]
    async fn search_notes(
        &self,
        Parameters(SearchParams { query, limit }): Parameters<SearchParams>,
    ) -> Result<String, String> {
        let limit = limit.unwrap_or(DEFAULT_SEARCH_LIMIT).min(MAX_SEARCH_LIMIT);
        self.run(move |vault| search(vault, &query, limit)).await
    }

    #[tool(description = "Create a new note. Fails if the note already exists.")]
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

    #[tool(description = "Append content to the end of an existing note.")]
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

    #[tool(
        description = "Create or overwrite a note inside the `AI/` folder. Notes elsewhere cannot be overwritten."
    )]
    async fn write_ai_note(
        &self,
        Parameters(WriteParams { path, content }): Parameters<WriteParams>,
    ) -> Result<String, String> {
        if path.split('/').next() != Some(OVERWRITE_FOLDER) {
            return Err(format!(
                "{path}: only notes inside {OVERWRITE_FOLDER}/ can be overwritten"
            ));
        }
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
            .with_instructions(
                "Access to the notes in an Obsidian vault. Notes can be listed, read, searched, created and appended to; \
                 existing notes can only be overwritten inside the AI/ folder.",
            )
    }
}

fn search(vault: &Vault, query: &str, limit: usize) -> vault::Result<String> {
    let query = query.to_lowercase();
    let mut matches = Vec::new();
    for path in vault.files("")? {
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

    #[tokio::test]
    async fn overwrites_only_inside_ai_folder() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("note.md"), "original").unwrap();
        let server = ObsidianServer::new(Arc::new(Vault::open(dir.path()).unwrap()));
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
