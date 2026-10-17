# client — LangChain 에이전트 예제

BIRD Mini-Dev **질문-SQL**을 넣고, LangChain v1 `create_agent()` + `MCPAdapter`로 **sql-mcp**를 호출하는 최소 클라이언트.

## 범위

- Q-SQL 로더: `bird_qa.py` ← `target_db/bird_qa/mini_dev_sqlite.json` (`fetch_bird.sh`가 zip에서 추출)
- 에이전트: `agent_bird.py` — **HTTP만** (sql-mcp 기동은 client 밖)
- 평가 하네스·정확도 벤치마크는 범위 밖 (후속)

## 흐름

```text
fetch_bird.sh → bird_qa/mini_dev_sqlite.json
                     ↓
              bird_qa.load_questions
                     ↓
         create_agent(model, MCP tools)
                     ↓
         sql-mcp HTTP (이미 기동된 서버)
```

BIRD gold `SQL`은 **물리 SELECT**이므로 `execute_sql`(시맨틱-only)에 그대로 넣지 않는다. 참고·비교용(`--show-gold`).

## Commands

```text
# 의존성 (Python 3.12+, 저장소 루트 기준)
# uv 권장: uv venv --python 3.12 client/.venv
python3.12 -m venv client/.venv
source client/.venv/bin/activate
pip install -r client/requirements.txt

# 단위 테스트
cd client && pytest

# 데이터 준비
../target_db/fetch_bird.sh --small

# sql-mcp HTTP (다른 터미널, 저장소 루트 — client가 띄우지 않음)
set -a && source ./target_db/env.bird.example && set +a
export SQL_MCP_CONFIG=./server/examples/config.bird.toml
export SQL_MCP_SEMANTIC_PATH=./data/semantic.db
cargo run -p sql-mcp

# 에이전트 (기본: Ollama gemma4:e2b — ollama serve 필요)
cd client
python agent_bird.py --db superhero --show-gold
# stderr에 agent trace(tool_call·ToolMessage). 끄기: --quiet-trace
# OpenAI: --model openai:gpt-4o-mini + OPENAI_API_KEY

# Cursor/VS Code 디버그 (.vscode/launch.json)
# - Debug sql-mcp (BIRD)              — :8090
# - Debug client agent (BIRD HTTP)    — 위 서버 필요
# - Debug BIRD: sql-mcp + agent       — compound
```
