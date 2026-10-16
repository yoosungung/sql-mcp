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

## Meta JSON

물리 SQLite를 MCP 없이 읽어 `document.schema.json` 골격 JSON을 만든다(코딩 에이전트 스킬과 동일 계약). gitignore — 로컬 생성물.

```text
python3 ./target_db/generate_meta.py
# → target_db/<db_id>/meta/<name>.json   (table | view | cube | metric)
```

- **table** / **view** / **cube**: `target`=`{db_id}`(MCP `TargetConfig.id`와 동일). table은 `schema`=`main`, 필터 비움. FK `to`가 비면 참조 테이블 PK로 채운다. (`as_code`는 table 컬럼이 아니라 cube dimension에만 둔다.)
- **view**: FK + (컬럼명=타 테이블 PK) + (동일 FK 컬럼 패턴 전파)로 그래프를 만들고, 연결 요소마다 hub 기준 spanning-tree `inner` join 하나. 전부 연결되면 `{db_id}_all`, 아니면 `{db_id}_{hub}`. sources ≥ 2.
- **cube** / **metric**: view당 휴리스틱 초안(자연 이름은 cube/dim/fact/metric 우선; 충돌 시 table/view/view컬럼에 `_table`/`_view`/`_col`). dimension·fact·metric에 `description` 필수. 등록 전 검토 권장.
- self-FK·동일 테이블 다중 FK는 source 이름 제약으로 스패닝에 한 간선만 쓴다.
- 일부 BIRD 컬럼명(공백·기호·하이픈)은 스키마 `name` 패턴(`^[A-Za-z_][A-Za-z0-9_]*$`)을 벗어나며, 물리 이름을 그대로 둔다(해당 컬럼은 cube 멤버에서 생략). 현재 `california_schools`의 `frpm`·`*_all` view와 `thrombosis_prediction`의 일부 table/view가 이에 해당해 schema 전체 통과는 안 된다.
- **MCP `metadata_*` 등록은 이 스크립트가 하지 않는다.**

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
