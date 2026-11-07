# Obsidian MCP
MCP server giving Claude read and write access to an [Obsidian Sync](https://obsidian.md/sync) vault, for use as a claude.ai custom connector.

## How it works
- **Sync client** keeps a local copy of the vault up to date using [Obsidian Headless](https://github.com/obsidianmd/obsidian-headless)
- **MCP server** exposes that copy to Claude over HTTP, authenticating requests with OAuth
- Both run side by side, sharing the vault directory; deployment manifests live in [apps](https://github.com/VWongDev/apps)

## Capabilities
- List, read and search notes
- Resolve links and find broken ones
- Create, append to and overwrite notes inside a writable folder, `000 Inbox/AI/` by default (`WRITE_FOLDER`)

Notes cannot be deleted or moved, and Obsidian's config directory is off limits.

## Configuration
Both images are configured through environment variables.

| Image | Required |
|---|---|
| Server ([`Dockerfile`](Dockerfile)) | `PUBLIC_URL`, `ISSUER_URL`, `AUDIENCE`, `ALLOWED_SUBJECTS` |
| Sync client ([`sync/`](sync)) | `OBSIDIAN_AUTH_TOKEN`, `VAULT_NAME` |

Get an Obsidian auth token by running `npx obsidian-headless login` once locally.

## Development
```sh
direnv allow  # Install tools with Nix
cargo test
```
