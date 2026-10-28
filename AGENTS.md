# AGENTS.md
 
This file provides guidance to AI coding assistants (Claude Code, Codex, Gemini, ...) when working with code in this repository. `CLAUDE.md`와 `GEMINI.md`는 이 파일로의 심볼릭 링크다 — 정본은 `AGENTS.md` 하나.
 
에이전트·기여자가 **무엇을 어디서 읽고 어떻게 실행하는지**만 담는다. 불변 규칙은 [ARCHITECTURE.md](ARCHITECTURE.md), 일정은 [ROADMAP.md](ROADMAP.md).
 
## 1. Documentation layout (문서 용도)
 
각 문서는 하나의 명확한 용도만 가진다. 같은 내용을 여러 문서에 중복하지 않는다. 한쪽을 고칠 때 다른 쪽이 같이 바뀌어야 한다면 잘못 나눈 것이므로 합치거나 한쪽이 다른 쪽을 참조하게 만든다.
 
| 파일 | 용도 | 위치 |
|------|------|------|
| `AGENTS.md` (이 파일, 정본) ← `CLAUDE.md`, `GEMINI.md` 심볼릭 | 수행 방법 + 문서 레이아웃 + 현황 | 루트 |
| `ARCHITECTURE.md` | **계약사항(불변 규칙)** + 컴포넌트 *간* 인터페이스 형태(스키마·레이아웃·이벤트) | 루트 |
| `README.md` | **사용자** 온보딩: Cursor/Claude 등에 설치 · 메타 구성 · DW 질의 (기여자 온보딩 아님) | 루트 |
| `ROADMAP.md` | 수행 계획(마일스톤·순서·미결정 항목) | 루트 |
| `<comp>/DESIGN.md` | 컴포넌트 *내부* 설계 + `## Commands` (빌드/실행/테스트) | `server/` (M1+), `semantic/`, `client/`, `semantic-ui/`. 조회 Admin API는 취소. 작성 HTTP `/api`는 D30 |
| `deploy/SETUP.md` | 배포 런북 | `deploy/` |
| `.github/workflows/ci.yml` | CI: PR/push `semantic-ui` `npm run build` 후 `cargo test -p sql-mcp --locked` | `.github/workflows/` |
| `.github/workflows/cd.yml` | CD: master CI 성공 후 UI 빌드 + linux/windows/macos release → GitHub Release `current` | `.github/workflows/` |
| `deploy/k8s/base/` | Kustomize 베이스 | `deploy/k8s/base/` |
| `target_db/README.md` | BIRD Mini-Dev SQLite 로컬 타깃 + Q-SQL(`bird_qa/`) · meta JSON · `semantic-map.md` 생성 | `target_db/` |
| `target_db/<db>/semantic_plan.yaml` | 허브(팩트) 정본 — `generate_meta`가 이 목록만 view+cube로 만듦 (`{db}_all` 없음). git 추적 | `target_db/` |
| `client/DESIGN.md` · `client/README.md` | LangChain v1 `create_agent` + sql-mcp 예제 (BIRD 질문) | `client/` |
| `client/docs/eval_report.md` · `client/docs/eval.md` | BIRD eval **요약** / **문항별**(Gold SQL·rows vs Agent SQL·rows) | `client/docs/` |
| `client/docs/fail_plan.md` | D29 EX FAIL 170건 **문항별** 개선 계획(패치 묶음 + qid) | `client/docs/` |
| `server/spec.md` | 시맨틱 SQL 6단계 컴파일·타입별 `meta_*` 상세 | `server/` |
| `skills/` | 코딩 에이전트용 Agent Skills 정본: `semantic-table-view` → `semantic-cube-metric-segment` → (`semantic-from-qa` \| `semantic-from-eval`) → `semantic-register`. MCP Resource(`https://sql-mcp.local/skills/…`). Cursor는 `.cursor/skills` → `../skills` | `skills/` |
| `client/docs/change_semantic.md` | BIRD eval 기반 시맨틱 메타 수정 계획(결정됨; `generate_meta`+`semantic_plan.yaml`+`enrich_meta_from_qa`) | `client/docs/` |
| `client/docs/semantic_layer_ui.svg` | 시맨틱 작성 UI 레이아웃 초안(타깃 선택 · Physical/Cubes · 보기 전용 다이어그램 · meta JSON 편집) | `client/docs/` |
| `semantic-ui/DESIGN.md` | 작성 화면(Vite React). HTTP `/api` · 정적 `GET /ui` | `semantic-ui/` |

#### 규칙:

