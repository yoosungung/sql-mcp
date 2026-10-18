# client — LangChain 에이전트 예제

BIRD Mini-Dev **질문-SQL**을 넣고, LangChain v1 `create_agent()` + `MCPAdapter`로 **sql-mcp**를 호출하는 최소 클라이언트.

## 범위

- Q-SQL 로더: `bird_qa.py` ← `target_db/bird_qa/mini_dev_sqlite.json` (`fetch_bird.sh`가 zip에서 추출)
- 에이전트: `agent_bird.py` — **HTTP만** (sql-mcp 기동은 client 밖). `SYSTEM`에 D23(통합 Grouped `(A)`/`(B)` 혼합·파생 수식·Ungrouped `(C)`·`SELECT DISTINCT` Cube JOIN)·허용 함수(`/`·`date_trunc`)·`ORDER BY`·이스케이프·재시도 규칙 고정(eval `dw_agent` 실패 대응)
- 평가 스냅샷: `bird_eval.py` + `run_eval.py` → `client/eval/eval.json`
- EX 채점: `bird_eval.score_eval_doc` — 마지막 성공 `execute_sql` rows vs `gold_rows` (unordered set). 실패는 `dw_agent` / `sql_mcp_code` / `semantic_layer`로 분류 → `client/eval/score_report.json`

## 흐름

```text
fetch_bird.sh → bird_qa/mini_dev_sqlite.json
                     ↓
              bird_qa.load_questions
                     ↓
         create_agent(model, MCP tools)
                     ↓
         sql-mcp HTTP (이미 기동된 서버)
                     ↓
    run_eval.py → eval.json  (question/evidence/gold_sql/gold_rows + agent)
```

BIRD gold `SQL`은 **물리 SELECT**이므로 `execute_sql`(시맨틱-only)에 그대로 넣지 않는다. 참고·비교용(`--show-gold`). gold 행은 `target_db/<db>/<db>.sqlite`에 직접 실행해 `gold_rows`로 저장한다.

### eval.json 항목

| 필드 | 설명 |
|------|------|
| `question` / `evidence` / `gold_sql` | BIRD Mini-Dev |
| `gold_rows` / `gold_error` | 물리 SQLite 실행 결과 |
| `agent.final` / `agent.tool_calls` | DW agent 최종 답 + MCP tool 궤적 |

### EX · 실패 3분류

| bucket | 의미 |
|--------|------|
| `dw_agent` | 시맨틱 표면으로 답이 가능한데 에이전트 SQL/로직이 틀림 |
| `sql_mcp_code` | 유효해 보이는 시맨틱 SQL이 파서·컴파일러에서 거절/오류 |
| `semantic_layer` | 메타(metric/dimension/view) 부족으로 표현 불가 · 또는 컴파일된 물리 SQL의 JOIN이 gold 대비 과도/추가 |

`tool_calls_from_messages`는 `execute_sql` 결과를 `log_max` 문자로 자르지 않고 rows만 `row_limit`(기본 500, `run_gold_sql`과 동일)으로 보존한다. EX 실패 시 `classify_failure`는 gold SQL에 JOIN이 있다는 사실만으로 `semantic_layer`로 묶지 않고, dry_run 컴파일 SQL·행 집합의 조인/필터/타입 차이로 세분한다.

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

# eval.json (개선 루프용 스냅샷; 기본 병렬 8)
python run_eval.py --db superhero --limit 3 --skip-agent   # gold만
python run_eval.py --url http://127.0.0.1:8090/mcp --concurrency 8
# → client/eval/eval.json

# EX 채점 + 실패 3분류
python -c "import json; from pathlib import Path; from bird_eval import score_eval_doc; \
d=json.loads(Path('eval/eval.json').read_text()); \
Path('eval/score_report.json').write_text(json.dumps(score_eval_doc(d),indent=2,ensure_ascii=False)+chr(10))"

# Cursor/VS Code 디버그 (.vscode/launch.json)
# - Debug sql-mcp (BIRD)              — :8090
# - Debug client agent (BIRD HTTP)    — 위 서버 필요
# - Debug BIRD: sql-mcp + agent       — compound
```
