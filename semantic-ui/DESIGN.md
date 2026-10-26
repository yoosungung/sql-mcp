# semantic-ui

시맨틱 메타 작성 화면. 조회 에이전트(`client/`)와 컴파일 서버 내부는 건드리지 않는다. 레이아웃 초안은 [`client/docs/semantic_layer_ui.svg`](../client/docs/semantic_layer_ui.svg). 정본은 meta JSON이다.

컨트롤은 `@radix-ui/themes`다. 라이트 화면, gray `slate`, accent `indigo`, 패널은 단색이다. 셸은 위 타깃 바, 왼쪽 목록, 가운데 보기, 오른쪽 편집, 아래 상세다. 왼쪽과 오른쪽은 접을 수 있다. 영역마다 다른 배경색을 칠하지 않는다.

`App.tsx`는 타깃·문서 상태와 저장만 둔다. 화면 조각은 `src/components/`의 `Catalog`, `Canvas`, `Inspector`, `Detail`이다.

## 화면

- 위: 타깃 하나. 타깃 사이 조인 없음. `list_databases`.
- 왼쪽 **Physical**: 테이블을 가운데 `join` · `union` · `minus`로 끌어 초안을 만든다. 체크도 같은 목록에 넣는다. 컬럼 목록은 왼쪽 트리에 두지 않는다.
- 왼쪽 **Cubes**: 뷰를 눌러 연다. `New cube`는 체크한 뷰로 cube를 만든다. 마지막이 정본 `view`, 나머지가 `choice`(`summary: true`, `prefer: summary`).
- 가운데 문서 막대: 연 뷰의 저장과 삭제. 새 초안은 `Save view`. 저장된 뷰는 `Save view`와 `Delete view`. 삭제는 확인 후 그 뷰의 metric · segment · cube를 먼저 지운다. Physical은 join 선 또는 union/minus 포개기. Cubes의 choice 스택은 위가 요약 우선. 드래그는 순서만 바꾸고 `Save view`가 cube `PUT /api/meta`.
- 오른쪽: Physical 초안과 조인 뷰는 물리 컬럼 매핑. Cubes는 cube 트리(dimension · fact · metric · segment).
- 아래: 선택한 table / view / dimension / fact / metric / segment. 테이블 컬럼 목록은 여기 두지 않는다. 뷰 설명은 문서의 `Save view`로 저장한다. 그 외는 `Save`.

데이터는 HTTP `/api` (`/api/databases`, 물리 tables/columns/sample, `/api/meta`). JSON-RPC `/mcp`는 쓰지 않는다.

## Commands

```text
cd semantic-ui
npm install
npm test
npm run build
# 산출물: semantic-ui/dist  →  sql-mcp GET /ui/

# 개발 (sql-mcp HTTP가 vite 프록시 대상에서 떠 있어야 함. 현재 8090 — 8080은 다른 프로세스가 점유)
npm run dev
# http://127.0.0.1:5173/ui/
```

정적 서빙은 [server/DESIGN.md](../server/DESIGN.md)의 `/ui`.
