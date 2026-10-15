# semantic

코딩 에이전트가 git에 두고 MCP 메타로 등록하는 JSON 문서의 스키마와 등록 검증.

필드 의미와 에이전트 계약은 [ARCHITECTURE.md](../ARCHITECTURE.md)에 있다. 기계가 읽는 정본은 `schema/` 다.

## Commands

```bash
python3 -m venv .venv
.venv/bin/pip install -r semantic/requirements.txt -r semantic/requirements-dev.txt
PYTHONPATH=semantic .venv/bin/pytest semantic/tests
PYTHONPATH=semantic .venv/bin/python -m sql_mcp_semantic.validate semantic/examples
```
