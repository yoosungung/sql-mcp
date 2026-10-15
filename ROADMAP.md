# ROADMAP

## 현재

코딩 에이전트가 등록하는 table, view, cube, metric, segment의 JSON Schema와 예시. `semantic/`에는 정의와 예시만 있다. 물리 테이블의 libSQL DDL 제안은 `semantic/libsql/`에 있다.

## 다음

- MCP 메타 CRUD. 검증은 `semantic/schema`의 JSON Schema로 한다.
- DW MCP 도구: 컬럼 검색, 큐브 목록, 큐브 조회, 샘플, SQL 실행
- SELECT AST 검사와 물리 SQL 컴파일

## 미결정

- 시멘틱 SQL에서 metric과 segment를 가리키는 문법
- `encryption`이 true일 때의 암호 방식
- DW SQL 다이얼렉트를 libSQL로 확정할지. 제안 DDL은 `semantic/libsql/`에 있다.
- dimension·fact `expression`과 metric `expression`의 view 컬럼 이름 검사, segment `filter`의 컬럼 이름 검사. 컴파일 단계에서 한다.
