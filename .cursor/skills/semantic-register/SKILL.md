---
name: semantic-register
description: >-
  Validates sql-mcp semantic registration JSON against document.schema.json and
  connection rules, then creates or updates documents via metadata_* MCP tools.
  Use when registering, updating, or deleting table/view/cube/metric/segment
  metadata, or when metadata_create returns validation errors.
---

# Semantic register · validate

초안 작성은 `semantic-table-view` / `semantic-cube-metric`. 이 스킬은 **검증 + CRUD**만.

## Workflow

```
Progress:
- [ ] 1. Schema 정본 로드
- [ ] 2. 문서 단위로 JSON Schema 형태 점검
- [ ] 3. 연결 그래프 순서대로 metadata_create / update
- [ ] 4. validation 실패 시 수정 후 재시도 (DB는 실패 시 불변)
- [ ] 5. metadata_get으로 round-trip 확인
```

### 1. Schema

우선 MCP Resource:

- URI: `https://sql-mcp.local/semantic/document.schema.json`
- `resources/list` → `resources/read`

폴백: 레포 파일 `semantic/schema/document.schema.json` (동일 `$id`).

### 2. Local shape check

문서 **하나**가 schema oneOf(`table`|`view`|`cube`|`metric`|`segment`)를 통과하는지 본다. 연결·존재 검사는 서버가 create/update 시 전체 등록 집합으로 한다.

파일로 둘 때: `semantic/examples/{tables|views|cubes|metrics|segments}/{name}.json`, 파일명 = `name`.

### 3. Register order

참조 방향: `table → view → cube → metric|segment`.

| 연산 | 툴 | 비고 |
|------|-----|------|
| 생성 | `metadata_create` | body = 문서 JSON |
| 교체 | `metadata_update` | full replace |
| 조회 | `metadata_get` / `metadata_list` | |
| 삭제 | `metadata_delete` | 삭제 후 남은 집합 연결 검증 |

한 번에 문서 하나. 실패(`validation`)면 시맨틱 DB는 그대로 — 메시지 보고 고친 뒤 같은 순서로 재시도.

의존이 빠졌으면 누락 table/view를 먼저 등록하거나, 같은 배치에서 참조 순서를 지킨다.

### 4. Fix loop

1. `code: validation` (또는 schema) 메시지 읽기
2. 해당 kind 초안 스킬·[reference.md](reference.md) quick checks
3. JSON 수정 → create/update 재호출
4. 통과할 때까지 반복. 추측으로 스키마를 바꾸지 않음

### 5. Verify

`metadata_get(kind, name)`으로 저장본 확인. 검색 인덱스는 CRUD 성공 후 서버가 재구축한다(에이전트가 직접 만지지 않음).

## Do not

- DW 에이전트 세션에 `metadata_*`를 노출·사용하지 않는다(계약 §1.8).
- Resource에 Skill/`SKILL.md`를 추가하지 않는다 — Cursor는 로컬 `.cursor/skills/`만 Skill로 본다.
- 시크릿·DSN을 메타 JSON이나 로그에 넣지 않는다.

상세 포인터: [reference.md](reference.md).
