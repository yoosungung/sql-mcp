# sql-mcp

AI 코딩 에이전트(Cursor, Claude 등)가 **데이터 웨어하우스(DW)** 를 시맨틱 계층으로 질의하게 해주는 MCP 서버입니다.

테이블이 수십·수백 개일 때 물리 스키마를 그대로 넘기면 에이전트가 길을 잃기 쉽습니다. sql-mcp는 table → view → cube → metric/segment 메타를 등록한 뒤, 에이전트가 **시맨틱 SQL**만 쓰면 서버가 물리 SELECT로 컴파일해 실행합니다. **테이블이 대략 100개 이상**인 DW에서 특히 효과가 큽니다.

## 이런 분께

- Cursor / Claude Desktop / Claude Code에 MCP로 붙여 DW를 묻고 싶을 때
- 코딩 에이전트로 메타(JSON)를 구성·등록하고, 이후 질의는 cube·metric 기준으로 하고 싶을 때
- 타깃은 **Postgres** 또는 **SQLite**(읽기 SELECT만)

기여자·내부 설계는 [AGENTS.md](AGENTS.md) · [ARCHITECTURE.md](ARCHITECTURE.md) · [server/DESIGN.md](server/DESIGN.md)를 보세요.

## 한눈에 보는 흐름

```text
1. 바이너리 준비 + 타깃 DW DSN / 시맨틱 DB 경로 설정
2. Cursor · Claude 등에 MCP(stdio 또는 HTTP)로 연결
3. 코딩 에이전트 + skills/ 로 메타 초안 → metadata_* 등록
4. 질의: search_columns / sample_column / execute_sql (시맨틱 SQL)
```

| 단계 | 하는 일 | 누가 |
|------|---------|------|
| 설치 | 서버 기동·클라이언트 MCP 설정 | 사용자 |
| 메타 구성 | table/view/cube/metric/segment JSON | 코딩 에이전트(+ skills) |
| DW 사용 | 시맨틱 검색·샘플·SELECT | 코딩/분석 에이전트 |

비밀번호·DSN은 채팅에 넣지 말고, 기동 환경 변수로만 주입하세요.

## 1. 설치 (서버)

