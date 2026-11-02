# Install sql-mcp

Full guide: https://github.com/yoosungung/sql-mcp

Release `current` (rebuilt from `master`): https://github.com/yoosungung/sql-mcp/releases/tag/current

You do not need to clone the repository or build with Rust. Download the binary for the OS, or run the container image. Create the semantic DB directory first: `mkdir -p ~/.sql-mcp`. Do not open the same `SQL_MCP_SEMANTIC_PATH` from HTTP and stdio at the same time.

The user keeps the real DSN in the environment. Use the placeholders below until they set it locally. Do not put the real URL in a chat, a commit, or this file.

## Binary

Asset names:

- macOS Apple Silicon: `sql-mcp-aarch64-apple-darwin`
- Linux x86_64: `sql-mcp-x86_64-unknown-linux-gnu`
- Windows x86_64: `sql-mcp-x86_64-pc-windows-msvc.exe`

Download the asset from the Release page, move it to a directory on `PATH` (for example `~/bin/sql-mcp`), and make it executable.

Minimal `config.toml`:

```toml
listen = "127.0.0.1:8080"
max_rows = 1000
sample_limit = 20
timeout_ms = 30000

[[targets]]
id = "dw"
name = "Data Warehouse"
engine = "postgres"          # or "sqlite"
url_env = "SQL_MCP_TARGET_DW_URL"
```

```bash
export SQL_MCP_SEMANTIC_PATH="$HOME/.sql-mcp/semantic.db"
export SQL_MCP_CONFIG="$HOME/bin/config.toml"
export SQL_MCP_TARGET_DW_URL='postgres://USER:PASS@HOST:5432/DB'

~/bin/sql-mcp --help
~/bin/sql-mcp --stdio
```

`~/bin/sql-mcp` with no args serves HTTP. MCP endpoint: `POST http://127.0.0.1:8080/mcp`. Authoring API: `/api`. Health: `GET /health`.

If startup fails, stderr prints `sql-mcp: …` plus the config path, database path, and a hint.

## Container

Mount `config.toml` and a volume for the semantic DB. Replace `<release-image>` with the image from the Release.

```bash
docker run --rm -p 8080:8080 \
  -e SQL_MCP_SEMANTIC_PATH=/data/semantic.db \
  -e SQL_MCP_CONFIG=/config/config.toml \
  -e SQL_MCP_TARGET_DW_URL \
  -v "$HOME/sql-mcp/config.toml:/config/config.toml:ro" \
  -v sqlmcp-data:/data \
  <release-image>
```

## Connect Cursor

`~/.cursor/mcp.json` or the project `.cursor/mcp.json`:

```json
{
  "mcpServers": {
    "sql-mcp": {
      "type": "stdio",
      "command": "/Users/YOU/bin/sql-mcp",
      "args": ["--stdio"],
      "env": {
        "SQL_MCP_SEMANTIC_PATH": "/Users/YOU/.sql-mcp/semantic.db",
        "SQL_MCP_CONFIG": "/Users/YOU/bin/config.toml",
        "SQL_MCP_TARGET_DW_URL": "postgres://USER:PASS@HOST:5432/DB"
      }
    }
  }
}
```

Replace the paths. Reload MCP. If the server is already on HTTP, set `url` to `http://127.0.0.1:8080/mcp` instead of `command`.

## Connect Claude Desktop

Add the same `mcpServers` entry to `claude_desktop_config.json`.

- macOS: `~/Library/Application Support/Claude/claude_desktop_config.json`
- Windows: `%APPDATA%\Claude\claude_desktop_config.json`

Quit the app completely, then start it again.

## Connect Claude Code

Register the same stdio command, or the HTTP URL, in the project `.mcp.json`, or with `claude mcp add`.

## Metadata, then questions

Read the physical catalog with `GET /api/databases/{id}/tables`. Register JSON in this order with `POST /api/meta`:

```text
table → view → cube → metric | segment
```

The server exposes the registration schema and authoring skills as MCP Resources. After cubes exist, ask questions with `search_columns`, `sample_column`, and `execute_sql`. Example prompts:

- From sales, get last month's revenue metric by region.
- Find columns related to status and show a sample.
