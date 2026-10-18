
  ## 1. Executive Summary: 숫자의 착시와 진짜 실패 구조

  • 전체 성적: 286문항 중 71문항 성공 (EX 24.83%), 215문항 실패 (75.17%)
  • 1차 관찰 (실행 성공 여부):
      • 215건의 실패 중 컴파일/실행 자체가 끝내 실패한 건은 **18건(8.4%)**뿐입니다.
      • 나머지 **197건(91.6%)**은 execute_sql이 정상적으로 물리 SQL을 실행해 결과를 반환했으나, 결과 집합(pred_rows)이 정답(gold_rows)과 불일치하거나(179건), 평가
      하네스의 문자열 절단 버그로 유실(18건)되었습니다.
  • 2차 관찰 (score_report.json 분류기의 착시):
      • 현재 채점기(classify_failure)는 gold_sql에 JOIN이 있고 에이전트 SQL에 JOIN이 없으면 무조건 "gold는 JOIN인데 에이전트는 단일 큐브만 질의해 오답"(139건, 64.7%)으로
      분류합니다.
      • 하지만 이는 분류 휴리스틱의 착시입니다. generate_meta.py가 DB당 **단 1개의 통합 큐브({db}_all)**만 생성했기 때문에, 에이전트는 원래 JOIN을 쓸 수 없고 단일 큐브만
      조회하는 것이 정상입니다. 진짜 원인은 **"큐브 간 JOIN 부재"가 아니라 "단일 거대 뷰(One-Big-View) 내부의 잘못된 조인 그래프와 그레인(Grain) 붕괴"**에 있습니다.

  ──────
  ## 2. 215건 실패의 6대 핵심 근본 원인 (Root Cause Taxonomy)

  전수 분석 결과, 215건의 실패는 크게 6가지 구조적 메커니즘(복합 작용 포함)으로 귀결됩니다.

