# 배포 런북

`sql-mcp`는 단일 프로세스 MCP 서버다. D7: **release 바이너리** 또는 **컨테이너**.

## 사전 조건

- Rust 1.94+ (`rust-toolchain.toml`)
- 시맨틱 DB 경로 쓰기 가능
- 타깃 Postgres DSN은 환경 변수로만 주입 (`url_env`)

## GitHub Release `current`

[`.github/workflows/ci.yml`](../.github/workflows/ci.yml)이 PR/`master`에서 테스트를 돌리고, 성공한 `master` 푸시에 한해 [`.github/workflows/cd.yml`](../.github/workflows/cd.yml)이 Linux·Windows·macOS **release** 바이너리를 빌드한 뒤 태그 [`current`](https://github.com/yoosungung/sql-mcp/releases/tag/current) Release를 교체한다(`workflow_dispatch`로 CD만 재실행 가능).

| 파일 | 러너 |
|------|------|
| `sql-mcp-x86_64-unknown-linux-gnu` | `ubuntu-latest` |
| `sql-mcp-x86_64-pc-windows-msvc.exe` | `windows-latest` |
| `sql-mcp-aarch64-apple-darwin` | `macos-latest` (Apple Silicon) |

각 파일에 `.sha256`이 붙는다. 시맨틱 버전 태그(`v0.1.0` 등)는 이 워크플로가 만들지 않는다.

## 바이너리

로컬:

```text
cargo build -p sql-mcp --release
# 산출물: target/release/sql-mcp

export SQL_MCP_SEMANTIC_PATH=/var/lib/sql-mcp/semantic.db
export SQL_MCP_CONFIG=/etc/sql-mcp/config.toml
export SQL_MCP_TARGET_DW_URL=postgres://...
./target/release/sql-mcp
```

헬스: `GET /health` → `{"ok":true}`  
MCP: `POST /mcp` (JSON)

## 컨테이너

```text
docker build -t sql-mcp:local -f deploy/Dockerfile .
docker run --rm -p 8080:8080 \
  -e SQL_MCP_SEMANTIC_PATH=/data/semantic.db \
  -e SQL_MCP_CONFIG=/config/config.toml \
  -e SQL_MCP_TARGET_DW_URL="$SQL_MCP_TARGET_DW_URL" \
  -v "$PWD/server/examples/config.example.toml:/config/config.toml:ro" \
  -v sqlmcp-data:/data \
  sql-mcp:local
```

## Kubernetes

베이스 매니페스트: [`k8s/base/`](k8s/base/).  
시크릿/DSN은 오버레이에서 주입한다(베이스에 비밀번호를 넣지 않는다).

```text
kubectl apply -k deploy/k8s/base
```

## 관측

- 로그: `RUST_LOG` / `tracing` (`info,sql_mcp=debug` 기본)
- 프로브: `/health`
- 시크릿·DSN은 로그/툴 응답에 남기지 않는다(ARCHITECTURE §1.9)
