-- MCP 시멘틱 등록 JSON을 담는 카탈로그.
-- 문서 하나의 정본은 body 의 JSON 이다. 필드 검증은 semantic/schema/document.schema.json 으로 한다.
-- 연결 검증은 등록 집합 전체를 읽은 뒤 ARCHITECTURE 규칙으로 한다.
-- description 은 검색용으로 body 바깥에 둔다. body.$.description 과 같아야 한다.

CREATE TABLE IF NOT EXISTS document (
  kind TEXT NOT NULL CHECK (
    kind IN ('table', 'view', 'cube', 'metric', 'segment')
  ),
  name TEXT NOT NULL CHECK (
    name GLOB '[A-Za-z_]*'
    AND name NOT GLOB '*[^A-Za-z0-9_]*'
  ),
  description TEXT,
  body TEXT NOT NULL CHECK (
    json_valid(body)
    AND json_extract(body, '$.kind') = kind
    AND json_extract(body, '$.name') = name
    AND (
      (description IS NULL AND json_extract(body, '$.description') IS NULL)
      OR description = json_extract(body, '$.description')
    )
  ),
  PRIMARY KEY (kind, name)
) STRICT;

-- search_columns 인덱스. 문서 단위가 아니라 검색 가능한 필드(컬럼/dimension/fact/metric) 한 행.
-- text 는 BM25(FTS5)용, embedding 은 벡터(리틀엔디안 f32 blob)용.
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

-- FTS는 content 테이블과 수동 동기화(트리거 없음). MCP가 CRUD 때 함께 갱신한다.
CREATE VIRTUAL TABLE IF NOT EXISTS column_index_fts USING fts5(
  text,
  row_id UNINDEXED
);
