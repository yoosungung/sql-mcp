import json
import sys
from pathlib import Path

from jsonschema import Draft202012Validator
from referencing import Registry, Resource
from referencing.exceptions import Unresolvable
from referencing.jsonschema import DRAFT202012

SCHEMA_DIR = Path(__file__).resolve().parent.parent / "schema"
KIND_DIRS = {
    "table": "tables",
    "view": "views",
    "cube": "cubes",
    "metric": "metrics",
    "segment": "segments",
}
_VALIDATORS = None


class SchemaError(Exception):
    def __init__(self, errors: list[str]):
        self.errors = errors
        super().__init__("\n".join(errors))


def validate_document(document: dict) -> dict:
    errors = _document_errors(document)
    if errors:
        raise SchemaError(errors)
    return document


def validate_registry(documents: list[dict]) -> list[dict]:
    errors = []
    for document in documents:
        errors.extend(_prefix(document.get("name", ""), _document_errors(document)))
    if errors:
        raise SchemaError(errors)
    errors = _registry_errors(documents)
    if errors:
        raise SchemaError(errors)
    return documents


def validate_file(path: Path) -> dict:
    path = Path(path)
    try:
        document = json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as exc:
        raise SchemaError([f"{path}: JSON 이 아니다 ({exc})"]) from exc
    if not isinstance(document, dict):
        raise SchemaError([f"{path}: 문서는 객체여야 한다"])
    errors = []
    kind = document.get("kind")
    directory = KIND_DIRS.get(kind)
    if directory is None:
        errors.append(f"{path}: kind 가 등록 문서가 아니다")
    elif path.parent.name != directory:
        errors.append(f"{path}: 디렉터리는 {directory} 여야 한다")
    name = document.get("name")
    if path.stem != name:
        errors.append(f"{path}: 파일명 {path.stem}.json 이 name {name} 와 다르다")
    errors.extend(_prefix(str(path), _document_errors(document)))
    if errors:
        raise SchemaError(errors)
    return document


def validate_tree(root: Path) -> list[dict]:
    root = Path(root)
    documents = [validate_file(path) for path in sorted(root.rglob("*.json"))]
    validate_registry(documents)
    return documents


def _document_errors(document: dict) -> list[str]:
    if not isinstance(document, dict):
        return ["문서는 객체여야 한다"]
    kind = document.get("kind")
    validator = _validators().get(kind)
    if validator is None:
        return ["kind 가 등록 문서가 아니다"]
    errors = _schema_errors(validator, document)
    if errors:
        return errors
    return _local_errors(document)


def _validators() -> dict[str, Draft202012Validator]:
    global _VALIDATORS
    if _VALIDATORS is not None:
        return _VALIDATORS
    resources = []
    schemas = {}
    for path in sorted(SCHEMA_DIR.glob("*.schema.json")):
        schema = json.loads(path.read_text(encoding="utf-8"))
        schemas[schema["$id"]] = schema
        resources.append((schema["$id"], Resource.from_contents(schema, default_specification=DRAFT202012)))
    registry = Registry().with_resources(resources)
    for schema in schemas.values():
        Draft202012Validator.check_schema(schema)
    validators = {}
    for kind, filename in (
        ("table", "table.schema.json"),
        ("view", "view.schema.json"),
        ("cube", "cube.schema.json"),
        ("metric", "metric.schema.json"),
        ("segment", "segment.schema.json"),
    ):
        schema_id = f"https://sql-mcp.local/semantic/{filename}"
        Draft202012Validator.check_schema(schemas[schema_id])
        validators[kind] = Draft202012Validator(schemas[schema_id], registry=registry)
    _VALIDATORS = validators
    return validators


def _schema_errors(validator: Draft202012Validator, document: dict) -> list[str]:
    errors = []
    try:
        found = sorted(validator.iter_errors(document), key=lambda item: list(item.absolute_path))
    except Unresolvable as exc:
        return [f"스키마 참조를 풀 수 없다: {exc}"]
    for error in found:
        path = "/" + "/".join(str(part) for part in error.absolute_path)
        if error.validator == "contains":
            errors.append(f"{path}: summary 가 true 인 source 가 없다")
            continue
        errors.append(f"{path}: {error.message}")
    return errors


def _local_errors(document: dict) -> list[str]:
    kind = document["kind"]
    if kind == "table":
        return _table_errors(document)
    if kind == "view":
        return _view_errors(document)
    if kind == "cube":
        return _cube_errors(document)
    return []


