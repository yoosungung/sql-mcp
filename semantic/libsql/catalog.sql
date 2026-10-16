-- 시맨틱 카탈로그: 타입별 meta_* 테이블이 저장 계층.
-- MCP metadata_* 입출력 JSON 검증 정본은 semantic/schema/document.schema.json.
-- 검색: column_index(+embedding) + column_index_fts(FTS5/bm25). keyword는 FTS5이며 vector/hybrid(RRF) 유지.

-- 1. Table
CREATE TABLE IF NOT EXISTS meta_tables (
  name TEXT PRIMARY KEY,
  description TEXT,
  db TEXT NOT NULL,
  schema TEXT NOT NULL,
  table_name TEXT NOT NULL,
  pk_json TEXT,
  fk_json TEXT,
  scope_filter TEXT,
  default_filter TEXT,
  role_json TEXT NOT NULL DEFAULT '"all"'
) STRICT;

CREATE TABLE IF NOT EXISTS meta_table_columns (
  table_name TEXT NOT NULL REFERENCES meta_tables(name) ON DELETE CASCADE,
  name TEXT NOT NULL,
  type TEXT NOT NULL,
  nullable INTEGER NOT NULL,
  PRIMARY KEY (table_name, name)
) STRICT;

CREATE TABLE IF NOT EXISTS meta_table_permission_filters (
  table_name TEXT NOT NULL REFERENCES meta_tables(name) ON DELETE CASCADE,
  role TEXT NOT NULL,
  expression TEXT NOT NULL
) STRICT;

-- 2. View
CREATE TABLE IF NOT EXISTS meta_views (
  name TEXT PRIMARY KEY,
  description TEXT,
  combine TEXT NOT NULL CHECK (combine IN ('join', 'union', 'minus', 'select')),
  prefer TEXT CHECK (prefer IS NULL OR prefer = 'summary')
) STRICT;

CREATE TABLE IF NOT EXISTS meta_view_sources (
  view_name TEXT NOT NULL REFERENCES meta_views(name) ON DELETE CASCADE,
  kind TEXT NOT NULL CHECK (kind IN ('table', 'view')),
  source_name TEXT NOT NULL,
  summary INTEGER NOT NULL DEFAULT 0
) STRICT;

CREATE TABLE IF NOT EXISTS meta_view_joins (
  view_name TEXT NOT NULL REFERENCES meta_views(name) ON DELETE CASCADE,
  left_source TEXT NOT NULL,
  right_source TEXT NOT NULL,
  join_type TEXT NOT NULL CHECK (join_type IN ('inner', 'left', 'right', 'full')),
  on_conditions_json TEXT NOT NULL
) STRICT;

CREATE TABLE IF NOT EXISTS meta_view_columns (
  view_name TEXT NOT NULL REFERENCES meta_views(name) ON DELETE CASCADE,
  name TEXT NOT NULL,
  mappings_json TEXT NOT NULL,
  PRIMARY KEY (view_name, name)
) STRICT;

-- 3. Cube
CREATE TABLE IF NOT EXISTS meta_cubes (
  name TEXT PRIMARY KEY,
  view_name TEXT NOT NULL REFERENCES meta_views(name),
  description TEXT NOT NULL
) STRICT;

CREATE TABLE IF NOT EXISTS meta_cube_dimensions (
  cube_name TEXT NOT NULL REFERENCES meta_cubes(name) ON DELETE CASCADE,
  name TEXT NOT NULL,
  type TEXT CHECK (type IN ('code', 'date', 'time', 'datetime', 'string')),
  column_ref TEXT,
  expression TEXT,
  encryption INTEGER NOT NULL,
  as_code_json TEXT,
  role_json TEXT NOT NULL DEFAULT '"all"',
  PRIMARY KEY (cube_name, name)
) STRICT;

CREATE TABLE IF NOT EXISTS meta_cube_facts (
  cube_name TEXT NOT NULL REFERENCES meta_cubes(name) ON DELETE CASCADE,
  name TEXT NOT NULL,
  type TEXT NOT NULL CHECK (type IN ('amount', 'fx', 'quantity')),
  column_ref TEXT,
  expression TEXT,
  encryption INTEGER NOT NULL,
  role_json TEXT NOT NULL DEFAULT '"all"',
  PRIMARY KEY (cube_name, name)
) STRICT;

-- 4. Metric & Segment
CREATE TABLE IF NOT EXISTS meta_metrics (
  name TEXT PRIMARY KEY,
  cube_name TEXT NOT NULL REFERENCES meta_cubes(name),
  description TEXT,
  expression TEXT NOT NULL,
  aggregation TEXT CHECK (aggregation IN ('group', 'rollup', 'cube', 'grouping_sets')),
  group_by_json TEXT,
  filter_expression TEXT,
  role_json TEXT NOT NULL DEFAULT '"all"'
) STRICT;

CREATE TABLE IF NOT EXISTS meta_segments (
  name TEXT PRIMARY KEY,
  cube_name TEXT NOT NULL REFERENCES meta_cubes(name),
  description TEXT,
  filter_expression TEXT NOT NULL,
  role_json TEXT NOT NULL DEFAULT '"all"'
) STRICT;

-- 5. 검색 투영 (FTS5 keyword/bm25 + vector embedding). 이중 FTS 없음.
-- D12: 실제 적재는 cube/dim/fact/metric/segment(이름·설명·dim type·fact amount/fx)만. column 역할은 레거시 CHECK용.
CREATE TABLE IF NOT EXISTS column_index (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  doc_kind TEXT NOT NULL CHECK (
    doc_kind IN ('table', 'view', 'cube', 'metric', 'segment')
  ),
  doc_name TEXT NOT NULL,
  field_name TEXT NOT NULL,
  field_role TEXT NOT NULL CHECK (
    field_role IN ('column', 'dimension', 'fact', 'metric', 'segment', 'document')
  ),
  text TEXT NOT NULL,
  embedding BLOB,
  UNIQUE (doc_kind, doc_name, field_name, field_role)
) STRICT;

CREATE VIRTUAL TABLE IF NOT EXISTS column_index_fts USING fts5(
  text,
  row_id UNINDEXED
);
