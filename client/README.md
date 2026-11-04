# client — LangChain + sql-mcp (BIRD)

BIRD Mini-Dev 질문 한 건을 골라 LangChain v1 `create_agent()`로 sql-mcp MCP 툴을 호출하는 예제입니다. **sql-mcp는 별도로 HTTP 기동**하고, 이 패키지는 연결만 합니다.

에이전트 가이드: [AGENTS.md](AGENTS.md). 계획: [ROADMAP.md](ROADMAP.md). 내부 설계: [DESIGN.md](DESIGN.md).

## 준비

```bash
# 1) SQLite 타깃 + 질문-SQL JSON (client/ 기준)
./bird/fetch_bird.sh --small
# → bird/<db>/*.sqlite
# → bird/bird_qa/mini_dev_sqlite.json

# 2) Python 3.12+
uv venv --python 3.12 .venv
source .venv/bin/activate
pip install -r requirements.txt
# 기본 모델: Ollama gemma4:e2b. OpenAI면 OPENAI_API_KEY + --model openai:…
```

## 실행

```bash
# 터미널 1 — sql-mcp HTTP (env가 config·semantic.db·DSN 설정)
set -a && source ./bird/env.bird.example && set +a
# 모노레포 루트: cargo run -p sql-mcp
# 분리 후: sql-mcp   # PATH 바이너리

# 터미널 2
cd client && source .venv/bin/activate   # 이미 client/면 cd 생략
python agent_bird.py --db superhero --show-gold
```

옵션: `--question-id`, `--index`, `--model ollama:gemma4:e2b`(기본) / `openai:gpt-4o-mini`, `--url http://127.0.0.1:8080/mcp`.

### eval.json (개선용)

```bash
source .venv/bin/activate
python run_eval.py --db superhero --limit 3 --skip-agent
python run_eval.py --url http://127.0.0.1:8090/mcp --concurrency 8
# → eval/eval.json
# 채점·문항별 md · EX FAIL SQL: DESIGN.md Commands
```

모노레포 Cursor/VS Code: `Debug BIRD: sql-mcp + agent`(compound).