| 순위 | 근본 원인 (Root Cause) | 영향 문항 수 (… | 책임 계층 | 핵심 메커니즘 요약 |
|------|------------------------|-----------------|-----------|-------------------|
| 1 | One-Big-View 조인 토폴로지 결함 및 Chasm/Fan Trap | 113건 (52.6%) | semantic_layer + server (compile.rs) | ① sources[0](Root) 무조건 강제 앵커링② Spanning Tree가 필수 FK 간선 삭제③ 무조건적인 INNER JOIN으로 인한 행 증발/뻥튀기④ Role-playing 차원 및 Bridge 테이블 왜곡 |
| 2 | 소형 LLM(gemma4:e2b)의 시맨틱 SQL 문법/식별자 착란 | 48건 (22.3%) | dw_agent + server 에러 메시지 | ① GROUP BY cube.col이 SQL 파서에서 CUBE(...) 예약어로 충돌 (Expected: (, found: . 410회)② Evidence의 DIVIDE/SUBTRACT/WHERE 필터 구문 직역③ 물리 별칭(T1, fact) 습관적 사용 |
| 3 | date_trunc 방언 변환(SQLite)과 날짜/나이 연산의 불일치 | 40건 (18.6%) | server (dialect.rs) + dw_agent | SQLite에서 date_trunc('year', col)이 'YYYY-01-01 00:00:00' 문자열을 반환하여 = 2019 비교 시 0건이 되거나, CURRENT_TIMESTAMP 차단으로 나이(Age) 계산 불가 |
| 4 | SQLite 정수 나눗셈(Integer Division) 절삭 및 프로젝션 열 불일치 | 45건 (20.9%) | dw_agent + server + bird_eval | ① SUM(...) / COUNT(...)가 정수 나눗셈되어 0 또는 정수로 절삭 (22건)② GROUP BY 열 추가 프로젝션 또는 컬럼 순서 차이로 인한 EX 오답 처리 |
| 5 | 복합 분석 질의(Subquery / CTE / Window OVER) 표현 불가 | 32건 (14.9%) | server (D16/D23 계약) + semantic_layer | 비율 비교·평균 초과 필터(col > AVG(col))·순위(RANK() OVER)·최단 랩타임 서브쿼리 등이 AST 가드에서 원천 차단됨 |
| 6 | 평가 하네스(bird_eval.py)의 log_max=4000 JSON 절단 버그 | 18건 (8.4%) | client (bird_eval.py) | execute_sql이 정상 성공했으나 결과 문자열이 4,000자에서 잘려 json.loads가 실패하고 pred_rows = None으로 채점됨 (실제 정답 3건 포함) |
  ──────
  ## 3. 원인별 심층 해부 (OLAP & DW 아키텍처 관점)

  ### 원인 ① [치명적] One-Big-View 스패닝 트리(Spanning Tree)와 Root 강제 앵커링이 부른 참사 (113건)

  OLAP/DW 설계에서 가장 금기시되는 안티패턴은 **"서로 다른 그레인(Grain)의 복수 팩트와 브릿지 테이블을 단일 INNER JOIN 트리로 묶는 것"**입니다. 현재 generate_meta.py와
  compile.rs의 조합은 다음 4가지 치명적 왜곡을 일으킵니다.

  #### 1) compile_join_view의 sources[0](Root) 무조건 강제 포함

    // server/src/compile.rs:505-509
    // Always anchor the primary/fact table (sources[0]) so LEFT JOIN direction is preserved.
    if let Some(base) = sources.first().and_then(|s| source_name(s).ok()) {
        t_req.insert(base.to_string());
    }
    let included = steiner_nodes(&joins, &t_req, &all_names)?;

  • 문제: 뷰의 모든 조인이 INNER JOIN인데도, 질의가 참조하지 않은 sources[0](최대 차수 허브 테이블)을 무조건 t_req에 넣습니다.
  • 실제 피해 (77건에서 Gold SQL은 Root 테이블을 쓰지 않음):
      • QID 1362 (student_club): "Orange County, Virginia에 몇 개의 도시가 있는가?"
          • Gold SQL: SELECT COUNT(city) FROM zip_code WHERE county = 'Orange County' AND state = 'Virginia' → 정답 7
          • sql-mcp 컴파일 결과: sources[0]인 member가 강제 포함되면서 FROM member INNER JOIN zip_code로 컴파일됨. 해당 카운티 출신 학생 회원이 없으므로 결과 0 반환.
      • QID 1380 (student_club): "Food 카테고리 예산의 총 지출액(spent)은?"
          • Gold SQL: SELECT SUM(spent) FROM budget WHERE category = 'Food' → 정답 1234.73
          • sql-mcp 컴파일 결과: budget만 조회하면 되는데 sources[0]인 member까지 경로를 잇기 위해 member ⋈ expense ⋈ budget을 조인함. 예산 1건당 여러 지출(expense) 행이
          붙어 Fan Trap(중복 합산) 발생 → 오답 2735.93 반환.



  #### 2) spanning_joins의 사이클 제거가 핵심 FK 경로를 절단

  관계형 스키마의 다이아몬드/사이클 관계를 단순 BFS 스패닝 트리로 펴면서, 실제 업무적으로 가장 중요한 직접 조인 간선이 잘려 나갔습니다.

  • student_club의 event ↔ budget 단절 (14건 실패):
      • 물리 DB에는 budget.link_to_event → event.event_id FK가 존재합니다.
      • 그러나 member를 루트로 BFS를 돌면서 member → attendance → event와 member → expense → budget이 먼저 방문되어 event ↔ budget 직접 간선이 버려졌습니다.
      • 결과적으로 이벤트별 예산/비용을 묻는 모든 질의(QID 1340, 1376, 1389, 1390, 1398, 1401, 1403 등)가 event ⋈ attendance ⋈ member ⋈ expense ⋈ budget이라는 5중
      다대다(M:N) Chasm Trap 경로를 타게 되어 데이터가 완전히 왜곡되었습니다. (예: QID 1401 포스터 비용 67.81이 출석 회원 수만큼 곱해져 372.96이 됨)
  • formula_1의 results/qualifying/pitStops/lapTimes ↔ drivers/constructors 단절 (45건 실패, 정답률 9.1%로 최저):
      • races가 루트가 되면서 drivers는 오직 driverStandings에만 연결되고, constructors는 오직 constructorResults에만 연결되었습니다!
      • 따라서 에이전트가 results(경주 결과)나 lapTimes(랩타임)와 drivers(드라이버 이름)를 함께 조회하면, results ⋈ races ⋈ driverStandings ⋈ drivers로 조인됩니다. 한
      경주(raceId)의 모든 드라이버 스탠딩과 모든 랩타임이 교차 곱(Cartesian Fanout)을 일으켜 formula_1에서 대량 오답이 발생했습니다.


  #### 3) Role-Playing Dimension(역할 다중 차원) 및 자기참조 브릿지 매핑 손실 (26건)

  • superhero의 색상(colour) 차원 손실 (13건):
      • superhero 테이블은 eye_colour_id, hair_colour_id, skin_colour_id 3개의 FK가 모두 colour.id를 가리킵니다.
      • spanning_joins의 if pair in pair_used: continue가 동일 테이블 쌍을 1개로 제한하여 skin_colour_id = colour.id 하나만 조인하고 눈 색깔과 머리 색깔 조인을 버렸습니다.
      • 게다가 skin_colour_id는 colour.id와 흡수(absorb)되었지만 eye_colour_id와 hair_colour_id는 정수 ID로 남았고, colour 차원은 오직 피부색만 의미하게 되어 눈/머리
      색상을 묻는 모든 질의(QID 739, 750, 753, 795, 819 등)가 실패했습니다.
  • toxicology의 atom ↔ connected 조인 왜곡 (13건):
      • connected(atom_id, atom_id2, bond_id)는 두 원자의 결합을 나타내는 브릿지 테이블입니다.
      • 뷰 정의에는 atom.atom_id = connected.atom_id2로 매핑되어 있고, 정작 bond ↔ connected(bond_id) 간선은 스패닝 트리에서 빠져(molecule ⋈ bond, molecule ⋈ atom ⋈
      connected) 특정 결합의 두 원자를 묻는 질의(QID 213, 243 등)가 분자 전체 결합으로 증폭되었습니다.


  #### 4) 다중 테이블 동명 컬럼의 강제 접두사(table_col)와 의미 혼동

  • student_club에서 income.amount가 먼저 등록되어 amount를 차지하자, 정작 질문에서 자주 쓰이는 budget.amount는 budget_amount로 밀려났고(QID 1359에서 에이전트가 amount를
  써서 income 테이블이 조인됨), event.status(status)와 budget.event_status(event_status)가 혼동되었습니다(QID 1376, 1403).
  • formula_1에서는 races.name(name), circuits.name(circuits_name), constructors.name(constructors_name), drivers.nationality(drivers_nationality) vs constructors.
  nationality(nationality) 등이 섞여 에이전트가 잘못된 엔티티의 컬럼을 필터링했습니다.
  ──────
  ### 원인 ② sqlparser의 CUBE 예약어 충돌 및 소형 모델(gemma4:e2b)의 문법 오류 (48건 / 전체 에러 410회)

  실행 로그 전체에서 **가장 많이 발생한 단일 에러(410회)**는 다음과 같습니다.

    sql parse error: sql parser error: Expected: (, found: . at Line: 1, Column: ...

  • 원인 메커니즘:
      • 에이전트(gemma4:e2b)는 시맨틱 SQL이라는 지시를 받으면 습관적으로 테이블 별칭이나 큐브 한정자를 cube.event_name처럼 작성합니다.
      • SELECT나 WHERE 절에서는 cube.col이 파싱 후 검증 단계에서 "qualifier cube does not match FROM cube ..." 에러를 내지만, GROUP BY cube.event_name에서는 sqlparser의
      GenericDialect가 GROUP BY CUBE(...) 구문으로 먼저 인식하여 cube 뒤에 여는 괄호 (가 오지 않고 점 .이 왔다는 구문 에러(Expected: (, found: .)를 던집니다!
      • 에이전트는 왜 GROUP BY 뒤에 (가 필요한지 이해하지 못해 동일한 쿼리를 한 문항당 30~117회씩 무한 반복 호출(QID 1322: 68회, QID 1323: 30회, QID 244: 110회, QID 83:
      117회)하다가 재시도 한도에 걸려 실패했습니다.
  • 기타 에이전트 문법/규칙 위반 패턴:
      1. Evidence 의사코드 직역: BIRD의 evidence에 적힌 DIVIDE(a, b), SUBTRACT(a, b), MULTIPLY(a, b)(총 165회), 또는 COUNT(ID WHERE Diagnosis = 'SLE')(40회)를 실제 SQL
      문법인 줄 알고 그대로 호출.
      2. 공백 포함 물리 컬럼명 사용: aCL IgM(49회)처럼 Evidence의 물리 컬럼명을 describe_table의 시맨틱 이름(aCL_IgM)으로 바꾸지 않고 그대로 사용.
      3. 쌍따옴표 별칭 내 연산자 오용: AS "FRPM Count (K-12)" / "Enrollment (K-12)"(QID 32, 157회 반복)처럼 잘못된 별칭 구문 생성.

  ──────
  ### 원인 ③ date_trunc의 SQLite 방언 변환(D21)과 에이전트의 연도/나이 연산 불일치 (40건)

  date_trunc가 사용된 27개 최종 쿼리 중 **26개가 실패(성공 1건)**했으며, 날짜 관련 문항 40건이 실패했습니다.

  1. date_trunc('year', col)의 반환값 형태 불일치:
      • dialect.rs에서 SQLite 타깃은 date_trunc('year', col)을 datetime(col, 'start of year')로 변환합니다.
      • SQLite에서 datetime('2019-09-10', 'start of year')의 결과는 정수 2019나 '2019'가 아니라 **'2019-01-01 00:00:00'**입니다!
      • 반면 에이전트는 YEAR(col)이 금지되자 date_trunc('year', event_date) = 2019 또는 = '2019'로 작성합니다(QID 1340, 1162, 1171, 1242, 895, 931, 959, 960, 72 등).
      • SQLite에서 '2019-01-01 00:00:00' = 2019는 항상 FALSE이며, 산술 연산 시 '1951-01-01 00:00:00'이 문자열 앞자리 숫자 1951로 강제 캐스팅되기도 하지만 동등
      비교(=)에서는 모두 매칭에 실패해 0 또는 NULL을 반환했습니다.
  2. 이미 연도(정수)인 컬럼에 date_trunc 적용:
      • formula_1의 year(예: 2009)나 california_schools의 Academic_Year('2013-2014')에 date_trunc('year', year)를 씌우면 SQLite datetime(2009, 'start of year')는
      줄리안일(Julian day) -4708-01-01이 되어 완전히 깨집니다(QID 931, 72).
  3. CURRENT_TIMESTAMP / NOW 부재로 인한 나이(Age) 계산 실패:
      • thrombosis_prediction과 formula_1에는 환자의 나이나 드라이버의 나이를 묻는 문항(Gold SQL: STRFTIME('%Y', CURRENT_TIMESTAMP) - STRFTIME('%Y', Birthday))이 다수
      존재합니다(QID 1227, 1229, 1232, 1235, 1239, 1242, 1243, 1257, 898).
      • 그러나 expr_guard.rs가 current_timestamp를 허용하지 않아(function not allowed in metric expr: current_timestamp, 60회 발생), 에이전트가 궁여지책으로
      date_trunc('year', Birthday) - date_trunc('year', Birthday)(결과 0) 같은 무의미한 우회식을 만들어 오답을 냈습니다.

  ──────
  ### 원인 ④ SQLite 정수 나눗셈(Integer Division) 절삭 및 프로젝션 열 불일치 (45건)

  1. SQLite 정수 나눗셈 절삭 (22건):
      • SQLite에서 COUNT(...) / COUNT(...) 또는 SUM(CASE WHEN ... THEN 1 ELSE 0 END) / COUNT(...)는 양변이 모두 정수(INTEGER)이므로 소수점 이하를 버리는 정수 나눗셈으로
      동작합니다.
      • Gold SQL은 CAST(... AS REAL) / ... 또는 1.0을 곱하지만, 에이전트는 SUM(CASE WHEN element = 'o' THEN 1 ELSE 0 END) / COUNT(molecule_id)처럼 작성하여 비율/퍼센트
      결과가 0 또는 정수로 절삭되었습니다(QID 1152, 801, 197, 198, 201, 226, 227, 228, 273, 282, 47, 48 등).
  2. 프로젝션 아리티(열 개수/순서) 불일치 (41건, 이 중 5건은 값 자체는 100% 일치):
      • 에이전트가 GROUP BY major_name HAVING major_name = 'Physics Teaching'을 쓰면서 SELECT COUNT(member_id)만 적었으나, sql-mcp 파이프라인이 Grouped 모드에서 GROUP
      BY에 등장한 차원(major_name)을 함께 묶거나 에이전트가 확인용으로 차원 컬럼을 함께 프로젝션하여 Gold([[1]])와 Pred([['Physics Teaching', 1]])의 열 개수가 달라져
      탈락했습니다(QID 1394, 1331, 1344, 1346, 1359, 85 등).

  ──────
  ### 원인 ⑤ 시맨틱 SQL(D16/D23)의 표현 한계: Subquery · CTE · Window 함수 (32건)

  BIRD 벤치마크의 Moderate/Challenging 문항 중 32건은 물리 SQL에서 다음 구문을 요구합니다.

  • 비집계 비교를 위한 스칼라 서브쿼리 (29건):
      • 예: "평균 랩타임보다 빠른 선수", "전체 평균보다 차이가 큰 학교"(QID 28, 736, 761, 884, 1189 등).
      • 시맨틱 SQL은 subquery not allowed in expression(72회 발생)으로 서브쿼리와 CTE를 전면 차단합니다. 에이전트가 이를 우회하려 WHERE aCL_IgM > 1.2 * AVG(aCL_IgM)이나
      HAVING weight_kg > 0.79 * AVG(weight_kg)를 시도하면 aggregate not allowed here 또는 그룹핑 그레인 불일치로 실패합니다.
  • 윈도우 함수 RANK() OVER (...) (4건):
      • QID 17, 726 등 순위 번호를 함께 출력해야 하는 문항에서 윈도우 함수 미지원으로 순위 컬럼을 생성하지 못합니다.

  ──────
  ### 원인 ⑥ 평가 하네스(bird_eval.py)의 log_max=4000 결과 절단 버그 (18건)

  • 코드 위치: bird_eval.py:132-155 (tool_calls_from_messages)
    def tool_calls_from_messages(messages: list[Any], *, log_max: int = 4000) -> list[dict[str, Any]]:
        ...
        if log_max > 0 and len(text) > log_max:
            text = text[:log_max] + f"…(+{len(text) - log_max})"

  • 원인 메커니즘:
      • run_eval.py가 tool_calls_from_messages(messages)를 기본값 log_max=4000으로 호출하여 eval.json에 저장합니다.
      • 반환 행이 수백 건 이상인 질의(예: QID 726, 738, 773, 11, 23, 24, 27, 77, 243, 964, 1014, 1155, 1187, 1209 등 총 18건)는 execute_sql의 JSON 응답 텍스트가 4,000자를
      넘어가면서 뒷부분이 …(+3788)로 잘립니다.
      • 이후 채점기 unwrap_tool_result가 잘린 JSON을 파싱할 때 JSONDecodeError가 발생하여 {"raw": text}를 반환하고, last_successful_rows는 "rows" in body가 거짓이 되어
      pred_rows = None ("성공한 execute_sql 없음")으로 오판합니다!
      • 특히 **QID 738(superhero, 371행)**과 **QID 773(superhero, 157행)**은 에이전트가 작성한 쿼리와 실제 실행 결과가 Gold와 완벽히 일치했음에도 이 버그 때문에 오답
      처리되었습니다.

  ──────
  ## 4. 계층별 개선 권고안 (Actionable Recommendations)

  성능(EX Accuracy)을 현재 24.8%에서 70%+ 수준으로 끌어올리기 위해 우선순위별로 조치해야 할 사항입니다.

  1. [즉시 / Client Eval] bird_eval.py의 execute_sql 결과 절단 방지 및 분류기 보정 — **적용됨**
      • `tool_calls_from_messages`: execute_sql 결과는 `log_max` 문자 절단 없이 파싱된 rows를 보존하고 `row_limit`(500)만 적용.
      • `classify_failure`: gold JOIN + 에이전트 단일 큐브 자동 `semantic_layer` 휴리스틱 제거. dry_run 컴파일 SQL·행 집합의 조인/필터/타입 차이로 세분.
  2. [핵심 / Server & Semantic] One-Big-View 조인 프루닝 및 메타 생성기 개편
      • compile.rs:505-509 (compile_join_view) 개선: 모든 조인이 INNER JOIN이거나 used_view_cols가 sources[0]을 전혀 참조하지 않을 때는 sources[0]을 무조건 t_req에 넣지
      말고, 실제 참조된 테이블들(tables_for_view_cols)의 최소 연결 서브그래프(Steiner Tree)만 조인해야 합니다. 이 한 줄의 변경만으로도 77건의 불필요한 Root 테이블 강제
      조인 문제가 해결됩니다.
      • generate_meta.py 개선:
          • 단순 차수 기반 단일 스패닝 트리({db}_all) 대신, 모든 유효한 FK 간선을 보존하거나, 주요 팩트/엔티티 중심의 멀티 큐브(또는 Role-playing FK인 eye_colour_id,
          hair_colour_id 등을 별도 dimension/코드사전 as_code로 매핑)를 생성해야 합니다.
          • 동명 컬럼 충돌 시 무조건 먼저 온 테이블에 자연 이름을 주고 나중 테이블에 접두사를 붙이는 대신, 두 테이블 모두 모호성이 없도록 명명하거나 description에 출처
          물리 테이블과 의미를 명확히 기입해야 합니다.

  3. [Server Dialect / SQLite] 연도/날짜 처리 및 나눗셈/파서 가드 보완
      • SQLite date_trunc 및 날짜 비교: 에이전트가 연도 비교 시 date_trunc('year', col) = '2019'처럼 쓰는 패턴에 대응하거나, EXTRACT(YEAR FROM col) / YEAR(col)을 시맨틱
      표준 스칼라 함수로 허용하여 SQLite에서는 CAST(strftime('%Y', col) AS INTEGER)로 emit하도록 지원하면 날짜 문항 40건의 정답률이 급상승합니다.
      • GROUP BY cube.col 파서 오류 안내: sqlparser가 GROUP BY cube.col에서 Expected: (, found: .를 반환할 때, 에러 메시지에 "hint: 'cube' is a SQL reserved keyword in
      GROUP BY CUBE(...); use unqualified member name or actual cube name"을 덧붙여 주면 에이전트의 410회 무한 루프를 즉시 차단할 수 있습니다.
      • 정수 나눗셈 방지 (프롬프트 또는 컴파일러): agent_bird.py의 SYSTEM 프롬프트에 "In division /, always multiply by 1.0 (e.g. SUM(...) * 1.0 / COUNT(...)) to avoid
      integer division truncation" 규칙을 1줄 추가하면 22건의 정수 나눗셈 오답이 해결됩니다.