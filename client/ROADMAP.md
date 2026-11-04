# ROADMAP.md — client (BIRD DW agent)

이 패키지의 수행 계획. sql-mcp 제품 로드맵은 상위 [ROADMAP.md](../ROADMAP.md). 계약은 상위 [ARCHITECTURE.md](../ARCHITECTURE.md).

## 목표

LangChain DW agent로 BIRD Mini-Dev를 평가하고, `bird/` 시맨틱 레이어(`semantic_plan.yaml` · meta · `semantic.db`)를 개선한다. sql-mcp는 외부 의존(MCP HTTP + `/api/meta`).

## 완료

- Mini-Dev fetch · Q-SQL 로더 · `agent_bird.py`(HTTP)
- `run_eval.py` → `eval/eval.json` · EX 채점 · `docs/eval.md` / `eval_report.md`
- `write_eval_sql.py` · `hub_grain.py` dry_run 그레인 게이트
- 허브 정본 `bird/<db>/semantic_plan.yaml` + `generate_meta` / `enrich_meta_from_qa` / `register_meta`
- BIRD 전용 카탈로그 `bird/semantic.db` · 타깃 설정 `bird/config.bird.toml`(제품 `./data/semantic.db`와 분리)
- D29 플래그 끈 채 SMALL_DBS 286 재eval 기준선 EX **116/286** R3 (그레인 34→30). 요약 [docs/eval_report.md](docs/eval_report.md)(student_club 별도 스냅샷 포함)

## 남은 일 — D29 거절 켠 뒤 BIRD 게이트

상위 제품의 팬아웃·캐즘 거절(`SQL_MCP_GRAIN_GUARD`)을 DB별로 켠 뒤, **이 패키지에서** 검증한다.

전제: 에이전트는 `semantic_plan.yaml`을 직접 읽지 않는다. 등록 큐브를 `execute_sql`로만 조회한다. 컷오버 전 검증은 `hub_grain.py` dry_run(컴파일 물리 테이블 ⊆ 허브 팩트+차원).

1. 바꾼 DB만 eval. 그레인 FAIL이 줄지 않거나 해당 DB EX가 R3(`eval/` 116/286의 그 DB 부분집합)보다 내려가면 그 DB의 거절은 끈다.
2. 286 전체는 DB별 게이트 뒤에만 돌린다.
3. `pk`가 비어 grain을 못 단 큐브 추적: `throm_examination`, `hero_attribute`, `hero_power`(메타·플랜 쪽).

- [ ] 거절 켠 DB별 게이트. 그레인 FAIL↓ · 해당 DB EX ≥ R3일 때만 유지
- [ ] 위 empty-grain 큐브 메타 보강 또는 플랜 조정

## 남은 일 — 저장소 분리

- [ ] sql-mcp를 배포 바이너리/컨테이너만 가정하도록 문서·경로 정리(모노레포 `../server` 의존 제거)
- [ ] CI: `pytest` + (선택) eval 스모크

## 작업 규칙

1. 문서 레이아웃·워크플로 → [AGENTS.md](AGENTS.md) · 내부 설계 → [DESIGN.md](DESIGN.md) · BIRD 파이프라인 → [bird/README.md](bird/README.md) · 계획 → 이 파일
2. 스켈레톤 → 테스트 → 구현 (`pytest` in `tests/` · `bird/`)
