# semantic-ui

시맨틱 메타 작성 화면. 조회 에이전트(`client/`)와 컴파일 서버 내부는 건드리지 않는다. 정본은 meta JSON이다. UI 목적은 시맨틱 레이어 JSON(CRUD): `table` · `view` · `cube` · `metric` · `segment`.

컨트롤은 `@radix-ui/themes`다. 라이트 화면, gray `slate`, accent `indigo`, 패널은 단색이다. 셸은 위 타깃 바, 왼쪽 목록, 가운데 보기, 오른쪽 편집, 아래 상세다. 왼쪽과 오른쪽은 접을 수 있다.

`App.tsx`는 타깃·문서 상태·저장·매핑 팝업 플래그를 둔다. 화면 조각은 `src/components/`의 `Catalog`, `Canvas`(ViewHeader/CubeHeader · JoinCanvas · UnionCanvas · ChoiceStack), `Inspector`, `Detail`, `ViewMappingEditor`, `CubeMappingEditor`다.

## 화면

- 위: 타깃 하나. `GET /api/databases`. 타깃 사이 조인 없음.
- 왼쪽 탭 **Tables** / **Cubes**.

### Tables 탭

- 목록은 **등록된** `kind=table` 문서(`GET /api/meta`). 물리 카탈로그를 목록으로 쓰지 않는다.
- 탭 아래 **Register** / **Delete**: Register는 물리 `GET /api/databases/{id}/tables`·describe로 table JSON을 만들어 `POST /api/meta`. Delete는 `DELETE /api/meta/table/{name}`(view/cube 참조 있으면 거절). table 문서 **편집 금지**(추가·삭제만).
- 가운데 **ViewHeader**(이름 · combine 타입 · New / Open / Rename / Save / Delete) + 본문:
  - `join`: `@xyflow/react` + `@dagrejs/dagre` JoinCanvas. 노드 위치는 저장하지 않음. 핸들 연결 → `view.joins`(선택 `relationship`).
  - `union` / `minus`: 세로 스택 + 드래그 재정렬.
- 오른쪽 **Columns**: 뷰 컬럼 목록(파생 PK · type · name · description). 위 **Map columns** → ViewMappingEditor(거의 전체 화면).
- 아래 Detail: Catalog/Canvas 테이블 · join 링크 · 컬럼 선택에 따라 네 패널.

View 편집은 메모리 JSON; **Save**가 `POST`/`PUT /api/meta`.

### Cubes 탭

- 왼쪽: 저장된 view 목록(체크 → choice 스택).
- 가운데 **CubeHeader**(이름 · New / Delete / Rename) + choice 스택(드래그 우선순위; 마지막=정본 `cube.view`).
- 오른쪽: dimension · fact · metric · segment 목록(name · description · type). 위 **Map members** → CubeMappingEditor.
- 아래: view 또는 멤버 타입별 Detail.
- cube 로드 시 붙은 metric/segment를 함께 읽고, Save 시 변경분을 cube와 함께 저장.

### 매핑 팝업

- **ViewMappingEditor**: 왼쪽 뷰 컬럼(+미매핑 테이블 컬럼 하단), 행 앞 체크박스=매핑 정본, 새 뷰는 동명 자동 매칭.
- **CubeMappingEditor**: 위 choice view 이름, 왼쪽 dim/fact, 드래그로 매핑 변경, 미매핑은「미매핑」구간+체크박스.

데이터는 HTTP `/api`만. JSON-RPC `/mcp`는 쓰지 않는다.

## Commands

```text
cd semantic-ui
npm install
npm test
npm run build
# 산출물: semantic-ui/dist  →  sql-mcp GET /ui/

# 개발 (sql-mcp HTTP가 vite 프록시 대상에서 떠 있어야 함. 현재 8090)
npm run dev
# http://127.0.0.1:5173/ui/
```

정적 서빙은 [server/DESIGN.md](../server/DESIGN.md)의 `/ui`.
