# sql-mcp

회사 데이터베이스를 조회하는 MCP 서버다. 코딩 에이전트는 시멘틱 JSON을 등록하고, DW 에이전트는 cube를 테이블처럼 보고 SELECT를 실행한다.

계약과 등록 문서 필드는 [ARCHITECTURE.md](ARCHITECTURE.md), 컴포넌트 사용법은 [semantic/DESIGN.md](semantic/DESIGN.md), 순서는 [ROADMAP.md](ROADMAP.md)에 있다.

메타 CRUD 도구는 `meta_create`, `meta_read`, `meta_update`, `meta_delete`, `meta_list`다. 저장 전에 JSON 검증과 연결 검증을 한다.

등록 예시 검증:

```bash
python3 -m venv .venv
.venv/bin/pip install -r semantic/requirements.txt -r semantic/requirements-dev.txt
PYTHONPATH=semantic .venv/bin/python -m sql_mcp_semantic.validate semantic/examples
```