사전: [Rust](https://rustup.rs/) (toolchain은 `rust-toolchain.toml`), 타깃 DW 읽기 권한.

```bash
git clone <this-repo> && cd sql-mcp
cargo build -p sql-mcp --release
# 산출물: target/release/sql-mcp
```

설정 예: [`server/examples/config.example.toml`](server/examples/config.example.toml)

```bash
export SQL_MCP_SEMANTIC_PATH=./data/semantic.db
export SQL_MCP_CONFIG=./server/examples/config.example.toml
export SQL_MCP_TARGET_DW_URL='postgres://USER:PASS@HOST:5432/DB'   # config의 url_env와 맞출 것

# 로컬 자식 프로세스용 (Cursor / Claude Desktop 권장)
./target/release/sql-mcp --stdio

# 또는 HTTP (원격·공유). MCP: POST http://127.0.0.1:8080/mcp
./target/release/sql-mcp
```

같은 `SQL_MCP_SEMANTIC_PATH`를 HTTP와 stdio가 **동시에** 열지 마세요(파일 락).

컨테이너·K8s: [deploy/SETUP.md](deploy/SETUP.md). 로컬 BIRD SQLite 샘플: [target_db/README.md](target_db/README.md).

## 2. 클라이언트에 붙이기

공통으로 **stdio**는 클라이언트가 `sql-mcp --stdio`를 자식으로 띄웁니다. 템플릿: [`server/examples/mcp.stdio.example.json`](server/examples/mcp.stdio.example.json) (`cwd`·경로를 본인 환경으로 바꾸세요).

### Cursor

프로젝트 `.cursor/mcp.json` 또는 전역 `~/.cursor/mcp.json`:

```json
{
  "mcpServers": {
    "sql-mcp": {
      "type": "stdio",
      "command": "/ABS/PATH/TO/sql-mcp/target/release/sql-mcp",
      "args": ["--stdio"],
      "env": {
        "SQL_MCP_SEMANTIC_PATH": "/ABS/PATH/TO/data/semantic.db",
        "SQL_MCP_CONFIG": "/ABS/PATH/TO/sql-mcp/server/examples/config.example.toml",
        "SQL_MCP_TARGET_DW_URL": "postgres://..."
      }
    }
  }
}
```

저장 후 MCP를 다시 로드합니다. 원격이면 `url`로 HTTP 엔드포인트를 지정할 수 있습니다([Cursor MCP 문서](https://cursor.com/docs/mcp)).

이 저장소를 Cursor에서 열면 `.cursor/skills` → `skills/` 심볼릭으로 시맨틱 등록 스킬이 보입니다.

### Claude Desktop

`claude_desktop_config.json`에 동일하게 `mcpServers.sql-mcp`를 넣습니다.

- macOS: `~/Library/Application Support/Claude/claude_desktop_config.json`
- Windows: `%APPDATA%\Claude\claude_desktop_config.json`

앱을 완전히 종료했다가 다시 켭니다. ([로컬 MCP 연결](https://modelcontextprotocol.io/docs/develop/connect-local-servers))

### Claude Code

프로젝트 `.mcp.json` 또는 `claude mcp add`로 stdio/HTTP를 등록합니다. Desktop과 설정 파일이 다를 수 있으니 Code 쪽 설정도 확인하세요.

## 3. 메타 구성

물리 DW 카탈로그를 에이전트가 직접 읽고(또는 스크립트로 골격 생성), 아래 순서로 JSON을 만든 뒤 MCP `metadata_*`로 등록합니다.

```text
table → view → cube → metric | segment
```

| 스킬 | 역할 |
|------|------|
| [`skills/semantic-table-view`](skills/semantic-table-view/SKILL.md) | 물리 테이블 → table/view 초안 |
| [`skills/semantic-cube-metric-segment`](skills/semantic-cube-metric-segment/SKILL.md) | view 위 cube·metric·segment |
| [`skills/semantic-register`](skills/semantic-register/SKILL.md) | schema 검증 + `metadata_create` / `update` |

서버가 같은 내용을 MCP Resource로도 제공합니다 (`document.schema.json`, `skills/…`). 필드·연결 규칙은 Resource/스킬 `contract.md`와 [ARCHITECTURE.md](ARCHITECTURE.md)를 따르고, README에 스키마를 복사하지 않습니다.

등록 예시는 `skills/*/examples/` 및 [`semantic/`](semantic/DESIGN.md).

## 4. DW 사용 (질의)

메타가 있으면 에이전트는 대략 이렇게 씁니다.

| 툴 | 용도 |
|----|------|
| `search_columns` | 시맨틱 컬럼/멤버 검색 (BM25; 임베딩 설정 시 vector/hybrid) |
| `sample_column` | cube 멤버 샘플(+코드사전) |
| `execute_sql` | **시맨틱 SQL만** → 서버가 물리 SELECT로 컴파일·실행 |
| `list_targets` / `list_tables` / `describe_table` | 타깃·목록 확인 |

`execute_sql`에 자유 물리 SELECT는 없습니다. cube FROM, metric/dimension/segment 기준 질의만 허용됩니다. 역할·필터·마스킹 등 계약은 [ARCHITECTURE.md](ARCHITECTURE.md).

예시 프롬프트:

- 「매출 cube에서 지난달 region별 revenue metric을 구해줘」
- 「status 관련 dimension 찾아 sample 보여줘」

## 문서 맵

| 문서 | 용도 |
|------|------|
| [ARCHITECTURE.md](ARCHITECTURE.md) | 불변 계약·MCP 툴 표면 |
| [skills/](skills/) | 메타 작성·등록 Agent Skills |
| [server/DESIGN.md](server/DESIGN.md) | 빌드·환경 변수·내부 모듈 |
| [deploy/SETUP.md](deploy/SETUP.md) | 바이너리·컨테이너·K8s |
| [AGENTS.md](AGENTS.md) | 기여자 워크플로 (개발자 온보딩) |
| [ROADMAP.md](ROADMAP.md) | 마일스톤 |
