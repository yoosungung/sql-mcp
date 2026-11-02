# sql-mcp

[한국어](README.ko.md)

An MCP server that lets AI coding agents (Cursor, Claude, and others) query a **data warehouse (DW)** through a semantic layer.

Handing over the physical schema of a warehouse with tens or hundreds of tables makes it easy for an agent to get lost. sql-mcp registers table → view → cube → metric/segment metadata, then compiles the agent's **semantic SQL** into a physical SELECT and runs it. It helps most when the warehouse has **roughly 100 tables or more**.

- Attach it as an MCP server in Cursor, Claude Desktop, or Claude Code and ask questions about the warehouse
- Register metadata (JSON) with an agent, and query by cube and metric
- Targets: **Postgres** or **SQLite** (read-only SELECT)

Do not put passwords or DSNs in chat. Inject them only as environment variables at startup.

## At a glance

```text
1. Get sql-mcp from a Release (binary or image)
2. Set config.toml, the DW DSN, and the semantic DB path
3. Connect it in Cursor or Claude MCP
4. Register metadata with an agent (the server exposes skill and schema Resources)
5. Query with search_columns / sample_column / execute_sql
```

## 1. Get it

Download the `sql-mcp` **binary** or **container image** for your OS from a Release. You do not need to clone the repository or build with Rust.

### Binary

Download the file for your OS from [Release `current`](https://github.com/yoosungung/sql-mcp/releases/tag/current) (updated on every `master` build).

```bash
# macOS Apple Silicon
curl -L -o sql-mcp \
  https://github.com/yoosungung/sql-mcp/releases/download/current/sql-mcp-aarch64-apple-darwin
# Linux x86_64: sql-mcp-x86_64-unknown-linux-gnu
# Windows x86_64: sql-mcp-x86_64-pc-windows-msvc.exe
chmod +x sql-mcp
```

1. Put the file somewhere on your PATH (for example `~/bin/sql-mcp`).
2. Make it executable: `chmod +x ~/bin/sql-mcp`
3. Create `config.toml` in the same folder, or at any path you choose.

```toml
# config.toml — minimal example
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

# Options and environment variables
~/bin/sql-mcp --help

# For a local agent (recommended for Cursor / Claude Desktop)
~/bin/sql-mcp --stdio

# Or HTTP (remote or shared). MCP: POST http://127.0.0.1:8080/mcp
~/bin/sql-mcp
```

If startup fails, stderr prints `sql-mcp: …` plus the config path, database path, and a hint (`SQL_MCP_CONFIG`, `SQL_MCP_SEMANTIC_PATH`, the listen port, and so on).

Create the semantic DB directory first (`mkdir -p ~/.sql-mcp`). Do not open the same `SQL_MCP_SEMANTIC_PATH` from HTTP and stdio **at the same time**.

### Container image

Pull the Release image, then mount `config.toml` and a volume for the semantic DB.

```bash
docker run --rm -p 8080:8080 \
  -e SQL_MCP_SEMANTIC_PATH=/data/semantic.db \
  -e SQL_MCP_CONFIG=/config/config.toml \
  -e SQL_MCP_TARGET_DW_URL="$SQL_MCP_TARGET_DW_URL" \
  -v "$HOME/sql-mcp/config.toml:/config/config.toml:ro" \
  -v sqlmcp-data:/data \
  <release-image>
```

HTTP MCP: `POST http://127.0.0.1:8080/mcp` · authoring API: `/api` · health: `GET /health`.

## 2. Connect an agent

### Cursor

`~/.cursor/mcp.json` (global) or the project `.cursor/mcp.json`:

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

Replace the paths with your own, then reload MCP. If the server is already running over HTTP, set `url` to `http://127.0.0.1:8080/mcp`.

### Claude Desktop

Add the same `mcpServers` entry as Cursor to `claude_desktop_config.json`.

- macOS: `~/Library/Application Support/Claude/claude_desktop_config.json`
- Windows: `%APPDATA%\Claude\claude_desktop_config.json`

Quit the app completely, then start it again.

### Claude Code

Register the same stdio command (or HTTP URL) in the project `.mcp.json`, or with `claude mcp add`.

## 3. Configure metadata

Read the physical DW catalog with HTTP `GET /api/databases/{id}/tables`, write JSON in this order, then register it with `POST /api/meta`.

```text
table → view → cube → metric | segment
```

The binary is enough. The server exposes the registration schema (`document.schema.json`) and Agent Skills (authoring, validation, and registration) as MCP Resources. Ask the agent to read the sql-mcp skill and schema Resources and register the DW metadata.

## 4. Query the warehouse

| Tool | Use |
|------|-----|
| `search_columns` | Search semantic columns and members |
| `sample_column` | Sample a table member (`database` / `table` / `name`, optional `like` substring match, plus a code dictionary) |
| `execute_sql` | **Semantic SQL only** → compile and run a physical SELECT |
| `list_databases` / `list_tables` / `describe_table` | List databases · table summaries (`database` / `name` / `description`) · cube detail |

`execute_sql` does not accept a free-form physical SELECT. Only a cube `FROM`, plus metric, dimension, and segment references, is allowed.

Example prompts:

- "From sales, get last month's revenue metric by region."
- "Find columns related to status and show a sample."
