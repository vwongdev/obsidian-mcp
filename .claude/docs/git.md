# Git Commit Naming Conventions

This repository follows conventional commit message format to maintain clear, searchable commit history.

## Format

```
<type>(<scope>): <subject>

<body>

<footer>
```

## Commit Types

- **feat** - New feature or functionality
- **fix** - Bug fix
- **docs** - Documentation changes
- **build** - Cargo build configuration, toolchain and container image changes
- **chore** - Maintenance tasks, dependency updates
- **ci** - CI/CD workflow changes
- **ai** - AI agent documentation or tooling changes

## Scope

Scope indicates the area of the codebase affected. Multiple scopes are comma-separated in alphabetical order:

- **server** - MCP transport and HTTP serving
- **auth** - OAuth resource server and token validation
- **vault** - Confined access to the vault directory
- **tools** - MCP tools exposed to clients
- **image** - Container images
- **deps** - Dependency updates
- **global** - Repository-wide changes

## Examples

```
feat(server): Serve MCP over Streamable HTTP
feat(auth): Authenticate requests as OAuth resource server
build(image): Add server and sync container images
chore(deps): Configure Renovate
```

## Subject Line Rules

- Use imperative mood ("Add" not "added" or "adds")
- First letter capitalized
- No period at the end
- Maximum 72 characters
- Be specific and descriptive

## Body (Optional)

Use body for:
- Explaining what and why (not how)
- Reference to issues/PRs

## Footer (Optional)

Use footer for:
- Issue references: `Closes #123`, `Fixes #456`
