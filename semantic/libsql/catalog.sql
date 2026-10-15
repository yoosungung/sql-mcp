-- MCP 시멘틱 등록 JSON을 담는 카탈로그.
-- 문서 하나의 정본은 body 의 JSON 이다. 필드 검증은 semantic/schema/document.schema.json 으로 한다.
-- 연결 검증은 등록 집합 전체를 읽은 뒤 ARCHITECTURE 규칙으로 한다.

CREATE TABLE document (
  kind TEXT NOT NULL CHECK (
    kind IN ('table', 'view', 'cube', 'metric', 'segment')
  ),
  name TEXT NOT NULL CHECK (
    name GLOB '[A-Za-z_]*'
    AND name NOT GLOB '*[^A-Za-z0-9_]*'
  ),
  body TEXT NOT NULL CHECK (
    json_valid(body)
    AND json_extract(body, '$.kind') = kind
    AND json_extract(body, '$.name') = name
  ),
  PRIMARY KEY (kind, name)
) STRICT;
