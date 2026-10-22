---
name: semantic-table-view
description: >-
  Explores a target data warehouse outside MCP and drafts semantic table and
  view registration JSON (kind table/view) for sql-mcp. Use when mapping
  physical DW tables to semantic documents, or when the user asks to author
  table/view metadata for a coding agent.
---

# Semantic table · view from DW

코딩 에이전트 전용.

규칙·필드: [contract.md](contract.md). 형태 예시: [examples/](examples/).

## Output layout

문서마다 `{outdir}/{name}.json` 한 파일(파일명 = `name`, `kind`는 JSON 안). 하위 `tables/` 폴더는 필수가 아니다. `outdir`는 사용자가 준 초안 디렉터리(프로젝트마다 다름).

다수 테이블이면 카탈로그를 읽는 **로컬 스크립트**로 table/view 골격을 만들어도 된다. 결과는 [contract.md](contract.md)를 지켜야 하며, 에이전트가 샘플·`as_code`·필터를 검토한다. **사용자가 요청하기 전에는 `metadata_*`로 MCP 등록하지 않는다.**

## Workflow

```
Progress:
- [ ] 1. 타깃 DW를 MCP 밖에서 탐색 (카탈로그·컬럼·샘플)
- [ ] 2. 이미 등록된 시맨틱 table/view 이름 확인 (선택, metadata_*)
- [ ] 3. table JSON 초안 → {outdir}/{name}.json
- [ ] 4. view JSON 초안 → {outdir}/{name}.json
- [ ] 5. 등록은 사용자 요청 시에만 semantic-register
```

### 1. Explore target DW (not MCP)

**물리 타깃 DB 조회는 MCP로 하지 않는다.** sql-mcp의 코딩 에이전트용 MCP 툴은 시맨틱 메타(`metadata_*`)·스키마 Resource 중심이다. 타깃 카탈로그·DDL·샘플은 환경에 맞게 직접 한다(예: `psql`/`sqlite3`/클라우드 콘솔/기존 DW 클라이언트). 시크릿·DSN은 채팅에 받지 말고 이미 설정된 연결만 쓴다.

확인할 것:

- 물리 `db` / `schema` / `table` 이름
- 컬럼명·타입·nullable
- PK·FK 후보
- 필터·코드값 분포(코드사전 `as_code`는 cube dimension에 등록)

### 2. Existing semantic names (optional)

이미 sql-mcp에 붙은 경우, 중복·참조용으로만 `metadata_list` / `metadata_get`을 쓴다. 물리 스키마를 알아내는 용도가 아니다.

### 3. Draft `table`

[contract.md](contract.md)의 table 절. 예시는 [examples/tables/orders.json](examples/tables/orders.json), [customer.json](examples/tables/customer.json), [orders_month.json](examples/tables/orders_month.json).

### 4. Draft `view`

[contract.md](contract.md)의 view 절. 예시는 [examples/views/orders_customer.json](examples/views/orders_customer.json).

### 5. Hand off

초안을 파일 또는 메시지로 둔 뒤 **`semantic-register`**에 맡긴다. 등록 순서: 참조되는 table → view.

## Do not

- 타깃 DW 탐색에 MCP `list_tables` / `describe_table` / `sample_column` / `execute_sql`에 의존하지 않는다.
- cube / metric / segment는 `semantic-cube-metric-segment`. Q&A·지식 보강은 `semantic-from-qa`. eval 기반 그레인 분할은 `semantic-from-eval`.
