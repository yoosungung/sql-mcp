# AGENTS.md — client (BIRD DW agent)

이 디렉터리는 **sql-mcp를 쓰는 LangChain DW agent 예제**와 **BIRD Mini-Dev 평가 하네스**다. 차후 별도 저장소로 분리할 패키지이므로, 에이전트·BIRD 데이터·BIRD 시맨틱 레이어·eval에 필요한 문서·실행 방법만 여기에 둔다.

sql-mcp(서버 코드·제품 스킬·제품 기본 시맨틱 DB)는 **외부 의존**이다. 이 패키지는 MCP HTTP에 붙고, BIRD 카탈로그는 `bird/semantic.db`만 쓴다.

모노레포에서는 상위 [ARCHITECTURE.md](../ARCHITECTURE.md) · [server/DESIGN.md](../server/DESIGN.md) · [server/spec.md](../server/spec.md)를 참조한다. 분리 후에는 배포된 sql-mcp 바이너리/컨테이너 + 공개 계약만 가정한다.

## 1. Documentation layout

| 파일 | 용도 |
|------|------|
| `AGENTS.md` (이 파일) | 무엇을 어디서 읽고 어떻게 eval 하는지 |
| `ROADMAP.md` | 이 패키지 수행 계획·eval 게이트 |
| `DESIGN.md` | 에이전트·채점·미들웨어 **내부** 설계 + `## Commands` |
| `README.md` | 사람용 quickstart |
| `bird/README.md` | BIRD SQLite · Q-SQL · meta · 등록 |
| `bird/config.bird.toml` | sql-mcp 타깃 설정 (이 패키지 소유) |
| `bird/env.bird.example` | DSN + `SQL_MCP_CONFIG` + `SQL_MCP_SEMANTIC_PATH` |
| `bird/<db>/semantic_plan.yaml` | 허브(팩트) 정본 (`{db}_all` 없음). git 추적 |
| `docs/eval_report.md` · `docs/eval.md` | eval 요약 / 문항별 |
| `eval/` | 런타임 산출. gitignore |

규칙: DESIGN·README·bird/README에 같은 본문을 두지 않는다. sql-mcp 계약·컴파일 규칙은 링크만. 파일이 생기거나 용도가 바뀌면 이 표를 갱신한다.

## 2. 소유 경계

**이 패키지** — `agent_bird.py` · `bird_qa.py` · `bird_eval.py` · `run_eval.py` · `write_eval_sql.py` · `bird/`(SQLite·Q-SQL·플랜·meta 스크립트·`config.bird.toml`·`semantic.db`) · `docs/` · `tests/` · 이 문서들.

**sql-mcp (외부)** — MCP 조회 · HTTP `/api/meta` · 컴파일러 · 제품 시맨틱 경로(BIRD와 분리).

## 3. 수행 방법

- 계획 → `ROADMAP.md` · 설계·에이전트 동작 → `DESIGN.md` · 문서 레이아웃·워크플로 → 이 파일 · BIRD 파이프라인 → `bird/README.md`.
- TDD: `pytest` (`tests/` + `bird/test_*.py`).
- sql-mcp는 client가 기동하지 않는다. `bird/env.bird.example` source 후 서버만 별도 기동.
- gold SQL은 물리 SELECT → `execute_sql`에 넣지 않음. gold 행은 `bird/<db>/<db>.sqlite`.
- 명령 정본: [DESIGN.md `## Commands`](DESIGN.md).

### Eval 루프

```text
bird/fetch_bird.sh → bird_qa + *.sqlite
     → generate_meta / enrich / register_meta  (→ bird/semantic.db)
     → sql-mcp HTTP (env.bird.example)
     → run_eval.py → eval/eval.json
     → score / write_eval_md / write_eval_sql.py
```

실패 버킷: `dw_agent` · `sql_mcp_code` · `semantic_layer` (DESIGN.md).

## 4. Status

DW agent + EX 채점 동작. 카탈로그 `bird/semantic.db`. 허브 정본 `semantic_plan.yaml`. 요약 [docs/eval_report.md](docs/eval_report.md). 남은 게이트 [ROADMAP.md](ROADMAP.md).
