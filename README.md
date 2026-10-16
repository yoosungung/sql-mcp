# sql-mcp

AI 코딩 에이전트(Cursor, Claude 등)가 **데이터 웨어하우스(DW)** 를 시맨틱 계층으로 질의하게 해주는 MCP 서버입니다.

테이블이 수십·수백 개일 때 물리 스키마를 그대로 넘기면 에이전트가 길을 잃기 쉽습니다. sql-mcp는 table → view → cube → metric/segment 메타를 등록한 뒤, 에이전트가 **시맨틱 SQL**만 쓰면 서버가 물리 SELECT로 컴파일해 실행합니다. **테이블이 대략 100개 이상**인 DW에서 특히 효과가 큽니다.

- Cursor / Claude Desktop / Claude Code에 MCP로 붙여 DW를 물을 때
- 에이전트로 메타(JSON)를 등록하고, 질의는 cube·metric 기준으로 할 때
- 타깃: **Postgres** 또는 **SQLite**(읽기 SELECT만)

비밀번호·DSN은 채팅에 넣지 말고, 기동 환경 변수로만 주입하세요.

## 한눈에 보는 흐름

```text
1. Release에서 sql-mcp 받기 (바이너리 또는 이미지)
2. config.toml + DW DSN / 시맨틱 DB 경로 설정
3. Cursor · Claude MCP에 연결
4. 에이전트로 메타 등록 (서버가 스킬·스키마 Resource 제공)
5. search_columns / sample_column / execute_sql 로 질의
```

## 1. 받기

Release에서 OS에 맞는 **`sql-mcp` 바이너리** 또는 **컨테이너 이미지**를 받습니다. 저장소 클론·Rust 빌드는 필요 없습니다.

### 바이너리

1. 받은 파일을 실행 가능한 위치에 둡니다 (예: `~/bin/sql-mcp`).
2. 실행 권한을 줍니다: `chmod +x ~/bin/sql-mcp`
3. 같은 폴더(또는 원하는 경로)에 `config.toml`을 만듭니다.

```toml
# config.toml — 최소 예
listen = "127.0.0.1:8080"
max_rows = 1000
sample_limit = 20
timeout_ms = 30000

[[targets]]
id = "dw"
name = "Data Warehouse"
engine = "postgres"          # 또는 "sqlite"
url_env = "SQL_MCP_TARGET_DW_URL"
```

```bash
export SQL_MCP_SEMANTIC_PATH="$HOME/.sql-mcp/semantic.db"
export SQL_MCP_CONFIG="$HOME/bin/config.toml"
export SQL_MCP_TARGET_DW_URL='postgres://USER:PASS@HOST:5432/DB'

# 로컬 에이전트용 (Cursor / Claude Desktop 권장)
~/bin/sql-mcp --stdio

# 또는 HTTP (원격·공유). MCP: POST http://127.0.0.1:8080/mcp
~/bin/sql-mcp
```

시맨틱 DB 디렉터리는 미리 만들어 두세요 (`mkdir -p ~/.sql-mcp`). 같은 `SQL_MCP_SEMANTIC_PATH`를 HTTP와 stdio가 **동시에** 열지 마세요.

### 컨테이너 이미지

Release 이미지를 pull한 뒤, `config.toml`과 시맨틱 DB 볼륨을 마운트합니다.

```bash
docker run --rm -p 8080:8080 \
  -e SQL_MCP_SEMANTIC_PATH=/data/semantic.db \
  -e SQL_MCP_CONFIG=/config/config.toml \
  -e SQL_MCP_TARGET_DW_URL="$SQL_MCP_TARGET_DW_URL" \
  -v "$HOME/sql-mcp/config.toml:/config/config.toml:ro" \
  -v sqlmcp-data:/data \
  <release-image>
```

HTTP MCP: `POST http://127.0.0.1:8080/mcp` · 헬스: `GET /health`.

## 2. 에이전트에 연결

### Cursor

`~/.cursor/mcp.json` (전역) 또는 프로젝트 `.cursor/mcp.json`:

```json
{
  "mcpServers": {
    "sql-mcp": {
      "type": "stdio",
      "command": "/Users/YOU/bin/sql-mcp",
      "args": ["--stdio"],
      "env": {
        "SQL_MCP_SEMANTIC_PATH": "/Users/YOU/.sql-mcp/semantic.db",
        "SQL_MCP_CONFIG": "/Users/YOU/bin/config.toml",
        "SQL_MCP_TARGET_DW_URL": "postgres://USER:PASS@HOST:5432/DB"
      }
    }
  }
}
```

경로를 본인 환경으로 바꾼 뒤 MCP를 다시 로드합니다. 이미 HTTP로 띄운 서버면 `url`에 `http://127.0.0.1:8080/mcp`를 넣으면 됩니다.

### Claude Desktop

`claude_desktop_config.json`의 `mcpServers`에 Cursor와 같은 항목을 넣습니다.

- macOS: `~/Library/Application Support/Claude/claude_desktop_config.json`
- Windows: `%APPDATA%\Claude\claude_desktop_config.json`

앱을 완전히 종료했다가 다시 켭니다.

### Claude Code

프로젝트 `.mcp.json` 또는 `claude mcp add`로 동일하게 stdio(또는 HTTP URL)를 등록합니다.

## 3. 메타 구성

물리 DW 카탈로그는 에이전트(또는 기존 DW 클라이언트)가 **MCP 밖에서** 읽고, 아래 순서로 JSON을 만든 뒤 MCP `metadata_*`로 등록합니다.

```text
table → view → cube → metric | segment
```

바이너리만 있어도 됩니다. 서버가 MCP Resource로 등록 스키마(`document.schema.json`)와 Agent Skills(작성·검증·등록 절차)를 제공합니다. 에이전트에게 「sql-mcp 스킬·스키마 Resource를 읽고 DW 메타를 등록해 줘」라고 요청하면 됩니다.

## 4. DW 질의

| 툴 | 용도 |
|----|------|
| `search_columns` | 시맨틱 컬럼/멤버 검색 |
| `sample_column` | cube 멤버 샘플(+코드사전) |
| `execute_sql` | **시맨틱 SQL만** → 물리 SELECT 컴파일·실행 |
| `list_targets` / `list_tables` / `describe_table` | 타깃 목록 · **cube** 목록/상세 |

`execute_sql`에 자유 물리 SELECT는 없습니다. cube FROM, metric/dimension/segment 기준만 허용됩니다.

예시 프롬프트:

- 「매출 cube에서 지난달 region별 revenue metric을 구해줘」
- 「status 관련 dimension 찾아 sample 보여줘」
