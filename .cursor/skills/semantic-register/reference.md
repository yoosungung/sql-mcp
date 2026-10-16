# register — pointers

| 주제 | 위치 |
|------|------|
| metadata CRUD · 전체 집합 연결 검증 | `ARCHITECTURE.md` §1 |
| Resource URI | `ARCHITECTURE.md` §3.6 |
| 툴 표면 | `ARCHITECTURE.md` §3, `server/src/tools.rs` |
| 저장 `meta_*` | `semantic/libsql/catalog.sql`, `semantic/DESIGN.md` |
| 에러 code | `ARCHITECTURE.md` §3.7 (`validation` 등) |

## MCP Skills note

프로토콜 SEP-2640(Skills over MCP)은 Resource로 Skill을 제공할 수 있으나, Cursor 클라이언트는 아직 미지원. 이 레포는 Resource를 `document.schema.json`만 유지한다.

## Dependency order (example)

1. `customer` table → `orders` table  
2. `orders_customer` view  
3. `sales` cube  
4. `revenue` metric · `paid` segment
