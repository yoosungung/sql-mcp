# semantic-ui

시맨틱 메타 작성 화면. 조회 에이전트(`client/`)와 컴파일 서버 내부는 건드리지 않는다. 정본은 meta JSON이다. UI 목적은 시맨틱 레이어 JSON(CRUD): `table` · `view` · `cube` · `metric` · `segment`.

컨트롤은 `@radix-ui/themes`다. 라이트 화면, gray `slate`, accent `indigo`, 패널은 단색이다. 셸은 위 Database 바, 왼쪽 목록, 가운데 보기, 오른쪽 편집, 아래 상세다. 왼쪽과 오른쪽은 접을 수 있다. 아래 Detail은 접기 · 기본 · 확장 세 단계다.

`App.tsx`는 Database(`target`)·문서 상태·저장·매핑 팝업 플래그를 둔다. 화면 조각은 `src/components/`의 `Catalog`, `Canvas`(ViewHeader/CubeHeader · JoinCanvas · UnionCanvas · ChoiceStack), `Inspector`, `Detail`, `ViewMappingEditor`, `CubeMappingEditor`다.

## 화면

- 위: Database 하나. 라벨은 **Database**, JSON 필드는 `target`. `GET /api/databases`. Database 사이 조인 없음.
- 왼쪽 탭 **Tables** / **Cubes**. 카탈로그 헤더는 한 줄(Tables · Cubes · 접기). 인스펙터 헤더는 접기 · Columns/Members · 버튼. 폭이 좁으면 버튼은 글자를 세로로 쪼개지 않고 다음 줄로 넘어간다.

### Tables 탭

- 목록은 **등록된** `kind=table` 문서(`GET /api/meta`). 물리 카탈로그를 목록으로 쓰지 않는다.
- 탭 아래 **Register** / **Delete**: Register 팝업은 물리 테이블 체크 목록. 여러 개를 한 번에 `POST /api/meta`하고, 이미 등록된 항목을 다시 체크하면 컬럼을 `PUT`으로 갱신(pk/fk/필터 유지). Delete는 `DELETE /api/meta/table/{name}`(view/cube 참조 있으면 거절). 컬럼·이름·**FK**는 등록 값(물리 DB 메타)을 유지하고 Detail에서 고치지 않는다. Register JSON의 `table`은 물리명, `name`은 논리명, `db`는 MCP `target`. **scope / default / permission 필터**와 선택 `role`만 Detail에서 편집해 `PUT /api/meta/table/{name}`. 각 필터는 라벨·식 한 줄, Remove는 아이콘(`×`). permission의 role은 라벨 열만큼 좁고, expression은 scope/default 식과 같은 폭. scope/default Remove는 식을 비우고, permission은 그 행을 뺀다.
- 가운데 **ViewHeader**(이름 · **View type** 셀렉터 하나 · Unsaved · New / Open / Rename / Save / Delete) + 본문:
  - New 팝업에서 이름과 View type을 넣고 Create.
  - combine은 헤더 셀렉터만. 본문 join/union/minus 버튼 행은 없다. union/minus로 바꾸면 `joins` 키를 빼고 조인 캔버스를 접는다. join으로 바꾸면 `joins: []`.
  - 테이블은 **캔버스**에 드롭한다.
  - `join`: `@xyflow/react` + `@dagrejs/dagre` JoinCanvas. 노드 위치는 저장하지 않음. 열기·드롭·조인 변경 시 dagre가 조인 그래프(source=노드, join=간선, `rankdir: LR`)로 다시 배치한다. 초기 `fitView`는 `maxZoom: 1`이라 테이블 글자가 UI 폰트를 넘기지 않고, Controls 수동 줌은 그대로다. 드롭은 source만 추가하고 조인을 만들지 않는다. 같은 table을 여러 번 드롭할 수 있다. 별칭은 **view `sources[].alias`**(table 문서 아님). 생략 시=`table`, 뷰 내 유일(중복 드롭은 `table_2` …). 캔버스 노드 선택 시 Detail **Alias**에서 그 뷰 source만 고친다. 핸들 연결 → `view.joins`(`left`/`right`=source 별칭). JoinCanvas는 헤더 아래 남는 칸을 채운다(고정 높이 없음). 드롭 하이라이트 테두리 없음. `relationship`은 선택(D29 카디널리티: many_to_one / one_to_one / one_to_many). 수동 조인의 relationship은 unset이고, 없으면 grain guard가 그 조인을 보지 않는다.
  - `union` / `minus`: 세로 스택 + 드래그 재정렬.
