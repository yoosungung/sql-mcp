import os
import sys
from pathlib import Path
from typing import Any

from mcp.server import MCPServer

from sql_mcp_semantic.catalog import Catalog, as_document, document_result

INSTRUCTIONS = "코딩 에이전트용 메타 CRUD. 저장 전에 JSON 검증과 연결 검증을 한다. DW 에이전트에게 이 도구를 연결하지 않는다."


def create_server(root: Path) -> MCPServer:
    catalog = Catalog.open(root)
    mcp = MCPServer("sql-mcp-meta", version="0.1.0", instructions=INSTRUCTIONS)

    @mcp.tool()
    def meta_create(document: dict[str, Any]) -> dict[str, Any]:
        """메타 문서를 추가한다. JSON 검증과 연결 검증을 통과해야 저장된다."""
        return document_result(lambda: {"document": catalog.create(as_document(document))})

    @mcp.tool()
    def meta_read(kind: str, name: str) -> dict[str, Any]:
        """kind 와 name 으로 메타 문서 하나를 읽는다."""
        return document_result(lambda: {"document": catalog.read(kind, name)})

    @mcp.tool()
    def meta_update(document: dict[str, Any]) -> dict[str, Any]:
        """같은 kind, name 의 메타 문서를 바꾼다. JSON 검증과 연결 검증을 통과해야 저장된다."""
        return document_result(lambda: {"document": catalog.update(as_document(document))})

    @mcp.tool()
    def meta_delete(kind: str, name: str) -> dict[str, Any]:
        """메타 문서를 지운다. 다른 문서가 참조하면 연결 검증으로 거절하고 파일을 남긴다."""

        def delete() -> dict:
            catalog.delete(kind, name)
            return {"kind": kind, "name": name}

        return document_result(delete)

    @mcp.tool()
    def meta_list(kind: str = "") -> dict[str, Any]:
        """메타 문서를 나열한다. kind 가 비어 있으면 전체를 반환한다."""
        return document_result(lambda: {"documents": catalog.list(kind or None)})

    return mcp


def main(argv: list[str]) -> None:
    root = Path(argv[1]) if len(argv) > 1 else Path(os.environ.get("SQL_MCP_META_ROOT", "semantic/meta"))
    create_server(root).run()


if __name__ == "__main__":
    main(sys.argv)
