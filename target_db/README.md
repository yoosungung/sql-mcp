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
```

- **table** / **view** / **cube**: `target`=`{db_id}`(MCP `list_databases.name` = `TargetConfig.id`와 동일). table은 `schema`=`main`, 필터 비움. FK `to`가 비면 참조 테이블 PK로 채운다. (`as_code`는 table 컬럼이 아니라 cube dimension에만 둔다.)
- **view**: FK + (컬럼명=타 테이블 PK, **단 child 자신의 PK 컬럼 제외**) + (동일 FK 컬럼 패턴 전파, PK→PK 제외)로 그래프를 만들고, 연결 요소마다 hub 기준 spanning-tree `inner` join 하나. 전부 연결되면 `{db_id}_all`, 아니면 `{db_id}_{hub}`. sources ≥ 2. (PK끼리 `id=id`를 붙이면 hero–power 같은 M:N이 깨진다.)
- **cube** / **metric**: view당 휴리스틱 초안(자연 이름은 cube/dim/fact/metric 우선; 충돌 시 table/view/view컬럼에 `_table`/`_view`/`_col`). dimension·fact·metric에 `description` 필수. 등록 전 검토 권장.
- self-FK·동일 테이블 다중 FK는 source 이름 제약으로 스패닝에 한 간선만 쓴다.
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