def _table_errors(document: dict) -> list[str]:
    errors = []
    columns = _names(document["columns"])
    errors.extend(_duplicates("columns", columns))
    for name in document["pk"]:
        if name not in columns:
            errors.append(f"pk 컬럼 {name} 이 columns 에 없다")
    for index, foreign_key in enumerate(document["fk"]):
        for name in foreign_key["columns"]:
            if name not in columns:
                errors.append(f"fk/{index} 컬럼 {name} 이 columns 에 없다")
        if len(foreign_key["columns"]) != len(foreign_key["references"]["columns"]):
            errors.append(f"fk/{index} 컬럼 수가 참조 컬럼 수와 다르다")
    return errors


def _view_errors(document: dict) -> list[str]:
    errors = []
    sources = [source["name"] for source in document["sources"]]
    errors.extend(_duplicates("sources", sources))
    source_names = set(sources)
    errors.extend(_duplicates("columns", _names(document["columns"])))
    combine = document["combine"]
    if combine != "select":
        for source in document["sources"]:
            if "summary" in source:
                errors.append(f"source {source['name']} 의 summary 는 select 에서만 쓴다")
    if combine == "join":
        errors.extend(_join_errors(document, source_names))
    for column in document["columns"]:
        errors.extend(_column_bundle_errors(combine, column, source_names))
    return errors


def _join_errors(document: dict, source_names: set[str]) -> list[str]:
    errors = []
    parent = {name: name for name in source_names}

    def find(name: str) -> str:
        while parent[name] != name:
            parent[name] = parent[parent[name]]
            name = parent[name]
        return name

    def union(left: str, right: str) -> None:
        parent[find(left)] = find(right)

    for index, join in enumerate(document["joins"]):
        for side in ("left", "right"):
            if join[side] not in source_names:
                errors.append(f"joins/{index} {side} {join[side]} 이 sources 에 없다")
        if join["left"] == join["right"]:
            errors.append(f"joins/{index} 가 같은 source 를 묶는다")
        elif join["left"] in source_names and join["right"] in source_names:
            union(join["left"], join["right"])
    if source_names and len({find(name) for name in source_names}) > 1:
        errors.append("join 이 모든 source 를 연결하지 않는다")
    return errors


def _column_bundle_errors(combine: str, column: dict, source_names: set[str]) -> list[str]:
    errors = []
    mappings = column["mappings"]
    mapped_sources = [item["source"] for item in mappings]
    if len(mapped_sources) != len(set(mapped_sources)):
        errors.append(f"컬럼 {column['name']} 이 같은 source 를 두 번 가리킨다")
    for item in mappings:
        if item["source"] not in source_names:
            errors.append(f"컬럼 {column['name']} 의 source {item['source']} 가 sources 에 없다")
    bundled = column["same_name"] and column["same_data"] and column["same_code"]
    if len(mappings) >= 2:
        if not bundled:
            errors.append(
                f"컬럼 {column['name']} 을 묶으려면 same_name, same_data, same_code 가 모두 true 여야 한다"
            )
        if len({item["column"] for item in mappings}) != 1:
            errors.append(f"컬럼 {column['name']} 은 같은 이름의 컬럼만 묶을 수 있다")
    elif bundled:
        errors.append(f"컬럼 {column['name']} 은 묶음이 아닌데 same_name, same_data, same_code 가 true 다")
    if combine in ("union", "minus") and set(mapped_sources) != source_names:
        errors.append(f"{combine} 컬럼 {column['name']} 이 모든 source 를 가리켜야 한다")
    return errors


def _cube_errors(document: dict) -> list[str]:
    errors = []
    names = _names(document["dimensions"]) + _names(document["facts"])
    if not names:
        errors.append("dimension 또는 fact 가 하나 이상 있어야 한다")
    errors.extend(_duplicates("columns", names))
    columns = [
        item["column"]
        for item in [*document["dimensions"], *document["facts"]]
        if "column" in item
    ]
    errors.extend(_duplicates("view columns", columns))
    return errors


