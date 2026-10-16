---
name: semantic-register
description: >-
  Validates sql-mcp semantic registration JSON against the bundled
  document.schema.json and connection rules, then creates or updates documents
  via metadata_* MCP tools. Use when registering, updating, or deleting
  table/view/cube/metric/segment metadata, or when metadata_create returns
  validation errors.
---

# Semantic register · validate

초안 작성은 `semantic-table-view` / `semantic-cube-metric`. 이 스킬은 **검증 + CRUD**만.

규칙: [contract.md](contract.md). Schema: [document.schema.json](document.schema.json).

## Workflow

```
Progress:
- [ ] 1. Schema 로드 (MCP Resource 또는 이 폴더 document.schema.json)
- [ ] 2. 문서 단위 형태 점검
- [ ] 3. 연결 순서대로 metadata_create / update
- [ ] 4. validation 실패 시 수정 후 재시도
- [ ] 5. metadata_get으로 round-trip 확인
```

### 1–2. Schema · shape

[contract.md](contract.md)의 Schema 로드 순서. 문서 하나가 `table|view|cube|metric|segment` oneOf를 통과하는지 본다. 연결·존재는 서버가 create/update 시 전체 집합으로 검사한다.

### 3. Register order

`table → view → cube → metric|segment`. 한 번에 문서 하나.

| 연산 | 툴 |
|------|-----|
| 생성 | `metadata_create` |
| 교체 | `metadata_update` |
| 조회 | `metadata_get` / `metadata_list` |
| 삭제 | `metadata_delete` |

### 4. Fix loop

1. `validation`(또는 schema) 메시지 읽기  
2. 초안 스킬의 `contract.md` quick checks  
3. JSON 수정 → 재호출  
4. 통과할 때까지. schema 파일을 임의로 고치지 않음

### 5. Verify

`metadata_get(kind, name)`. 검색 인덱스는 서버가 CRUD 성공 후 재구축한다.

## Do not

- DW 에이전트 세션에서 `metadata_*` 사용.
- 타깃 물리 DB 탐색을 이 스킬의 책임으로 두지 않음.
- 외부 레포 경로를 schema/계약 정본으로 삼지 않음 — 이 스킬 폴더(+ 가능하면 MCP Resource)만.