- 오른쪽 **Columns**: 뷰 컬럼 목록(파생 PK · type · name). **description 없음**(`viewColumn`은 `name`+`mappings`만). 뷰 이름은 헤더에만 두고 여기엔 안 보여 준다. 헤더 **Map columns** · **Add column** · **Delete column**(좁으면 다음 줄). Add는 첫 source의 첫 물리 컬럼을 매핑한 `col`을 만들고, Delete는 선택된 뷰 컬럼을 뺀다. Map columns → ViewMappingEditor(거의 전체 화면).
- 물리 컬럼명이 `$defs/name` 밖이면 뷰 컬럼은 sanitize한 논리명, `mappings.column`은 물리명(D22).
- 아래 Detail: Catalog/Canvas 테이블 · join 링크 · 컬럼 선택에 따라 네 패널. 헤더 한 줄에 접기 · 기본 · 확장. 테이블 상세는 세 칸 비율 **1 : 2 : 2**(헤더+PK : 컬럼 : 필터). 칸은 가로는 숨기고(`overflow-x: hidden`) 컬럼 목록만 세로 스크롤. 헤더 칸 라벨은 Name · Alias(join 뷰 source일 때만) · **Database** · PK. 컬럼 칸은 name · type · nullable · FK 표. FK 셀은 문서 `fk`(물리 DB에서 온 값)를 `table.column`으로 읽기 전용 표시하고, 없으면 `—`. `name`=`table`(물리명), `db`=`target`. 스키마·Physical 중복 표시 없음. join 선택 시 Detail은 세 줄: 시작 테이블+시작 컬럼, 연산자(`op`, 생략=`=`)+Join type(`inner`/`left`/`right`/`full` 콤보)+relationship, 끝 테이블+끝 컬럼. 조인 `left`/`right`는 source 별칭이다. 뷰 컬럼 선택 시 두 칸: **Name**(편집) / Type·PK(매핑에서 파생, 읽기 전용) · **Mappings**(source별 물리 컬럼, 테이블명은 한 줄). 스키마 `viewColumn`은 `name`+`mappings`만.

View 편집은 메모리 JSON; **Save**가 `POST`/`PUT /api/meta`.

### Cubes 탭

- 왼쪽: 저장된 view 목록(체크 → choice 스택).
- 가운데 **CubeHeader**(이름 · New / Delete / Rename) + choice 스택(드래그 우선순위; 마지막=정본 `cube.view`).
- **뷰 Delete는 큐브를 지우지 않는다.** 해당 뷰를 cube `choice`/`view`에서 빼고 캔버스 스택에서 즉시 제거한다. 큐브 Delete만 cube·metric·segment를 지운다.
- 오른쪽: dimension · fact · metric · segment 목록(name · description · type). 위 **Map members** · **Add metric** · **Add segment**. Map members → CubeMappingEditor. 행 Kind는 dimension/fact 콤보.
- 아래: 큐브(description · grain · role) 또는 멤버 타입별 Detail. dim/fact는 type enum, column XOR expression, encryption, dim `as_code`, fact `additive`, `role`. metric은 expression · aggregation · (`group`이면 `group_by`) · filter(`null` 허용) · role. segment는 filter · role.
- cube 로드 시 붙은 metric/segment를 함께 읽고, 큐브 헤더 Save 시 변경분을 cube와 함께 저장.
- **검증은 HTTP `/api/meta`.** 클라에서 스키마를 다시 검사하거나 Save를 막지 않는다. 400 `{ code, message }`는 상단 배너(code · 본문 · 닫기). 뷰 Delete는 큐브를 `view: ""`로 고치지 않는다. 참조 중이면 서버 메시지를 보여 준다. Rename 후 Save는 새 이름 저장 뒤 옛 문서를 `DELETE`.

### 매핑 팝업

- **ViewMappingEditor**: 왼쪽 뷰 컬럼(+미매핑 테이블 컬럼 하단), 행 앞 체크박스=매핑 정본, 새 뷰는 동명 자동 매칭. 위 테이블 헤더는 source 별칭. join은 dagre 왼쪽→오른쪽, union/minus는 캔버스에서 드래그한 `sources` 순서.
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
11. **뷰 삭제** — 조인·유니온 뷰 Delete(큐브는 남기고 스택에서만 제거).
12. **테이블 삭제** — 참조가 없을 때 Delete table.

재현 오류 (superhero 12단계 사이클):
- ~~헤더 아래 join/union/minus 버튼 중복~~ → 헤더 View type만. 드롭은 캔버스.
- ~~Map members 체크가 바로 풀린다~~ → 다이얼로그를 열 때만 rows를 심음. Kind는 dimension/fact 콤보.
- Add metric / Add segment.
- Rename 후 옛 문서 `DELETE`.

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
