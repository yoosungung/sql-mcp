# Agensi listing copy (not shipped in the ZIP)

Price: Free ($0)

Name: sql-mcp: semantic warehouse queries

Summary: Tells Cursor, Claude Code, and other agents to query Postgres or SQLite only through the sql-mcp semantic layer, and links the GitHub install guide. Free to use.

Description:

Agents lose the thread when a warehouse has dozens or hundreds of tables and they see the physical schema. sql-mcp is the MCP server that keeps them on a semantic layer: table, view, cube, then metric and segment. The agent writes semantic SQL. The server compiles it to a read-only physical SELECT and runs it.

This free skill is the prompt guide for that server. It tells the agent when to use sql-mcp, which tools to call (`list_databases`, `list_tables`, `describe_table`, `search_columns`, `sample_column`, `execute_sql`), and when to stop. It refuses free-form physical SQL and tells the agent not to ask for a DSN or password in chat. The binary is not in the package. Install steps point at the GitHub repository and the `current` Release.

https://github.com/yoosungung/sql-mcp

Compatibility: Requires the sql-mcp MCP server from GitHub Releases (macOS, Linux, or Windows binary, or the container image). Documented targets are Postgres and SQLite, read-only semantic SELECT. Cursor, Claude Desktop, and Claude Code connect over stdio or HTTP.

Docs: https://github.com/yoosungung/sql-mcp

See it in action:

User: From the sales warehouse, what was revenue by region last month? Do not write SQL against the base tables.

Agent: I will use sql-mcp and stay on the semantic layer. `list_databases` shows `dw`. `list_tables` shows a `sales` cube described as order lines at grain order_line_id. `describe_table` lists dimension `region`, metric `revenue` (sum of line amount), and segment `last_month`.

Semantic SQL:

SELECT region, revenue
FROM sales
WHERE last_month

`execute_sql` returns three rows: West 1,240,000; East 980,000; North 410,000. I used cube `sales`, metric `revenue`, and dimension `region`. I did not open a database client and I did not ask for the warehouse DSN.

User: sql-mcp is not in my MCP list. How do I install it?

Agent: The server is separate from this skill. Download the binary for your OS from https://github.com/yoosungung/sql-mcp/releases/tag/current , put it on PATH, and set `SQL_MCP_CONFIG`, `SQL_MCP_SEMANTIC_PATH`, and `SQL_MCP_TARGET_DW_URL` in your own environment. The full steps are in the repository README. Keep the DSN out of this chat. After you reload MCP, ask the revenue question again.

Limitations:

- The skill does not include the sql-mcp binary or container. Install those from GitHub.
- Read-only semantic SQL. No inserts, updates, deletes, or DDL.
- `execute_sql` rejects a free-form physical SELECT and rejects `SELECT *`.
- The public quickstart documents Postgres and SQLite.
- Cubes must already be registered, or the agent stops and points at metadata setup.
- Warehouse credentials stay in the user's environment. They are not part of this package.

FAQ:

Q: Is the MCP server included?
A: No. This package tells the agent how to use sql-mcp and where to install it. The server is the open-source project at https://github.com/yoosungung/sql-mcp .

Q: What does it cost?
A: The skill is free. Anyone with a connected agent can load it.

Q: Which agents can use it?
A: Any agent that can load this skill and connect an MCP server, including Cursor, Claude Code, and Claude Desktop.

Q: Will the agent see my database password?
A: It should not. The skill tells the agent not to ask for or repeat a DSN or password. You set `SQL_MCP_TARGET_DW_URL` locally.

Q: Can it query every table in the warehouse directly?
A: No. Questions go through registered cubes, metrics, dimensions, and segments. That is the point when the warehouse is large.

Tags: data-warehouse, semantic-layer, sql, mcp, postgres, sqlite

Use cases:

- Answer a business question from a warehouse without pasting the physical schema
- Install sql-mcp from GitHub and connect it to Cursor or Claude Code
- Query cubes, metrics, dimensions, and segments with read-only semantic SQL
