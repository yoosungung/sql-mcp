import asyncio
import json

from mcp import Client

from sql_mcp_semantic.server import create_server


def test_meta_tools_validate_json_and_links(tmp_path):
    server = create_server(tmp_path)

    async def run():
        async with Client(server) as client:
            names = sorted(tool.name for tool in (await client.list_tools()).tools)
            assert names == ["meta_create", "meta_delete", "meta_list", "meta_read", "meta_update"]

            invalid = await client.call_tool(
                "meta_create",
                {"document": {"kind": "table", "name": "orders"}},
            )
            body = invalid.structured_content
            assert body["ok"] is False
            assert body["errors"]
            assert list(tmp_path.rglob("*.json")) == []

            created = await client.call_tool("meta_create", {"document": _table()})
            assert created.structured_content["ok"] is True
            assert created.is_error is False

            missing = await client.call_tool("meta_create", {"document": json.dumps(_view("missing"))})
            assert missing.structured_content["ok"] is False
            assert any("missing" in error for error in missing.structured_content["errors"])

            linked = await client.call_tool("meta_create", {"document": _view("orders")})
            assert linked.structured_content["ok"] is True

            removed = await client.call_tool("meta_delete", {"kind": "table", "name": "orders"})
            assert removed.structured_content["ok"] is False
            assert (tmp_path / "tables" / "orders.json").exists()

            listed = await client.call_tool("meta_list", {"kind": "view"})
            assert [item["name"] for item in listed.structured_content["documents"]] == ["orders_view"]

    asyncio.run(run())


def _table() -> dict:
    return {
        "kind": "table",
        "name": "orders",
        "db": "dw",
        "schema": "sales",
        "table": "orders",
        "columns": [
            {"name": "order_id", "type": "bigint", "nullable": False},
            {"name": "amount", "type": "numeric", "nullable": False},
        ],
        "pk": ["order_id"],
        "fk": [],
        "permission_filters": [],
        "scope_filter": None,
        "default_filter": None,
    }


def _view(table_name: str) -> dict:
    return {
        "kind": "view",
        "name": "orders_view",
        "combine": "from",
        "sources": [{"kind": "table", "name": table_name}],
        "columns": [
            {
                "name": "order_id",
                "mappings": [{"source": table_name, "column": "order_id"}],
                "same_name": False,
                "same_data": False,
                "same_code": False,
            }
        ],
    }
