# 등록·검증 계약

이 파일은 스킬 동봉 정본이다. JSON Schema 바이트 정본은 같은 폴더의 [document.schema.json](document.schema.json)이다. MCP가 있으면 Resource로도 동일 `$id`를 제공한다.

## 문서 kinds

`table` | `view` | `cube` | `metric` | `segment` (oneOf).

참조 방향: `table → view → cube → metric|segment`.

**이름 우선권:** cube · dimension · fact · metric · segment > table · view · view 컬럼(충돌 시 후자에 접미사). 물리 table/컬럼 경로는 유지(D22: `name`=시맨틱, `physical_ident`=물리).

연결 검증에 쓰는 링크: `fk.references`, view `sources`/`mappings`, cube `view`/`choice[].view`, dimension·fact `column`, metric·segment `cube`, metric `group_by`, **table/view/cube `target` 일치**(MCP 타깃 id; leaf table과 같아야 함).

## /api/meta 동작

| HTTP | 동작 |
|------|------|
| `POST /api/meta` | 문서 하나 schema 검증 → 반영 후 **전체 등록 집합** 연결 검증 → 성공 시만 저장 |
| `PUT /api/meta/{kind}/{name}` | full body replace, 동일 검증. view/cube/metric/segment는 path `name`과 body `name`이 다르면 rename(저장 id 유지). JSON에 `id` 없음 |
| `DELETE /api/meta/{kind}/{name}` | 뺀 뒤 남은 집합 연결 검증 |
| `GET /api/meta` · `GET /api/meta/{kind}/{name}` | 저장된 JSON 반환 |

검증 실패 시 시맨틱 DB는 바꾸지 않는다. 에러 `code` 예: `validation`, `not_found`, …

입출력은 JSON. 시크릿·DSN 필드를 메타에 넣지 않는다.

## Schema 로드 순서

1. MCP `resources/list` · `resources/read` — URI `https://sql-mcp.local/semantic/document.schema.json` (있을 때)
2. 폴백: 스킬 Resource `https://sql-mcp.local/skills/semantic-register/document.schema.json` 또는 이 스킬 동봉 [document.schema.json](document.schema.json)

스킬 본문·예시도 같은 prefix(`https://sql-mcp.local/skills/…`)로 Resource에 있다. 레포 경로에 의존하지 않는다.

## 파일 레이아웃 (선택)

문서를 파일로 둘 때 권장:

```text
tables/{name}.json
views/{name}.json
cubes/{name}.json
metrics/{name}.json
segments/{name}.json
```

파일명(확장자 제외) = `name`. 경로는 프로젝트마다 달라도 된다.

## DW vs coding agent

- 코딩 에이전트: HTTP `/api/meta` + schema Resource. 물리 카탈로그는 `/api/databases/{id}/tables`.
- DW 에이전트: 메타 CRUD를 주지 않는다. 테이블=cube, 컬럼=dimension/fact. metric/segment는 `describe_table.examples`만(D26).

## Quick checks

- [ ] 문서 하나가 schema oneOf를 통과
- [ ] create 순서가 참조 방향과 일치
- [ ] `validation`이면 메시지 보고 수정 후 재시도(추측으로 schema 변경 금지)