- **ARCHITECTURE.md §1 (계약) vs §2 이후 (형태).** §1은 "지켜야 하는 규칙(왜)" — 짧고 단정적. §2 이후는 "그 규칙을 구현하는 모양(어떻게)" — 스키마·필드·이벤트 목록. 규칙이 바뀌면 §1을 먼저 고치고 §2 이후를 따라 고친다. 두 부분을 다른 파일로 쪼개면 동기화 부담만 늘어 단일 문서로 둔다.
- **ARCHITECTURE.md vs `<comp>/DESIGN.md`.** ARCHITECTURE는 컴포넌트 *간*, 서브폴더 DESIGN은 해당 컴포넌트 *내부*. 두 쪽에 같은 내용을 적지 않는다.
- **README.md vs ARCHITECTURE.md / DESIGN.md.** README는 **인간 사용자**(MCP 설치·메타·DW 질의)용이다. 기여자 온보딩은 이 파일. 계약·내부 설계는 README에 복사하지 않고 [ARCHITECTURE.md](ARCHITECTURE.md)·`<comp>/DESIGN.md`로 **링크만** 한다. 
- **배포 가능한 패키지·실행 하네스**에는 README를 둘 수 있다 — 해당 디렉터리 **로컬 사용법만**.
- 파일이 새로 생기거나 용도가 바뀌면 위 표를 즉시 갱신한다.
- 배포 런북은 `deploy/SETUP.md`, Kustomize 베이스는 `deploy/k8s/base/`, CI는 `.github/workflows/ci.yml`, CD는 `.github/workflows/cd.yml` (Release `current`). 빈 stub을 만들지 않는다.
 
## 2. 수행 방법 (How we work in this repo)
 
- 계획·설계·구현 변경은 해당 문서를 먼저(또는 함께) 고친다: 계획 변경 → `ROADMAP.md`, 설계·규칙 변경 → `ARCHITECTURE.md`(또는 해당 컴포넌트의 `DESIGN.md`), 워크플로 변경 → 이 파일.
- 코드가 처음 들어오는 컴포넌트는 그 폴더의 `DESIGN.md`를 함께 만들고, 이 파일의 §1 표 또는 `ARCHITECTURE.md` §1 계약사항을 필요 시 갱신한다.
- 컴포넌트에 첫 코드가 들어오면, 해당 폴더의 `DESIGN.md`에 **`## Commands`** 섹션을 추가해 빌드/실행/테스트 방법을 기록한다. 그 전까지는 비워둔다(존재하지 않는 명령을 만들어 적지 않는다).
- 개발은 TDD 방식으로 진행한다. (코드 스켈레톤 -> 테스트 코드 -> 기능 구현)
- 한국어/영어 혼용을 허용한다. 한 문서 내 일관성만 지킨다(현재 AGENTS/ARCHITECTURE/ROADMAP/DESIGN은 한국어 본문 + 영어 식별자).
 
## 3. Status
 
M0–M5 완료 + **D30:** 메타 CRUD·물리 카탈로그는 HTTP `/api`, MCP는 조회만, `SQL_MCP_MODE` 없음. **D26:** metric/segment는 cube 컬럼이 아님(`describe_table.examples`만) · `execute_sql`은 dim/fact ad-hoc · 읽기 ANSI SELECT(서브쿼리·CTE·윈도우·집합연산) · FROM `cube.physical`/`AS alias`는 첫 식별자를 큐브로 라우팅. **D27:** 리터럴-only 프로젝션 → existence dim rewrite · `SELECT *` 거절. **D28:** dim/fact `expression`=스칼라 · fact `additive: false`는 SUM/AVG/COUNT(col) 거절. **D29:** `viewJoin.relationship` + cube `grain`=허브 pk. 팬아웃·캐즘 거절은 `SQL_MCP_GRAIN_GUARD`(기본 끔, 켠 DB 없음). 타입별 `meta_*` · 요약뷰/join·union pruning · 전 계층 RBAC · transport=HTTP+stdio. table/view/cube `target` · DW 표면 `list_databases`/`list_tables`/`describe_table`=cube(D20) · `search_columns`는 dim/fact만. 타깃: Postgres·MySQL·SQLite·Oracle. **D21** DialectProfile · **D22** physical_ident · **D23** Grouped/Ungrouped · **D24** metric `group_by`=기본 grain(allowlist 폐기) · **D25** 표준 날짜 편의(`CURRENT_DATE`/`NOW`/`DATEDIFF`/`EXTRACT`). 계약 [ARCHITECTURE.md](ARCHITECTURE.md), 계획 [ROADMAP.md](ROADMAP.md), 파이프라인 [server/spec.md](server/spec.md).
