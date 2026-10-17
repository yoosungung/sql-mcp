# client — LangChain + sql-mcp (BIRD)

BIRD Mini-Dev 질문 한 건을 골라 LangChain v1 `create_agent()`로 sql-mcp MCP 툴을 호출하는 예제입니다. **sql-mcp는 별도로 HTTP 기동**하고, client는 연결만 합니다.

## 준비

```bash
# 1) SQLite 타깃 + 질문-SQL JSON
./target_db/fetch_bird.sh --small
# → target_db/<db>/*.sqlite
# → target_db/bird_qa/mini_dev_sqlite.json

# 2) Python 3.12+
uv venv --python 3.12 client/.venv   # 또는 python3.12 -m venv client/.venv
source client/.venv/bin/activate
pip install -r client/requirements.txt
# 기본 모델: Ollama gemma4:e2b (ollama serve 필요). OpenAI면 OPENAI_API_KEY + --model openai:…
```

## 실행

```bash
# 터미널 1 (저장소 루트) — sql-mcp HTTP
set -a && source ./target_db/env.bird.example && set +a
export SQL_MCP_CONFIG=./server/examples/config.bird.toml
export SQL_MCP_SEMANTIC_PATH=./data/semantic.db
cargo run -p sql-mcp

# 터미널 2
cd client && source .venv/bin/activate
python agent_bird.py --db superhero --show-gold
```

옵션: `--question-id`, `--index`, `--model ollama:gemma4:e2b`(기본) / `openai:gpt-4o-mini`, `--url http://127.0.0.1:8080/mcp`.

### eval.json (개선용)

질문·evidence·gold SQL/행 + DW agent 실행 결과를 한 JSON으로 모읍니다.

```bash
cd client && source .venv/bin/activate
python run_eval.py --db superhero --limit 3 --skip-agent          # gold만
python run_eval.py --url http://127.0.0.1:8090/mcp --concurrency 8  # 전체, 병렬 8
# → client/eval/eval.json
```

Cursor/VS Code: `Debug BIRD: sql-mcp + agent`(compound) 또는 서버(`:8090`) 기동 후 `Debug client agent (BIRD HTTP)`.

내부 설계·테스트: [DESIGN.md](DESIGN.md).
