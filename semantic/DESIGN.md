# semantic

코딩 에이전트가 등록하는 JSON의 스키마 정의와 예시다. 이 디렉터리에는 실행 코드를 두지 않는다.

필드 의미와 연결 규칙은 [ARCHITECTURE.md](../ARCHITECTURE.md)에 있다.

- `schema/` — JSON Schema. 문서 하나의 정본이다.
- `examples/` — 종류별 예시. 파일명(확장자 제외)은 `name`과 같다.
- `libsql/` — 예시 물리 테이블의 libSQL DDL 제안. `examples/tables`와 같은 테이블이다.

libSQL에는 스키마가 없다. JSON의 `db`가 데이터베이스이고 `table`이 물리 테이블 이름이다. `schema`는 논리 이름이다.

JSON 타입은 libSQL `STRICT` 타입으로 대응한다. `bigint`는 `INTEGER`, `varchar`와 `date`는 `TEXT`, `numeric`은 `REAL`이다. `date`는 `YYYY-MM-DD` 텍스트다. 행 필터는 JSON에 두고 DDL의 `CHECK`로 두지 않는다.
