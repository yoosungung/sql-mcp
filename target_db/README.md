# target_db — BIRD Mini-Dev SQLite

로컬 테스트용 **타깃 DB**. [BIRD](https://bird-bench.github.io/) Mini-Dev의 SQLite 파일을 `target_db/<db_id>/<db_id>.sqlite`에 둔다.

## Fetch

```text
./target_db/fetch_bird.sh          # 11개 전부
./target_db/fetch_bird.sh --small  # 스모크용 6개
```

출처: BIRD Mini-Dev 공식 zip  
`https://bird-bench.oss-cn-beijing.aliyuncs.com/minidev.zip`  
([bird-bench/mini_dev](https://github.com/bird-bench/mini_dev)). zip 캐시는 `target_db/.cache/`(gitignore).

`.sqlite`는 gitignore — 스크립트로만 받는다.

같은 zip에서 **질문-SQL**도 추출한다:

```text
target_db/bird_qa/mini_dev_sqlite.json   # NL question + gold SQL (500건)
```

에이전트 예제: [client/README.md](../client/README.md).

## Meta JSON

물리 SQLite를 MCP 없이 읽어 `document.schema.json` 골격 JSON을 만든다(코딩 에이전트 스킬과 동일 계약). gitignore — 로컬 생성물.

```text
python3 ./target_db/generate_meta.py
# → target_db/<db_id>/meta/<name>.json   (table | view | cube | metric)
python3 ./target_db/enrich_meta_from_qa.py
# → evidence 기반 domain metric/segment 추가 (generate_meta 이후; 덮어쓰지 않음)
python3 ./target_db/generate_semantic_map.py
# → target_db/<db_id>/meta/semantic-map.md  (diagram-first ERD + OLAP Mermaid)
python3 ./target_db/generate_semantic_map.py financial   # 단일 DB
```

- **semantic-map.md** (`generate_semantic_map.py`): meta JSON의 읽기 전용 투영. §A Views(Mermaid=테이블명+join · Column mappings에 `(PK)`/`(FK)`) · §B Cubes(표). SoT는 JSON. 테스트: `python3 -m pytest target_db/test_generate_semantic_map.py` (venv의 pytest).

- **table** / **view** / **cube**: `target`=`{db_id}`(MCP `list_databases.name` = `TargetConfig.id`와 동일). table은 `schema`=`main`, 필터 비움. FK `to`가 비면 참조 테이블 PK로 채우며, 선언 순서(컬럼 순서 우선)를 유지한다. 선언 FK가 없어도 컬럼 `{table}_id`/`{Table}Id` → 논리 테이블 `{table}`(또는 `{table}s`)의 `id`/`Id` FK·조인을 추론한다. (`as_code`는 table 컬럼이 아니라 cube dimension에만 둔다.)
- **view**: FK + (`{table}_id`→`{table}.id`) + (컬럼명=타 테이블 PK, **단 child 자신의 PK 컬럼 제외**) + (동일 FK 컬럼 패턴 전파, PK→PK 제외)로 그래프를 만들고, 연결 요소마다 hub 기준 BFS 스패닝 간선 + **사이클을 닫는 추가 FK 간선**을 모두 `inner` join으로 보존한다(컴파일러가 질의 시 최단 경로 서브그래프·스패닝 트리만 방출). 전부 연결되면 `{db_id}_all`, 아니면 `{db_id}_{hub}`. 조인 키 동치류(Equivalence Class) 내 물리 컬럼명이 다르면(`event_id` vs `link_to_event`, `zip_code` vs `zip`) 각 이름을 모두 뷰 컬럼으로 노출하고 상호 매핑을 부여한다.
- **cube** / **metric**: view당 휴리스틱 초안(자연 이름은 cube/dim/fact/metric 우선; 충돌 시 table/view/view컬럼에 `_table`/`_view`/`_col`). cube·dim·fact·metric `description`은 메타 **사용**용이므로 자동 생성 시 비운다(`""`; 생성 출처 문구 금지). 자동 metric의 `group_by`는 모든 컬럼 기반 차원을 포함한다.
- **domain metric/segment** (`enrich_meta_from_qa.py`): `bird_qa/mini_dev_sqlite.json` evidence의 이름 붙은 공식·코호트만 승격(`semantic-from-qa`). `generate_meta`가 meta를 비우므로 항상 그 다음에 실행한다.
- 동일 테이블 쌍 다중 FK는 PK 타깃·컬럼명 일치·짧은 키 순으로 대표 간선을 선택하되, 동일 부모를 2~3회 참조하는 역할(Role-Playing) FK는 **클론 table 없이** view `sources` `{ kind, table, alias? }`로 이중 조인한다(예: `Team` + `alias: away_team`). 소규모 룩업(≤4 cols)은 최대 3역할, 2역할은 더 큰 차원도 허용. 동일 부모 FK가 4개 초과(라인업 슬롯)는 조인 제외·ID만 유지하며, `Player`+`Player_Attributes`는 별도 `player` view/cube로 분리한다.
- BIRD처럼 물리 컬럼에 공백·기호가 있으면 table/`mappings.column`에는 **물리명 그대로**(D22 `physical_ident`). view 컬럼 `name`은 시맨틱 패턴으로 sanitize하고, cube dim/fact는 그 별칭을 쓴다.
- JSON 생성만 하면 MCP에는 안 들어간다. **등록은 `.cursor/mcp.json`의 sql-mcp**(stdio · `SQL_MCP_MODE=author` · 동일 `./data/semantic.db`)로 `metadata_*` / `semantic-register` 스킬을 쓰면 된다. 같은 시맨틱 DB를 HTTP `query` 디버그 세션과 **동시에** 열지 말 것(libSQL 파일 락). `metadata_*` 성공 시 해당 프로세스 캐시는 즉시 갱신된다. (선택) HTTP author가 필요할 때만 `python3 ./target_db/register_meta.py`.

## Run with sql-mcp

저장소 루트에서:

```text
./target_db/fetch_bird.sh --small
set -a && source ./target_db/env.bird.example && set +a
export SQL_MCP_CONFIG=./server/examples/config.bird.toml
export SQL_MCP_SEMANTIC_PATH=./data/semantic.db
cargo run -p sql-mcp
```

DSN 형식: `sqlite:<절대또는상대경로>?mode=ro`  
(서버는 SQLite 타깃을 `libsql`로 연다 — `sqlx` sqlite feature와 이중 링크하지 않음.)
