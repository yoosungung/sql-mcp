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
