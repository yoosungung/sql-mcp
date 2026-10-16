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

물리 스키마를 `document.schema.json` 골격으로 뽑는다.

```text
python3 ./target_db/generate_meta.py
# → target_db/<db_id>/meta/<table>.json
# → target_db/<db_id>/meta/<view>.json   (연결 요소당 최대 join)
```

- **table**: `schema`=`main`, 필터 비움. FK `to`가 비면 참조 테이블 PK로 채운다.
- **view**: FK + (컬럼명=타 테이블 PK) + (동일 FK 컬럼 패턴 전파)로 그래프를 만들고, 연결 요소마다 hub 기준 spanning-tree `inner` join 하나. 전부 연결되면 `{db_id}_all`, 아니면 `{db_id}_{hub}`.
- self-FK·동일 테이블 다중 FK는 source 이름 제약으로 스패닝에 한 간선만 쓴다.
- 일부 BIRD 컬럼명(공백·기호)은 스키마 `name` 패턴을 벗어나며, 물리 이름을 그대로 둔다.

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
