use rmcp::{
    ServerHandler,
    model::{Implementation, ServerConfig},
};

#[derive(Clone)]
pub struct ObsidianServer;

impl ServerHandler for ObsidianServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::default()
            .with_server_info(Implementation::new(
                env!("CARGO_PKG_NAME"),
                env!("CARGO_PKG_VERSION"),
            ))
            .with_instructions("Access to the notes in an Obsidian vault.")
    }
}
