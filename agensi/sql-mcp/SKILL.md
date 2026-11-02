---
name: sql-mcp
description: >
  Guides an agent to answer data-warehouse questions only through the sql-mcp
  semantic layer (cube, metric, dimension, segment) on Postgres or SQLite.
  Use when the user asks for warehouse numbers, wants sql-mcp installed, or
  would otherwise write physical SQL against many tables. Sends them to the
  GitHub install guide when the MCP server is missing, and never repeats a
  DSN or password.
summary: >
  Free guide for Cursor, Claude Code, and other agents to query a warehouse
  through sql-mcp semantic SQL, with the GitHub install steps.
version: 1.0.0
tags:
  - data-warehouse
  - semantic-layer
  - sql
  - mcp
  - postgres
  - sqlite
keywords:
  - data warehouse
  - semantic SQL
  - MCP server
  - Cursor
  - Claude Code
  - business metrics
use_cases:
  - Answer a business question from a warehouse without pasting the physical schema
  - Install sql-mcp from GitHub and connect it to Cursor or Claude Code
  - Query cubes, metrics, dimensions, and segments with read-only semantic SQL
docs_url: https://github.com/yoosungung/sql-mcp
---

# sql-mcp

sql-mcp is an MCP server that compiles **semantic SQL** into a physical read-only SELECT. This skill tells the agent when to use that server and how to install it. It does not bundle the server binary.

Repository and full install guide: https://github.com/yoosungung/sql-mcp

Korean guide: https://github.com/yoosungung/sql-mcp/blob/master/README.ko.md

## When to use

Use this skill when the user wants numbers, lists, or samples from a data warehouse, or asks to connect sql-mcp. It fits warehouses with many tables, where handing the agent the physical schema causes wrong joins.

## When to stop

Stop and say this skill does not apply when the user wants writes, DDL, or a direct client against an application database. sql-mcp accepts read-only semantic SELECT only.

## Credentials

The user sets the warehouse DSN in their own environment (`SQL_MCP_TARGET_DW_URL`) and config file. Never ask them to paste a password, DSN, or connection string into the chat. Never print those values. Placeholder examples in `references/install.md` stay placeholders.

## If the sql-mcp tools are missing

The tools are `list_databases`, `list_tables`, `describe_table`, `search_columns`, `sample_column`, and `execute_sql`.

If they are not available:

1. Say that the sql-mcp MCP server is not connected.
2. Read `references/install.md` and walk the user through it.
3. Point at the Release binary or container and the README. Do not pipe a download into a shell.
4. After they reload MCP, continue only when the tools above exist.

## How to query

Discover the semantic model before writing SQL. Do not invent table or column names.

1. `list_databases` and choose the warehouse the user named.
2. `list_tables` for cube summaries (`database`, `name`, `description`).
3. `describe_table` on the cube you will query.
4. `search_columns` when a business term does not match a member name. `sample_column` (`database`, `table`, `name`, optional `like`) when a filter value or code is unclear.
5. `execute_sql` with **semantic SQL only**: `FROM` a cube, using metrics, dimensions, and segments.

`execute_sql` rejects a free-form physical SELECT and rejects `SELECT *`. If a call is rejected, fix the semantic query. Do not open a database client, driver, or second connection to bypass the server.

State the cube, the members you used, the semantic SQL, and the rows you got.

## If no cube is registered

Stop. Say the semantic layer is not registered yet. Registration order is `table`, then `view`, then `cube`, then `metric` or `segment`, via `POST /api/meta`. The running server exposes `document.schema.json` and the authoring skills as MCP Resources. Full steps: https://github.com/yoosungung/sql-mcp#3-configure-metadata

## Install reference

Read `references/install.md` when the user asks how to install sql-mcp or the tools are missing.
