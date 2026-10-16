---
name: semantic-table-view
description: >-
  Explores target DW catalogs and drafts semantic table and view registration
  JSON (kind table/view) for sql-mcp. Use when mapping physical tables to
  semantic documents, creating or editing files under semantic/examples/tables
  or views, or when the user asks to register DW schema as table/view metadata.
---

# Semantic table · view from DW

코딩 에이전트 전용. DW 에이전트 워크플로가 아니다. 계약·필드 정본은 레포의 `ARCHITECTURE.md` §1·§4와 `semantic/schema/document.schema.json`이다. 복제하지 말고 필요 시 읽는다.

예시는 `semantic/examples/tables/`, `semantic/examples/views/` — 파일명(확장자 제외) = `name`.

## Workflow

```
Progress:
- [ ] 1. list_targets → 대상 db id
- [ ] 2. list_tables / describe_table / sample_column으로 물리 스키마·코드성 파악
- [ ] 3. table JSON 초안 (필터·pk·fk·as_code)
- [ ] 4. view JSON 초안 (combine·sources·mappings)
- [ ] 5. semantic-register로 검증·등록 (이 스킬에서 metadata_* 호출하지 않음)
```

### 1–2. Explore

MCP 툴만 사용한다(시크릿·DSN을 채팅에 받지 않음).

| 목적 | 툴 |
|------|-----|
| 타깃 id | `list_targets` |
| 물리 목록 | `list_tables` (`target` 지정) |
| 컬럼·타입 | `describe_table` |
| 코드성·샘플 | `sample_column` → `as_code` 후보 |

이미 등록된 시맨틱 table이 있으면 `metadata_list`/`metadata_get`으로 중복·이름을 확인한다.

### 3. Draft `table`

필수: `kind`, `name`, `db`, `schema`, `table`, `columns`, `pk`, `fk`, `permission_filters`, `scope_filter`, `default_filter`.

- `db` = 타깃 id. `schema`/`table` = 물리 이름.
- `columns[]`: `name`, `type`, `nullable`; 코드성이면 `as_code: true`(D9 — 에이전트가 기입, 서버는 추론하지 않음).
- `pk` = 컬럼 이름 배열. `fk.columns` = 이 테이블, `fk.references` = 다른 **논리** table `name` + 컬럼.
- 필터 식은 **물리 컬럼 이름**. `scope_filter`/`default_filter`는 객체 또는 `null`. `permission_filters`는 배열(없으면 `[]`).
- `role` 생략 또는 `"all"`.

형태는 `semantic/examples/tables/orders.json`을 따른다. 요약/월별 물리 테이블 예: `orders_month.json`.

### 4. Draft `view`

필수: `kind`, `name`, `combine`, `sources`, `columns`. `combine` 없으면 view가 아니다.

| combine | sources | 추가 |
|---------|---------|------|
| `join` | table | `joins` (`inner`/`left`/`right`/`full`) |
| `union` / `minus` | table | 첫 source 기준; 각 view 컬럼이 **모든** source를 mapping |
| `select` | **view** | `prefer` 가능; 요약 view 우선 규칙은 ARCHITECTURE |

- join 시 동일 의미·동일 이름 컬럼은 mappings를 여러 source에 묶는다(예: `customer_id`).
- 이름 충돌은 view 컬럼명을 바꾼다(예: `customer_status_code`).

형태는 `semantic/examples/views/orders_customer.json`.

### 5. Hand off

초안 JSON을 파일로 쓰거나 사용자에게 제시한 뒤 **`semantic-register`** 스킬로 schema·연결 검증과 `metadata_create`를 맡긴다. 등록 순서: 참조되는 table → view.

## Do not

- metric/segment/cube를 이 스킬에서 설계하지 않는다 → `semantic-cube-metric`.
- `execute_sql`로 물리 SELECT를 돌리지 않는다(시맨틱 SQL만).
- 스키마·연결 규칙을 SKILL에 장문으로 복사하지 않는다 → [reference.md](reference.md).
