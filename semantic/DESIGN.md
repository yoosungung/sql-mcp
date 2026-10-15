# semantic

코딩 에이전트가 등록하는 JSON의 스키마 정의와 예시다. 이 디렉터리에는 실행 코드를 두지 않는다.

필드 의미와 연결 규칙은 [ARCHITECTURE.md](../ARCHITECTURE.md)에 있다.

- `schema/` — JSON Schema. 문서 하나의 정본이다.
- `examples/` — 종류별 예시. 파일명(확장자 제외)은 `name`과 같다.
- `libsql/` — MCP가 등록 JSON을 저장하는 시맨틱 DB(libSQL) 테이블 제안.

## libSQL 카탈로그

`libsql/catalog.sql`은 MCP 메타 CRUD가 쓰는 시맨틱 DB다. 타깃 DW 물리 테이블이 아니다.

문서 한 건은 `document` 한 행이다. `kind`와 `name`이 기본키이고, `body`에 등록 JSON 전체를 둔다. `body.kind`와 `body.name`은 행의 `kind`, `name`과 같아야 한다.

`metadata_create` / `metadata_update` / `metadata_delete`는 JSON Schema와 연결 검증을 통과한 뒤에만 이 테이블을 바꾼다. `metadata_get`은 `(kind, name)`으로 `body`를 읽고, `metadata_list`는 `kind`로 걸러 `body` 목록을 반환한다.

필드별 정규화 테이블은 두지 않는다. 연결 검사는 등록 집합의 `body`를 읽어 한다.
