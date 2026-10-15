# ROADMAP

## 현재

코딩 에이전트가 등록하는 table, view, cube, metric, segment JSON 스키마, 등록 검증, 메타 CRUD MCP 도구.

## 다음

- DW MCP 도구: 컬럼 검색, 큐브 목록, 큐브 조회, 샘플, SQL 실행
- SELECT AST 검사와 물리 SQL 컴파일

## 미결정

- 시멘틱 SQL에서 metric과 segment를 가리키는 문법
- `encryption`이 true일 때의 암호 방식
- DW SQL 다이얼렉트
- metric `expression`과 segment `filter` 안의 컬럼 이름 검사. 컴파일 단계에서 한다.