def _registry_errors(documents: list[dict]) -> list[str]:
    errors = []
    by_kind = {kind: {} for kind in KIND_DIRS}
    for document in documents:
        kind = document["kind"]
        name = document["name"]
        if name in by_kind[kind]:
            errors.append(f"{kind} {name} 이 중복이다")
            continue
        by_kind[kind][name] = document
    if errors:
        return errors

    for table in by_kind["table"].values():
        for index, foreign_key in enumerate(table["fk"]):
            ref = foreign_key["references"]
            target = by_kind["table"].get(ref["table"])
            if target is None:
                errors.append(f"table {table['name']} fk/{index} 가 없는 테이블 {ref['table']} 을 참조한다")
                continue
            target_columns = set(_names(target["columns"]))
            for column in ref["columns"]:
                if column not in target_columns:
                    errors.append(
                        f"table {table['name']} fk/{index} 참조 컬럼 {column} 이 {ref['table']} 에 없다"
                    )

    for view in by_kind["view"].values():
        errors.extend(_view_reference_errors(view, by_kind))
    errors.extend(_view_cycles(by_kind["view"]))

    for cube in by_kind["cube"].values():
        view = by_kind["view"].get(cube["view"])
        if view is None:
            errors.append(f"cube {cube['name']} 의 view {cube['view']} 가 없다")
            continue
        view_columns = set(_names(view["columns"]))
        for group in ("dimensions", "facts"):
            for item in cube[group]:
                column = item.get("column")
                if column is not None and column not in view_columns:
                    errors.append(
                        f"cube {cube['name']} {group} {item['name']} 의 컬럼 {column} 이 view 에 없다"
                    )

    for metric in by_kind["metric"].values():
        cube = by_kind["cube"].get(metric["cube"])
        if cube is None:
            errors.append(f"metric {metric['name']} 의 cube {metric['cube']} 가 없다")
            continue
        if metric["name"] in set(_names(cube["dimensions"]) + _names(cube["facts"])):
            errors.append(f"metric {metric['name']} 이 cube {cube['name']} 의 컬럼 이름과 겹친다")

    for segment in by_kind["segment"].values():
        if segment["cube"] not in by_kind["cube"]:
            errors.append(f"segment {segment['name']} 의 cube {segment['cube']} 가 없다")
    return errors


def _view_reference_errors(view: dict, by_kind: dict) -> list[str]:
    errors = []
    tables = by_kind["table"]
    views = by_kind["view"]
    source_columns = {}
    for source in view["sources"]:
        if view["combine"] == "select":
            target = views.get(source["name"])
            if target is None:
                errors.append(f"view {view['name']} 의 source view {source['name']} 가 없다")
                continue
            if target["name"] == view["name"]:
                errors.append(f"view {view['name']} 이 자기 자신을 참조한다")
                continue
            source_columns[source["name"]] = set(_names(target["columns"]))
        else:
            target = tables.get(source["name"])
            if target is None:
                errors.append(f"view {view['name']} 의 source table {source['name']} 이 없다")
                continue
            source_columns[source["name"]] = set(_names(target["columns"]))
    for column in view["columns"]:
        for item in column["mappings"]:
            columns = source_columns.get(item["source"])
            if columns is not None and item["column"] not in columns:
                errors.append(
                    f"view {view['name']} 컬럼 {column['name']} 의 {item['source']}.{item['column']} 이 없다"
                )
    if view["combine"] == "join":
        for index, join in enumerate(view["joins"]):
            for side, column_key in (("left", "left_column"), ("right", "right_column")):
                columns = source_columns.get(join[side])
                for item in join["on"]:
                    if columns is not None and item[column_key] not in columns:
                        errors.append(
                            f"view {view['name']} joins/{index} {column_key} {item[column_key]} 이 {join[side]} 에 없다"
                        )
    return errors


def _view_cycles(views: dict) -> list[str]:
    errors = []
    visiting = set()
    visited = set()

    def walk(name: str) -> None:
        if name in visiting:
            errors.append(f"view {name} 참조가 순환한다")
            return
        if name in visited or name not in views:
            return
        visiting.add(name)
        view = views[name]
        if view["combine"] == "select":
            for source in view["sources"]:
                walk(source["name"])
        visiting.remove(name)
        visited.add(name)

    for name in views:
        walk(name)
    return errors


def _names(items: list[dict]) -> list[str]:
    return [item["name"] for item in items]


def _duplicates(label: str, names: list[str]) -> list[str]:
    seen = set()
    errors = []
    for name in names:
        if name in seen:
            errors.append(f"{label} {name} 이 중복이다")
        seen.add(name)
    return errors


def _prefix(label: str, errors: list[str]) -> list[str]:
    if not label:
        return errors
    return [f"{label}: {error}" for error in errors]


def main(argv: list[str]) -> int:
    if len(argv) != 2:
        print("usage: python -m sql_mcp_semantic.validate <file-or-directory>", file=sys.stderr)
        return 2
    target = Path(argv[1])
    try:
        if target.is_dir():
            validate_tree(target)
        else:
            validate_file(target)
    except SchemaError as exc:
        print(str(exc), file=sys.stderr)
        return 1
    print("ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
