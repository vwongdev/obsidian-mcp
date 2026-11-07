# Obsidian MCP
MCP server with read and write access to an [Obsidian Sync](https://obsidian.md/sync) vault, for use as a claude.ai custom connector.

The server runs beside an [Obsidian Headless](https://github.com/obsidianmd/obsidian-headless) sync client, sharing the vault directory. Kubernetes manifests live in [apps](https://github.com/VWongDev/apps).

## Images
| Image | Source | Purpose |
|---|---|---|
| `obsidian-mcp/mcp` | [`Dockerfile`](Dockerfile) | MCP server over Streamable HTTP, acting as OAuth resource server |
| `obsidian-mcp/sync` | [`sync/`](sync) | Obsidian Sync client (`ob sync --continuous`) |

Both run as UID `65532` and work with a read-only root filesystem. Pushes to `main` publish both to ECR and Artifact Registry, tagged with the commit SHA.

## Tools
| Tool | Effect |
|---|---|
| `list_notes` | List note paths in the vault or one folder |
| `read_note` | Read a note |
| `search_notes` | Case-insensitive search of note paths and contents |
| `create_note` | Create a note; fails if it exists |
| `append_note` | Append to an existing note |
| `write_ai_note` | Create or overwrite a note inside `AI/` |

Notes cannot be deleted or moved. Paths are relative to the vault root; `..`, symlinks and the `.obsidian/` config directory are rejected.

## Configuration
### Server
| Variable | Default | Description |
|---|---|---|
| `PUBLIC_URL` | required | Public URL of the server, such as `https://obsidian.gcp.vwong.dev`; the OAuth resource identifier and the only accepted `Host` |
| `ISSUER_URL` | required | Identity provider issuer, exactly as in its tokens (Auth0 includes a trailing slash) |
| `AUDIENCE` | required | Expected access token audience; the Auth0 API identifier |
| `ALLOWED_SUBJECTS` | required | Comma-separated token subjects allowed access |
| `VAULT_PATH` | `/vault` | Vault directory |
| `LISTEN_ADDR` | `0.0.0.0:8080` | Listen address |
| `RUST_LOG` | `info` | Log filter |

The server fetches signing keys from the issuer's `/.well-known/openid-configuration` or `/.well-known/oauth-authorization-server` at startup, and serves:
- `/`: MCP endpoint, requiring a bearer token
- `/.well-known/oauth-protected-resource`: protected resource metadata pointing clients at the issuer
- `/healthz`: unauthenticated health check

Signing keys must declare their algorithm (`alg`) in the JWKS; the token header's algorithm is never trusted.

### Sync client
| Variable | Default | Description |
|---|---|---|
| `OBSIDIAN_AUTH_TOKEN` | required | Obsidian account token |
| `VAULT_NAME` | required | Remote vault name or ID |
| `VAULT_PASSWORD` | none | End-to-end encryption password |
| `DEVICE_NAME` | `obsidian-mcp` | Device name in the vault's sync history |
| `VAULT_PATH` | `/vault` | Vault directory |
| `XDG_CONFIG_HOME` | `/config` | Writable directory for sync state; persist it to avoid a full resync on restart |

On first start the client links the vault, then on every start it disables config syncing so `.obsidian/` stays out of the vault.

Get a token by logging in once locally, then reading it from the headless client's config directory:
```sh
npx obsidian-headless login
cat ~/.obsidian-headless/auth_token  # macOS; ~/.config/obsidian-headless/auth_token on Linux
```

## Development
The tools are installed automatically with [Nix](https://nixos.org/download) and [direnv](https://direnv.net):
```sh
direnv allow
```

Run the tests:
```sh
cargo test
```

Build the images locally, such as for `k3d image import`:
```sh
docker build -t obsidian-mcp/mcp:dev .
docker build -t obsidian-mcp/sync:dev sync
```
