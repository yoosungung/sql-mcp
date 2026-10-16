# table · view — pointers

정본만 링크한다. 내용을 여기 복제하지 않는다.

| 주제 | 위치 |
|------|------|
| 계약·연결·필터 AND 순서 | `ARCHITECTURE.md` §1 |
| 등록 문서 필드 | `ARCHITECTURE.md` §4 |
| JSON Schema | `semantic/schema/document.schema.json` (MCP Resource URI 동일 `$id`) |
| 예시 | `semantic/examples/tables/`, `semantic/examples/views/` |
| 검색 투영(table column) | `semantic/DESIGN.md` D12 |

## Quick checks

- [ ] `fk.references.table`은 이미 존재하는(또는 함께 등록할) 논리 table `name`
- [ ] view `sources[].name`이 논리 table/view `name`과 일치
- [ ] join `on` 컬럼이 각 source table에 존재
- [ ] union/minus: 모든 mapping이 전 source를 덮음
- [ ] 필터 식별자는 물리 컬럼(table) / 해당 단계 규칙(ARCHITECTURE)
