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
- 가운데 **ViewHeader**(이름 · **View type** 셀렉터 하나 · Unsaved · New / Open / Rename / Save / Delete) + 본문:
  - combine은 헤더 셀렉터만. 본문 join/union/minus 버튼 행은 없다.
  - 테이블은 **캔버스**에 드롭한다.
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

## E2E 시나리오

BIRD `superhero` 타깃. 만든 문서만 지운다. sql-mcp HTTP `:8090` + `npm run dev`.
정본 경로는 아래 **한 사이클**(12단계)이다. 뷰 생성은 캔버스 드래그. New→Create 빈 껍질은 정본이 아니다(두 번째 뷰를 시작하려면 저장 후 New로 캔버스만 비운 뒤 다시 드래그).

1. **테이블 등록** — Register로 물리 테이블(조인용 `superhero`+`publisher`, 유니온용 공통 컬럼 테이블 둘).
2. **뷰 생성** — 헤더 **View type**만으로 combine을 고른다(본문 join/union/minus 버튼 없음). 테이블을 **캔버스**에 드래그해 **join 뷰 하나**와 **union 뷰 하나**를 만들고 각각 Save.
3. **컬럼 매핑** — 각 뷰 Map columns에서 매핑 확인·Apply 후 Save.
4. **큐브 만들기** — Cubes 탭 → 저장한 view 체크 → **Add to cube stack** → Save.
5. **큐브 디멘션·팩트 매핑** — **Map members**. 체크한 행이 dimension/fact, Apply 후 Members의 Dimensions / Facts.
6. **metric·segment 만들기** — Members에서 추가 후 Detail Expression / Filter, cube Save.
7. **뷰 리네임** — 뷰 Rename 후 Save.
8. **큐브 리네임** — 큐브 Rename 후 Save.
9. **열어서 보기** — Open으로 저장한 뷰·큐브를 다시 연다.
10. **큐브 삭제** — Cube 헤더 Delete(붙은 metric/segment 함께).
11. **뷰 삭제** — 조인·유니온 뷰 Delete.
12. **테이블 삭제** — 참조가 없을 때 Delete table.

재현 오류 (superhero 12단계 사이클):
- ~~헤더 아래 join/union/minus 버튼 중복~~ → 헤더 View type만. 드롭은 캔버스.
- Map members 체크가 바로 풀린다(`open`/`cube`/`viewDocs` effect가 rows를 다시 심음). Apply해도 dim/fact가 안 남음.
- Map members 미매핑 후보는 전부 `kind=dimension`이고 Kind는 글자만 보여 **fact로 바꿀 UI가 없다**.
- Metrics/Segments **Add metric / Add segment 버튼이 없다**.
- Rename 후 Save는 새 이름 `POST`라 **옛 문서가 남는다**(뷰·큐브 복제).

## Commands

```text
cd semantic-ui
npm install
npm test
# 정본 E2E: BIRD superhero · sql-mcp HTTP 127.0.0.1:8090 + vite. 브라우저에서 DESIGN 12단계 한 사이클.
# 샌드박스 cargo는 aws-lc EPERM → 권한(all)로 기동.
npm run build
# 산출물: semantic-ui/dist  →  sql-mcp GET /ui/

# 개발 (sql-mcp HTTP가 vite 프록시 대상에서 떠 있어야 함. 현재 8090)
npm run dev
# http://127.0.0.1:5173/ui/
```

정적 서빙은 [server/DESIGN.md](../server/DESIGN.md)의 `/ui`.
