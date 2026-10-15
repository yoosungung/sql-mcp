import copy
import json
from pathlib import Path

from sql_mcp_semantic.validate import KIND_DIRS, SchemaError, validate_document, validate_registry, validate_tree


class CatalogError(Exception):
    def __init__(self, errors: list[str]):
        self.errors = errors
        super().__init__("\n".join(errors))


class Catalog:
    def __init__(self, root: Path):
        self.root = Path(root)
        self._docs: dict[tuple[str, str], dict] = {}

    @classmethod
    def open(cls, root: Path) -> "Catalog":
        root = Path(root)
        root.mkdir(parents=True, exist_ok=True)
        catalog = cls(root)
        if any(root.rglob("*.json")):
            for document in validate_tree(root):
                catalog._docs[(document["kind"], document["name"])] = document
        return catalog

    def create(self, document: dict) -> dict:
        document = validate_document(copy.deepcopy(document))
        key = (document["kind"], document["name"])
        if key in self._docs:
            raise CatalogError([f"{document['kind']} {document['name']} 이 이미 있다"])
        self._commit(key, document)
        return copy.deepcopy(document)

    def update(self, document: dict) -> dict:
        document = validate_document(copy.deepcopy(document))
        key = (document["kind"], document["name"])
        if key not in self._docs:
            raise CatalogError([f"{document['kind']} {document['name']} 이 없다"])
        self._commit(key, document)
        return copy.deepcopy(document)

    def delete(self, kind: str, name: str) -> None:
        key = self._existing(kind, name)
        remaining = [document for existing, document in self._docs.items() if existing != key]
        validate_registry(remaining)
        self._path(kind, name).unlink()
        del self._docs[key]

    def read(self, kind: str, name: str) -> dict:
        key = self._existing(kind, name)
        return copy.deepcopy(self._docs[key])

    def list(self, kind: str | None = None) -> list[dict]:
        if kind is not None and kind not in KIND_DIRS:
            raise CatalogError(["kind 가 등록 문서가 아니다"])
        documents = [
            document
            for (doc_kind, _), document in self._docs.items()
            if kind is None or doc_kind == kind
        ]
        documents.sort(key=lambda document: (document["kind"], document["name"]))
        return [copy.deepcopy(document) for document in documents]

    def _existing(self, kind: str, name: str) -> tuple[str, str]:
        if kind not in KIND_DIRS:
            raise CatalogError(["kind 가 등록 문서가 아니다"])
        key = (kind, name)
        if key not in self._docs:
            raise CatalogError([f"{kind} {name} 이 없다"])
        return key

    def _commit(self, key: tuple[str, str], document: dict) -> None:
        proposed = [stored for existing, stored in self._docs.items() if existing != key]
        proposed.append(document)
        validate_registry(proposed)
        path = self._path(document["kind"], document["name"])
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps(document, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
        self._docs[key] = document

    def _path(self, kind: str, name: str) -> Path:
        return self.root / KIND_DIRS[kind] / f"{name}.json"


def document_result(action):
    try:
        return {"ok": True, **action()}
    except (SchemaError, CatalogError) as exc:
        return {"ok": False, "errors": exc.errors}
    except json.JSONDecodeError as exc:
        return {"ok": False, "errors": [f"JSON 이 아니다 ({exc})"]}


def as_document(document: dict | str) -> dict:
    if isinstance(document, str):
        parsed = json.loads(document)
        if not isinstance(parsed, dict):
            raise SchemaError(["문서는 객체여야 한다"])
        return parsed
    if isinstance(document, dict):
        return document
    raise SchemaError(["문서는 객체여야 한다"])
