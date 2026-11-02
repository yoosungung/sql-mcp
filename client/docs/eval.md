# BIRD Mini-Dev Eval (문항별)

요약은 [eval_report.md](eval_report.md). 문항마다 **Gold SQL / Gold rows**와 **Agent SQL / Agent rows**를 붙여 비교한다.

- **일시**: `2026-10-29T06:28:00.509826+00:00` · scored `2026-10-29T06:28:25.678623+00:00`
- **모델**: `ollama:gemma4:e2b-mlx` · MCP `http://127.0.0.1:8090/mcp`
- **EX**: 15/48 (31.25%)

## Index

| qid | db_id | EX | reason |
|-----|-------|----|--------|
| [1312](#q1312) | student_club | PASS |  |
| [1317](#q1317) | student_club | FAIL | execute_sql은 성공했으나 gold_rows와 불일치(로직/필터/값) |
| [1322](#q1322) | student_club | FAIL | 필터/조인으로 결과 공집합(gold는 비공집합) |
| [1323](#q1323) | student_club | FAIL | 결과 행 수가 gold 대비 과도(조인 팬아웃 의심) |
| [1331](#q1331) | student_club | FAIL | execute_sql은 성공했으나 gold_rows와 불일치(로직/필터/값) |
| [1334](#q1334) | student_club | PASS |  |
| [1338](#q1338) | student_club | PASS |  |
| [1339](#q1339) | student_club | FAIL | 성공한 execute_sql 없음(도구/재시도 실패) |
| [1340](#q1340) | student_club | PASS |  |
| [1344](#q1344) | student_club | FAIL | 성공한 execute_sql 없음(도구/재시도 실패) |
| [1346](#q1346) | student_club | PASS |  |
| [1350](#q1350) | student_club | FAIL | 필터/조인으로 결과 공집합(gold는 비공집합) |
| [1351](#q1351) | student_club | FAIL | 성공한 execute_sql 없음(도구/재시도 실패) |
| [1352](#q1352) | student_club | PASS |  |
| [1356](#q1356) | student_club | PASS |  |
| [1357](#q1357) | student_club | FAIL | 성공한 execute_sql 없음(도구/재시도 실패) |
| [1359](#q1359) | student_club | FAIL | 성공한 execute_sql 없음(도구/재시도 실패) |
| [1361](#q1361) | student_club | FAIL | execute_sql은 성공했으나 gold_rows와 불일치(로직/필터/값) |
| [1362](#q1362) | student_club | FAIL | 성공한 execute_sql 없음(도구/재시도 실패) |
| [1368](#q1368) | student_club | FAIL | 성공한 execute_sql 없음(도구/재시도 실패) |
| [1371](#q1371) | student_club | FAIL | execute_sql은 성공했으나 gold_rows와 불일치(로직/필터/값) |
| [1375](#q1375) | student_club | FAIL | 성공한 execute_sql 없음(도구/재시도 실패) |
| [1376](#q1376) | student_club | PASS |  |
| [1378](#q1378) | student_club | PASS |  |
| [1380](#q1380) | student_club | PASS |  |
| [1381](#q1381) | student_club | FAIL | 성공한 execute_sql 없음(도구/재시도 실패) |
| [1387](#q1387) | student_club | FAIL | 결과 행 수가 gold 대비 과도(조인 팬아웃 의심) |
| [1389](#q1389) | student_club | PASS |  |
| [1390](#q1390) | student_club | FAIL | 성공한 execute_sql 없음(도구/재시도 실패) |
| [1392](#q1392) | student_club | PASS |  |
| [1394](#q1394) | student_club | PASS |  |
| [1398](#q1398) | student_club | FAIL | 프로젝션 열 개수/구성이 gold와 불일치 |
| [1399](#q1399) | student_club | FAIL | execute_sql은 성공했으나 gold_rows와 불일치(로직/필터/값) |
| [1401](#q1401) | student_club | FAIL | 성공한 execute_sql 없음(도구/재시도 실패) |
| [1403](#q1403) | student_club | FAIL | 성공한 execute_sql 없음(도구/재시도 실패) |
| [1404](#q1404) | student_club | FAIL | execute_sql은 성공했으나 gold_rows와 불일치(로직/필터/값) |
| [1405](#q1405) | student_club | PASS |  |
| [1409](#q1409) | student_club | FAIL | execute_sql은 성공했으나 gold_rows와 불일치(로직/필터/값) |
| [1410](#q1410) | student_club | FAIL | 성공한 execute_sql 없음(도구/재시도 실패) |
| [1411](#q1411) | student_club | FAIL | SELECT * 금지 — dimension/metric 명시 |
| [1422](#q1422) | student_club | FAIL | 결과 행 수가 gold 대비 과도(조인 팬아웃 의심) |
| [1426](#q1426) | student_club | PASS |  |
| [1427](#q1427) | student_club | FAIL | 프로젝션 열 개수/구성이 gold와 불일치 |
| [1432](#q1432) | student_club | FAIL | execute_sql은 성공했으나 gold_rows와 불일치(로직/필터/값) |
| [1435](#q1435) | student_club | FAIL | 시맨틱 식별자 미등록(unknown metric/dimension/column) |
| [1457](#q1457) | student_club | FAIL | 성공한 execute_sql 없음(도구/재시도 실패) |
| [1460](#q1460) | student_club | FAIL | 결과 행 수가 gold 대비 과도(조인 팬아웃 의심) |
| [1464](#q1464) | student_club | FAIL | 프로젝션 열 개수/구성이 gold와 불일치 |

## 문항

## student_club

### Q1312

- **db**: `student_club` · **EX**: **PASS**
- **tools**: 5: list_databases, list_tables, describe_table, sample_column, execute_sql
- **row set**: gold=1 · agent=1 · both=1 only_gold=0 only_agent=0
- **question**: What's Angela Sanders's major?
- **evidence**: Angela Sanders is the full name; full name refers to first_name, last_name; major refers to major_name.

#### Gold SQL

```sql
SELECT T2.major_name FROM member AS T1 INNER JOIN major AS T2 ON T1.link_to_major = T2.major_id WHERE T1.first_name = 'Angela' AND T1.last_name = 'Sanders'
```

#### Gold rows (1)

| c1 |
| --- |
| Business |

#### Agent SQL

```sql
SELECT T1.major_name FROM cube_join_student_club AS T1 WHERE T1.last_name = 'Sanders'
```

#### Agent rows (1)

| major_name |
| --- |
| Business |

- **final**: Business

### Q1317

- **db**: `student_club` · **EX**: **FAIL**
- **reason**: execute_sql은 성공했으나 gold_rows와 불일치(로직/필터/값)
- **tools**: 7: list_databases, list_tables, describe_table, sample_column, execute_sql, execute_sql, execute_sql
- **row set**: gold=1 · agent=1 · both=0 only_gold=1 only_agent=1
- **question**: Among the students from the Student_Club who attended the event "Women's Soccer", how many of them want a T-shirt that's in medium size?
- **evidence**: Women's Soccer is an event name; T-shirt that is in medium size refers to t_shirt_size = 'Medium'

#### Gold SQL

```sql
SELECT COUNT(T1.event_id) FROM event AS T1 INNER JOIN attendance AS T2 ON T1.event_id = T2.link_to_event INNER JOIN member AS T3 ON T2.link_to_member = T3.member_id WHERE T1.event_name = 'Women''s Soccer' AND T3.t_shirt_size = 'Medium'
```

#### Gold rows (1)

| c1 |
| --- |
| 7 |

#### Agent SQL

```sql
SELECT count(*) FROM student_club.cube_join_student_club WHERE event_name = 'Women''s Soccer' AND t_shirt_size = 'medium'
```

- **last_error**: not found: cube/student_club

#### Agent rows (1)

| count |
| --- |
| 0 |

- **final**: 0

### Q1322

- **db**: `student_club` · **EX**: **FAIL**
- **reason**: 필터/조인으로 결과 공집합(gold는 비공집합)
- **tools**: 5: list_databases, list_tables, describe_table, sample_column, execute_sql
- **row set**: gold=9 · agent=0 · both=0 only_gold=9 only_agent=0
- **question**: Among the events attended by more than 10 members of the Student_Club, how many of them are meetings?
- **evidence**: meetings events refers to type = 'Meeting'; attended by more than 10 members refers to COUNT(event_id) > 10

#### Gold SQL

```sql
SELECT T1.event_name FROM event AS T1  INNER JOIN attendance AS T2 ON T1.event_id = T2.link_to_event GROUP BY T1.event_id  HAVING COUNT(T2.link_to_event) > 10 EXCEPT SELECT T1.event_name  FROM event AS T1  WHERE T1.type = 'Meeting'
```

#### Gold rows (9)

| c1 |
| --- |
| Community Theater |
| Food Bank |
| Football game |
| Laugh Out Loud |
| November Speaker |
| October Speaker |
| Registration |
| September Speaker |
| Women's Soccer |

#### Agent SQL

```sql
SELECT COUNT(T1.event_name) FROM cube_join_student_club AS T1 WHERE (T1.event_name LIKE '%Meeting%' OR T1.event_type LIKE '%Meeting%') GROUP BY T1.event_name HAVING COUNT(*) > 10
```

#### Agent rows (0)

_empty_

- **final**: 0

### Q1323

- **db**: `student_club` · **EX**: **FAIL**
- **reason**: 결과 행 수가 gold 대비 과도(조인 팬아웃 의심)
- **tools**: 5: list_databases, list_tables, describe_table, sample_column, execute_sql
- **row set**: gold=8 · agent=38 · both=8 only_gold=0 only_agent=8
- **question**: List all the names of events that had an attendance of over 20 students but were not fundraisers.
- **evidence**: name of events refers to event_name; attendance of over 20 students COUNT(event_id) > 20.

#### Gold SQL

```sql
SELECT T1.event_name FROM event AS T1 INNER JOIN attendance AS T2 ON T1.event_id = T2.link_to_event GROUP BY T1.event_id HAVING COUNT(T2.link_to_event) > 20 EXCEPT SELECT T1.event_name FROM event AS T1  WHERE T1.type = 'Fundraiser'
```

#### Gold rows (8)

| c1 |
| --- |
| November Meeting |
| November Speaker |
| October Meeting |
| October Speaker |
| Registration |
| September Meeting |
| September Speaker |
| Yearly Kickoff |

#### Agent SQL

```sql
SELECT event_name FROM cube_join_student_club WHERE event_type <> 'Fundraiser' AND budget_amount > 20
```

#### Agent rows (38)

| event_name |
| --- |
| September Speaker |
| October Meeting |
| February Speaker |
| February Speaker |
| April Meeting |
| September Speaker |
| February Speaker |
| April Speaker |
| January Speaker |
| November Meeting |
| September Meeting |
| November Speaker |
| March Meeting |
| Registration |
| October Meeting |
| March Meeting |
| April Speaker |
| March Speaker |
| February Meeting |
| January Speaker |
| October Speaker |
| Yearly Kickoff |
| November Meeting |
| March Speaker |
| January Speaker |
| March Speaker |
| February Meeting |
| January Meeting |
| January Meeting |
| October Speaker |
| April Meeting |
| October Speaker |
| September Speaker |
| April Speaker |
| November Speaker |
| Yearly Kickoff |
| September Meeting |
| November Speaker |

- **final**: The events with an attendance over 20 students that were not fundraisers are:
* September Speaker
* October Meeting
* February Speaker
* February Speaker
* April Meeting
* September Speaker
* February Speaker
* April Speaker
* January Speaker
* November Meeting
* September Meeting
* November Speaker
* March Meeting
* Registration
* October Meeting
* March Meeting
* April Speaker
* March Speaker
* …

### Q1331

- **db**: `student_club` · **EX**: **FAIL**
- **reason**: execute_sql은 성공했으나 gold_rows와 불일치(로직/필터/값)
- **tools**: 12: list_databases, list_tables, describe_table, sample_column, list_tables, list_tables, search_columns, sample_column, execute_sql, describe_table, sample_column, execute_sql
- **row set**: gold=1 · agent=1 · both=0 only_gold=1 only_agent=1
- **question**: What is the amount of the funds that the Vice President received?
- **evidence**: 'Vice President' is a position of Student Club; funds received refers to amount.

#### Gold SQL

```sql
SELECT T2.amount FROM member AS T1 INNER JOIN income AS T2 ON T1.member_id = T2.link_to_member WHERE T1.position = 'Vice President'
```

#### Gold rows (1)

| c1 |
| --- |
| 50 |

#### Agent SQL

```sql
SELECT SUM(budget_amount) FROM cube_join_student_club WHERE position = 'Vice President'
```

#### Agent rows (1)

| sum_budget_amount |
| --- |
| 1240 |

- **final**: The amount of the funds that the Vice President received is 1240.

### Q1334

- **db**: `student_club` · **EX**: **PASS**
- **tools**: 5: list_databases, list_tables, describe_table, sample_column, execute_sql
- **row set**: gold=3 · agent=3 · both=3 only_gold=0 only_agent=0
- **question**: List the full name of the Student_Club members that grew up in Illinois state.
- **evidence**: full name of member refers to first_name, last_name

#### Gold SQL

```sql
SELECT T1.first_name, T1.last_name FROM member AS T1 INNER JOIN zip_code AS T2 ON T1.zip = T2.zip_code WHERE T2.state = 'Illinois'
```

#### Gold rows (3)

| c1 | c2 |
| --- | --- |
| Trent | Smith |
| Tyler | Hewitt |
| Annabella | Warren |

#### Agent SQL

```sql
SELECT first_name, last_name FROM cube_join_student_club WHERE state = 'Illinois'
```

#### Agent rows (3)

| first_name | last_name |
| --- | --- |
| Trent | Smith |
| Tyler | Hewitt |
| Annabella | Warren |

- **final**: The full names of the Student_Club members that grew up in Illinois state are Trent Smith, Tyler Hewitt, and Annabella Warren.

### Q1338

- **db**: `student_club` · **EX**: **PASS**
- **tools**: 5: list_databases, list_tables, search_columns, sample_column, execute_sql
- **row set**: gold=3 · agent=3 · both=1 only_gold=0 only_agent=0
- **question**: Was each expense in October Meeting on October 8, 2019 approved?
- **evidence**: event_name = 'October Meeting' where event_date = '2019-10-08'; approved = True means expenses was approved; approved = False means expenses was not approved

#### Gold SQL

```sql
SELECT T3.approved FROM event AS T1 INNER JOIN budget AS T2 ON T1.event_id = T2.link_to_event INNER JOIN expense AS T3 ON T2.budget_id = T3.link_to_budget WHERE T1.event_name = 'October Meeting' AND T1.event_date LIKE '2019-10-08%'
```

#### Gold rows (3)

| c1 |
| --- |
| true |
| true |
| true |

#### Agent SQL

```sql
SELECT expense_approved FROM cube_join_student_club WHERE expense_date = '2019-10-08'
```

#### Agent rows (3)

| expense_approved |
| --- |
| true |
| true |
| true |

- **final**: All expenses for the "October Meeting on October 8, 2019" were approved.

### Q1339

- **db**: `student_club` · **EX**: **FAIL**
- **reason**: 성공한 execute_sql 없음(도구/재시도 실패)
- **tools**: 7: list_databases, list_tables, describe_table, sample_column, sample_column, execute_sql, execute_sql
- **row set**: gold=1 · agent=—
- **question**: Calculate the total average cost that Elijah Allen spent in the events on September and October.
- **evidence**: Elijah Allen is the full name; full name refers to first_name, last_name; The 5th and 6th string of the expense_date in the expense table can refer to month; events in September and October refers to month(expense_date) = 9 OR month(expense_date) = 10

#### Gold SQL

```sql
SELECT AVG(T2.cost) FROM member AS T1 INNER JOIN expense AS T2 ON T1.member_id = T2.link_to_member WHERE T1.last_name = 'Allen' AND T1.first_name = 'Elijah' AND (SUBSTR(T2.expense_date, 6, 2) = '09' OR SUBSTR(T2.expense_date, 6, 2) = '10')
```

#### Gold rows (1)

| c1 |
| --- |
| 74.0775 |

#### Agent SQL

```sql
SELECT AVG(expense_cost) FROM student_club.cube_join_student_club WHERE first_name = 'Elijah Allen' AND (event_date LIKE '2019-09-%' OR event_date LIKE '2019-10-%' OR event_date LIKE '2020-09-%' OR event_date LIKE '2020-10-%')
```

- **last_error**: not found: cube/student_club

#### Agent rows (—)

_없음_

- **final**: SELECT AVG(expense_cost) FROM student_club.cube_join_student_club WHERE first_name = 'Elijah Allen' AND (event_date LIKE '2019-09-%' OR event_date LIKE '2019-10-%' OR event_date LIKE '2020-09-%' OR event_date LIKE '2020-10-%')

### Q1340

- **db**: `student_club` · **EX**: **PASS**
- **tools**: 10: list_databases, list_tables, describe_table, sample_column, execute_sql, execute_sql, execute_sql, execute_sql, execute_sql, execute_sql
- **row set**: gold=1 · agent=1 · both=1 only_gold=0 only_agent=0
- **question**: Calculate the difference of the total amount spent in all events by the Student_Club in year 2019 and 2020.
- **evidence**: The first 4 strings of the event_date values in the event table can represent year; The difference of the total amount spent = SUBTRACT(spent where YEAR(event_date) = 2019, spent where YEAR(event_date) = 2020)

#### Gold SQL

```sql
SELECT SUM(CASE WHEN SUBSTR(T1.event_date, 1, 4) = '2019' THEN T2.spent ELSE 0 END) - SUM(CASE WHEN SUBSTR(T1.event_date, 1, 4) = '2020' THEN T2.spent ELSE 0 END) AS num FROM event AS T1 INNER JOIN budget AS T2 ON T1.event_id = T2.link_to_event
```

#### Gold rows (1)

| c1 |
| --- |
| 2086.05 |

#### Agent SQL

```sql
SELECT SUM(CASE WHEN YEAR(event_date) = 2019 THEN expense_cost ELSE 0 END) - SUM(CASE WHEN YEAR(event_date) = 2020 THEN expense_cost ELSE 0 END) FROM cube_join_student_club
```

- **last_error**: not found: cube/student_club

#### Agent rows (1)

| sum_case_when_year_event_date_2019_then_expense_cost_else_0_end_sum_case_when_year_event_date_2020_then_expense_cost_else_0_end |
| --- |
| 2086.05 |

- **final**: The difference in the total amount spent in all events by the Student_Club between 2019 and 2020 is 2086.05.

### Q1344

- **db**: `student_club` · **EX**: **FAIL**
- **reason**: 성공한 execute_sql 없음(도구/재시도 실패)
- **max_retries**: 동일 SQL 검증 실패 3회 중단
- **tools**: 7: list_databases, list_tables, describe_table, sample_column, execute_sql, execute_sql, execute_sql
- **row set**: gold=1 · agent=—
- **question**: What was the notes of the fundraising on 2019/9/14?
- **evidence**: fundraising on 2019/9/14 refers to source = 'Fundraising' where date_received = '2019-09-14'

#### Gold SQL

```sql
SELECT notes FROM income WHERE source = 'Fundraising' AND date_received = '2019-09-14'
```

#### Gold rows (1)

| c1 |
| --- |
| Secured donations to help pay for speaker gifts. |

#### Agent SQL

```sql
SELECT event_notes FROM student_club.cube_join_student_club WHERE event_date = '2019-09-14'
```

- **last_error**: not found: cube/student_club

#### Agent rows (—)

_없음_

- **final**: Stopped: execute_sql validation/execution failed 3 times with the same SQL. Aborting instead of retrying.

### Q1346

- **db**: `student_club` · **EX**: **PASS**
- **tools**: 5: list_databases, list_tables, describe_table, sample_column, execute_sql
- **row set**: gold=1 · agent=1 · both=1 only_gold=0 only_agent=0
- **question**: Tell the phone number of "Carlo Jacobs".
- **evidence**: Carlo Jacobs is the full name; full name refers to first_name, last_name;

#### Gold SQL

```sql
SELECT phone FROM member WHERE first_name = 'Carlo' AND last_name = 'Jacobs'
```

#### Gold rows (1)

| c1 |
| --- |
| 928-555-2577 |

#### Agent SQL

```sql
SELECT phone FROM cube_join_student_club WHERE first_name = 'Carlo' AND last_name = 'Jacobs'
```

#### Agent rows (1)

| phone |
| --- |
| 928-555-2577 |

- **final**: The phone number for Carlo Jacobs is 928-555-2577.

### Q1350

- **db**: `student_club` · **EX**: **FAIL**
- **reason**: 필터/조인으로 결과 공집합(gold는 비공집합)
- **tools**: 8: list_databases, list_tables, describe_table, sample_column, sample_column, execute_sql, sample_column, execute_sql
- **row set**: gold=1 · agent=0 · both=0 only_gold=1 only_agent=0
- **question**: What is the status of the event which bought "Post Cards, Posters" on 2019/8/20?
- **evidence**: 'Post Cards, Posters' is an expense description; on 2019/8/20 refers to expense_date = '2019-8-20'; status of event refers to event_status

#### Gold SQL

```sql
SELECT T1.event_status FROM budget AS T1 INNER JOIN expense AS T2 ON T1.budget_id = T2.link_to_budget WHERE T2.expense_description = 'Post Cards, Posters' AND T2.expense_date = '2019-08-20'
```

#### Gold rows (1)

| c1 |
| --- |
| Closed |

#### Agent SQL

```sql
SELECT event_status FROM cube_join_student_club WHERE expense_description = 'Post Cards, Posters' AND event_date = '2019-08-20'
```

#### Agent rows (0)

_empty_

- **final**: No record was found for an event that bought "Post Cards, Posters" on 2019/8/20.

### Q1351

- **db**: `student_club` · **EX**: **FAIL**
- **reason**: 성공한 execute_sql 없음(도구/재시도 실패)
- **tools**: —
- **row set**: gold=1 · agent=—
- **question**: What was Brent Thomason's major?
- **evidence**: Brent Thomason is the full name; full name refers to first_name, last_name; major refers to major_name

#### Gold SQL

```sql
SELECT T2.major_name FROM member AS T1 INNER JOIN major AS T2 ON T1.link_to_major = T2.major_id WHERE T1.first_name = 'Brent' AND T1.last_name = 'Thomason'
```

#### Gold rows (1)

| c1 |
| --- |
| Business Education |

#### Agent SQL

—

#### Agent rows (—)

_없음_

- **final**: I do not have access to general knowledge databases to answer questions about individuals. My capabilities are limited to analyzing data within the provided SQL schema using the available tools. Therefore, I cannot answer what Brent Thomason's major was.

### Q1352

- **db**: `student_club` · **EX**: **PASS**
- **tools**: 7: list_databases, list_tables, describe_table, sample_column, sample_column, execute_sql, execute_sql
- **row set**: gold=1 · agent=1 · both=1 only_gold=0 only_agent=0
- **question**: For all the club members from "Business" major, how many of them wear medium size t-shirt?
- **evidence**: 'Business' is a major name; wear medium size t-shirt refers to t_shirt_size = 'Medium'

#### Gold SQL

```sql
SELECT COUNT(T1.member_id) FROM member AS T1 INNER JOIN major AS T2 ON T1.link_to_major = T2.major_id WHERE T2.major_name = 'Business' AND T1.t_shirt_size = 'Medium'
```

#### Gold rows (1)

| c1 |
| --- |
| 1 |

#### Agent SQL

```sql
SELECT count(*) FROM cube_join_student_club WHERE major_name = 'Business' AND t_shirt_size = 'Medium'
```

- **last_error**: not found: cube/student_club

#### Agent rows (1)

| count |
| --- |
| 1 |

- **final**: The count is 1.

### Q1356

- **db**: `student_club` · **EX**: **PASS**
- **tools**: 5: list_databases, list_tables, describe_table, sample_column, execute_sql
- **row set**: gold=1 · agent=1 · both=1 only_gold=0 only_agent=0
- **question**: Which department was the President of the club in?
- **evidence**: 'President' is a position of Student Club

#### Gold SQL

```sql
SELECT T2.department FROM member AS T1 INNER JOIN major AS T2 ON T1.link_to_major = T2.major_id WHERE T1.position = 'President'
```

#### Gold rows (1)

| c1 |
| --- |
| Liberal Arts Program |

#### Agent SQL

```sql
SELECT department FROM cube_join_student_club WHERE position = 'President'
```

#### Agent rows (1)

| department |
| --- |
| Liberal Arts Program |

- **final**: The President of the club was in the Liberal Arts Program department.

### Q1357

- **db**: `student_club` · **EX**: **FAIL**
- **reason**: 성공한 execute_sql 없음(도구/재시도 실패)
- **tools**: 8: list_databases, list_tables, describe_table, sample_column, execute_sql, sample_column, execute_sql, execute_sql
- **row set**: gold=2 · agent=—
- **question**: State the date Connor Hilton paid his/her dues.
- **evidence**: Connor Hilton is the full name; full name refers to first_name, last_name; date the dues was paid refers to date_received where source = 'Dues';

#### Gold SQL

```sql
SELECT T2.date_received FROM member AS T1 INNER JOIN income AS T2 ON T1.member_id = T2.link_to_member WHERE T1.first_name = 'Connor' AND T1.last_name = 'Hilton' AND T2.source = 'Dues'
```

#### Gold rows (2)

| c1 |
| --- |
| 2019-10-02 |
| 2019-09-12 |

#### Agent SQL

```sql
SELECT expense_date FROM student_club.cube_join_student_club WHERE first_name = 'Connor'
```

- **last_error**: not found: cube/student_club

#### Agent rows (—)

_없음_

- **final**: SELECT expense_date FROM student_club.cube_join_student_club WHERE first_name = 'Connor' OR last_name = 'Hilton'

### Q1359

- **db**: `student_club` · **EX**: **FAIL**
- **reason**: 성공한 execute_sql 없음(도구/재시도 실패)
- **max_retries**: 동일 SQL 검증 실패 3회 중단
- **tools**: 12: list_databases, list_tables, describe_table, search_columns, describe_table, sample_column, execute_sql, execute_sql, execute_sql, execute_sql, execute_sql, execute_sql
- **row set**: gold=1 · agent=—
- **question**: How many times was the budget in Advertisement for "Yearly Kickoff" meeting more than "October Meeting"?
- **evidence**: budget in Advertisement refer to category = 'Advertisement' in the budget table; DIVIDE(SUM(amount when event_name = 'Yearly Kickoff'), SUM(amount when event_name = 'October Meeting'))

#### Gold SQL

```sql
SELECT CAST(SUM(CASE WHEN T2.event_name = 'Yearly Kickoff' THEN T1.amount ELSE 0 END) AS REAL) / SUM(CASE WHEN T2.event_name = 'October Meeting' THEN T1.amount ELSE 0 END) FROM budget AS T1 INNER JOIN event AS T2 ON T1.link_to_event = T2.event_id WHERE T1.category = 'Advertisement' AND T2.type = 'Meeting'
```

#### Gold rows (1)

| c1 |
| --- |
| 2.727272727272727 |

#### Agent SQL

```sql
SELECT COUNT(T1.budget_amount) FROM student_club AS T1 WHERE T1.event_name = 'Yearly Kickoff' AND T1.budget_amount > (SELECT T2.budget_amount FROM student_club AS T2 WHERE T2.event_name = 'October Meeting' AND T2.budget_amount > 0)
```

- **last_error**: not found: cube/student_club

#### Agent rows (—)

_없음_

- **final**: Stopped: execute_sql validation/execution failed 3 times with the same SQL. Aborting instead of retrying.

### Q1361

- **db**: `student_club` · **EX**: **FAIL**
- **reason**: execute_sql은 성공했으나 gold_rows와 불일치(로직/필터/값)
- **tools**: 7: list_databases, list_tables, describe_table, sample_column, execute_sql, execute_sql, execute_sql
- **row set**: gold=1 · agent=1 · both=0 only_gold=1 only_agent=1
- **question**: What is the total cost of the pizzas for all the events?
- **evidence**: total cost of the pizzas refers to SUM(cost) where expense_description = 'Pizza'

#### Gold SQL

```sql
SELECT SUM(cost) FROM expense WHERE expense_description = 'Pizza'
```

#### Gold rows (1)

| c1 |
| --- |
| 600.11 |

#### Agent SQL

```sql
SELECT SUM(expense_cost) FROM cube_join_student_club
```

- **last_error**: not found: cube/student_club

#### Agent rows (1)

| sum_expense_cost |
| --- |
| 2086.05 |

- **final**: The total cost of the pizzas for all the events is 2086.05.

### Q1362

- **db**: `student_club` · **EX**: **FAIL**
- **reason**: 성공한 execute_sql 없음(도구/재시도 실패)
- **tools**: 3: list_databases, list_tables, list_tables
- **row set**: gold=1 · agent=—
- **question**: How many cities are there in Orange County, Virginia?
- **evidence**: Orange County is the county name, Virginia is the state name

#### Gold SQL

```sql
SELECT COUNT(city) FROM zip_code WHERE county = 'Orange County' AND state = 'Virginia'
```

#### Gold rows (1)

| c1 |
| --- |
| 7 |

#### Agent SQL

—

#### Agent rows (—)

_없음_

- **final**: I cannot answer this question as none of the available databases appear to contain geographical data for counties and cities. The available databases are related to topics such as superheroes, student clubs, toxicology, thrombosis prediction, California schools, and a formula.

### Q1368

- **db**: `student_club` · **EX**: **FAIL**
- **reason**: 성공한 execute_sql 없음(도구/재시도 실패)
- **tools**: 4: list_databases, list_tables, describe_table, sample_column
- **row set**: gold=1 · agent=—
- **question**: What does the person with the phone number "809-555-3360" major in?
- **evidence**: major in refers to major_name

#### Gold SQL

```sql
SELECT T2.major_name FROM member AS T1 INNER JOIN major AS T2 ON T1.link_to_major = T2.major_id WHERE T1.phone = '809-555-3360'
```

#### Gold rows (1)

| c1 |
| --- |
| Agricultural Education |

#### Agent SQL

—

#### Agent rows (—)

_없음_

- **final**: I am sorry, but I cannot answer this question as the required information (phone number) is not available in the dataset I can query.

### Q1371

- **db**: `student_club` · **EX**: **FAIL**
- **reason**: execute_sql은 성공했으나 gold_rows와 불일치(로직/필터/값)
- **tools**: 7: list_databases, list_tables, describe_table, sample_column, execute_sql, execute_sql, execute_sql
- **row set**: gold=1 · agent=1 · both=0 only_gold=1 only_agent=1
- **question**: How many members attended the "Women's Soccer" event?
- **evidence**: 'Women's Soccer' is the event name;

#### Gold SQL

```sql
SELECT COUNT(T2.link_to_member) FROM event AS T1 INNER JOIN attendance AS T2 ON T1.event_id = T2.link_to_event WHERE T1.event_name = 'Women''s Soccer'
```

#### Gold rows (1)

| c1 |
| --- |
| 17 |

#### Agent SQL

```sql
SELECT count(*) FROM cube_join_student_club WHERE event_name = 'Women''s Soccer'
```

- **last_error**: not found: cube/student_club

#### Agent rows (1)

| count |
| --- |
| 1 |

- **final**: 1

### Q1375

- **db**: `student_club` · **EX**: **FAIL**
- **reason**: 성공한 execute_sql 없음(도구/재시도 실패)
- **tools**: 3: list_databases, list_tables, search_columns
- **row set**: gold=5 · agent=—
- **question**: List all the members of the "School of Applied Sciences, Technology and Education" department.
- **evidence**: list all members means to list all the full name; full name refers to first_name, last_name;

#### Gold SQL

```sql
SELECT T1.first_name, T1.last_name FROM member AS T1 INNER JOIN major AS T2 ON T1.link_to_major = T2.major_id WHERE T2.department = 'School of Applied Sciences, Technology and Education'
```

#### Gold rows (5)

| c1 | c2 |
| --- | --- |
| Angela | Sanders |
| Dean | O'Reilly |
| Edwardo | Ing |
| Mccauley | Nash |
| Brent | Thomason |

#### Agent SQL

—

#### Agent rows (—)

_없음_

- **final**: I am sorry, but I cannot find any tables or columns related to a "School of Applied Sciences, Technology and Education" department in the available databases. Please provide the name of a database you are interested in, or check if the information is stored in a different system.

### Q1376

- **db**: `student_club` · **EX**: **PASS**
- **tools**: 5: list_databases, list_tables, describe_table, sample_column, execute_sql
- **row set**: gold=1 · agent=1 · both=1 only_gold=0 only_agent=0
- **question**: Among all the closed events, which event has the highest spend-to-budget ratio?
- **evidence**: closed events refers to event_name where status = 'Closed'; highest spend-to budget ratio refers to MAX(DIVIDE(spent, amount))

#### Gold SQL

```sql
SELECT T2.event_name FROM budget AS T1 INNER JOIN event AS T2 ON T1.link_to_event = T2.event_id WHERE T2.status = 'Closed' ORDER BY T1.spent / T1.amount DESC LIMIT 1
```

#### Gold rows (1)

| c1 |
| --- |
| September Speaker |

#### Agent SQL

```sql
SELECT event_name FROM cube_join_student_club WHERE event_status = 'Closed' ORDER BY budget_spent * 1.0 / budget_amount DESC LIMIT 1
```

#### Agent rows (1)

| event_name |
| --- |
| September Speaker |

- **final**: The event with the highest spend-to-budget ratio among closed events is "September Speaker".

### Q1378

- **db**: `student_club` · **EX**: **PASS**
- **tools**: 7: list_databases, list_tables, describe_table, sample_column, execute_sql, execute_sql, execute_sql
- **row set**: gold=1 · agent=1 · both=1 only_gold=0 only_agent=0
- **question**: What is the highest amount of budget spend for an event?
- **evidence**: highest amount of budget spend refers to MAX(spent)

#### Gold SQL

```sql
SELECT MAX(spent) FROM budget
```

#### Gold rows (1)

| c1 |
| --- |
| 327.07 |

#### Agent SQL

```sql
SELECT MAX(budget_spent) FROM cube_join_student_club
```

- **last_error**: not found: cube/student_club

#### Agent rows (1)

| max_budget_spent |
| --- |
| 327.07 |

- **final**: The highest amount of budget spend for an event is 327.07.

### Q1380

- **db**: `student_club` · **EX**: **PASS**
- **tools**: 5: list_databases, list_tables, describe_table, sample_column, execute_sql
- **row set**: gold=1 · agent=1 · both=1 only_gold=0 only_agent=0
- **question**: What is the total amount of money spent for food?
- **evidence**: total amount of money spent refers to SUM(spent); spent for food refers to category = 'Food'

#### Gold SQL

```sql
SELECT SUM(spent) FROM budget WHERE category = 'Food'
```

#### Gold rows (1)

| c1 |
| --- |
| 1234.73 |

#### Agent SQL

```sql
SELECT SUM(expense_cost) FROM cube_join_student_club WHERE budget_category = 'Food'
```

#### Agent rows (1)

| sum_expense_cost |
| --- |
| 1234.73 |

- **final**: The total amount of money spent for food is 1234.73.

### Q1381

- **db**: `student_club` · **EX**: **FAIL**
- **reason**: 성공한 execute_sql 없음(도구/재시도 실패)
- **tools**: 6: list_databases, list_tables, describe_table, sample_column, execute_sql, execute_sql
- **row set**: gold=29 · agent=—
- **question**: List the name of students that have attended more than 7 events.
- **evidence**: name of students means the full name; full name refers to first_name, last_name; attended more than 7 events refers to COUNT(link_to_event) > 7

#### Gold SQL

```sql
SELECT T1.first_name, T1.last_name FROM member AS T1 INNER JOIN attendance AS T2 ON T1.member_id = T2.link_to_member GROUP BY T2.link_to_member HAVING COUNT(T2.link_to_event) > 7
```

#### Gold rows (29)

| c1 | c2 |
| --- | --- |
| Grant | Gilmour |
| Luisa | Guidi |
| Connor | Hilton |
| Sacha | Harrison |
| Carlo | Jacobs |
| Phillip | Cullen |
| Matthew | Snay |
| Adele | Deleon |
| Casey | Mason |
| Trent | Smith |
| Rafi | Mckee |
| Katy | Balentine |
| Dean | O'Reilly |
| Emily | Jaquith |
| Edwardo | Ing |
| Amy | Firth |
| Mccauley | Nash |
| Tyler | Hewitt |
| Maya | Mclean |
| Vincent | Ratcliffe |
| Adela | O'Gallagher |
| Garrett | Gerke |
| Keaton | Mccray |
| Keith | Dunlop |
| Elijah | Allen |
| Madelyn | Nicholson |
| Annabella | Warren |
| Kevin | Pierce |
| Sherri | Ramsey |

#### Agent SQL

```sql
SELECT first_name, last_name FROM student_club.cube_join_student_club GROUP BY first_name, last_name HAVING count(*) > 7
```

- **last_error**: not found: cube/student_club

#### Agent rows (—)

_없음_

- **final**: SELECT first_name, last_name FROM student_club.cube_join_student_club GROUP BY first_name, last_name HAVING count(*) > 7

### Q1387

- **db**: `student_club` · **EX**: **FAIL**
- **reason**: 결과 행 수가 gold 대비 과도(조인 팬아웃 의심)
- **tools**: 5: list_databases, list_tables, describe_table, sample_column, execute_sql
- **row set**: gold=4 · agent=30 · both=2 only_gold=0 only_agent=28
- **question**: Which student has been entrusted to manage the budget for the Yearly Kickoff?
- **evidence**: name of students means the full name; full name refers to first_name, last_name;'Yearly Kickoff' is an event name;

#### Gold SQL

```sql
SELECT T4.first_name, T4.last_name FROM event AS T1 INNER JOIN budget AS T2 ON T1.event_id = T2.link_to_event INNER JOIN expense AS T3 ON T2.budget_id = T3.link_to_budget INNER JOIN member AS T4 ON T3.link_to_member = T4.member_id WHERE T1.event_name = 'Yearly Kickoff'
```

#### Gold rows (4)

| c1 | c2 |
| --- | --- |
| Sacha | Harrison |
| Elijah | Allen |
| Elijah | Allen |
| Sacha | Harrison |

#### Agent SQL

```sql
SELECT first_name, last_name FROM cube_join_student_club WHERE event_name LIKE '%Yearly Kickoff%'
```

#### Agent rows (30)

| first_name | last_name |
| --- | --- |
| Annabella | Warren |
| Garrett | Gerke |
| Amy | Firth |
| Grant | Gilmour |
| Elijah | Allen |
| Vincent | Ratcliffe |
| Katy | Balentine |
| Mccauley | Nash |
| Luisa | Guidi |
| Rafi | Mckee |
| Maya | Mclean |
| Randy | Woodard |
| Matthew | Snay |
| Adele | Deleon |
| Adela | O'Gallagher |
| Sacha | Harrison |
| Casey | Mason |
| Keith | Dunlop |
| Madelyn | Nicholson |
| Edwardo | Ing |
| Trent | Smith |
| Sherri | Ramsey |
| Dean | O'Reilly |
| Connor | Hilton |
| Kevin | Pierce |
| Keaton | Mccray |
| Carlo | Jacobs |
| Phillip | Cullen |
| Tyler | Hewitt |
| Emily | Jaquith |

- **final**: The students who have been entrusted to manage the budget for the Yearly Kickoff are: Annabella Warren, Garrett Gerke, Amy Firth, Grant Gilmour, Elijah Allen, Vincent Ratcliffe, Katy Balentine, Mccauley Nash, Luisa Guidi, Rafi McKee, Maya Mclean, Randy Woodard, Matthew Snay, Adele Deleon, Adela O'Gallagher, Sacha Harrison, Casey Mason, Keith Dunlop, Madelyn Nicholson, Edwardo Ing, Trent Smith, She…

### Q1389

- **db**: `student_club` · **EX**: **PASS**
- **tools**: 4: list_databases, list_tables, describe_table, execute_sql
- **row set**: gold=1 · agent=1 · both=1 only_gold=0 only_agent=0
- **question**: Which event has the lowest cost?
- **evidence**: event refers to event_name; lowest cost means MIN(cost)

#### Gold SQL

```sql
SELECT T1.event_name FROM event AS T1 INNER JOIN budget AS T2 ON T1.event_id = T2.link_to_event INNER JOIN expense AS T3 ON T2.budget_id = T3.link_to_budget ORDER BY T3.cost LIMIT 1
```

#### Gold rows (1)

| c1 |
| --- |
| November Speaker |

#### Agent SQL

```sql
SELECT event_name FROM cube_join_student_club ORDER BY expense_cost ASC LIMIT 1
```

#### Agent rows (1)

| event_name |
| --- |
| November Speaker |

- **final**: November Speaker

### Q1390

- **db**: `student_club` · **EX**: **FAIL**
- **reason**: 성공한 execute_sql 없음(도구/재시도 실패)
- **max_retries**: 동일 SQL 검증 실패 3회 중단
- **tools**: 8: list_databases, list_tables, describe_table, sample_column, execute_sql, execute_sql, execute_sql, execute_sql
- **row set**: gold=1 · agent=—
- **question**: Based on the total cost for all event, what is the percentage of cost for Yearly Kickoff event?
- **evidence**: percentage = DIVIDE(SUM(cost where event_name = 'Yearly Kickoff'), SUM(cost)) * 100

#### Gold SQL

```sql
SELECT CAST(SUM(CASE WHEN T1.event_name = 'Yearly Kickoff' THEN T3.cost ELSE 0 END) AS REAL) * 100 / SUM(T3.cost) FROM event AS T1 INNER JOIN budget AS T2 ON T1.event_id = T2.link_to_event INNER JOIN expense AS T3 ON T2.budget_id = T3.link_to_budget
```

#### Gold rows (1)

| c1 |
| --- |
| 21.530164665276477 |

#### Agent SQL

```sql
SELECT (SUM(CASE WHEN event_name = 'Yearly Kickoff' THEN expense_cost ELSE 0 END) * 100.0 / SUM(expense_cost)) FROM student_club
```

- **last_error**: not found: cube/student_club

#### Agent rows (—)

_없음_

- **final**: Stopped: execute_sql validation/execution failed 3 times with the same SQL. Aborting instead of retrying.

### Q1392

- **db**: `student_club` · **EX**: **PASS**
- **tools**: 6: list_databases, list_tables, describe_table, sample_column, execute_sql, execute_sql
- **row set**: gold=1 · agent=1 · both=1 only_gold=0 only_agent=0
- **question**: Indicate the top source of funds received in September 2019 based on their amount.
- **evidence**: top source funds refers to MAX(source); September 2019 means date_received BETWEEN '2019-09-01' and '2019-09-30'

#### Gold SQL

```sql
SELECT source FROM income WHERE date_received BETWEEN '2019-09-01' and '2019-09-30' ORDER BY source DESC LIMIT 1
```

#### Gold rows (1)

| c1 |
| --- |
| School Appropration |

#### Agent SQL

```sql
SELECT income_source FROM cube_join_student_club WHERE income_date_received >= '2019-09-01' AND income_date_received <= '2019-09-30' GROUP BY income_source ORDER BY SUM(income_amount) DESC LIMIT 1
```

- **last_error**: not found: cube/student_club

#### Agent rows (1)

| income_source |
| --- |
| School Appropration |

- **final**: The top source of funds received in September 2019 based on their amount was School Appropration.

### Q1394

- **db**: `student_club` · **EX**: **PASS**
- **tools**: 6: list_databases, list_tables, describe_table, sample_column, sample_column, execute_sql
- **row set**: gold=1 · agent=1 · both=1 only_gold=0 only_agent=0
- **question**: How many members of the Student_Club have major in 'Physics Teaching'?
- **evidence**: 'Physics Teaching' is the major_name;

#### Gold SQL

```sql
SELECT COUNT(T2.member_id) FROM major AS T1 INNER JOIN member AS T2 ON T1.major_id = T2.link_to_major WHERE T1.major_name = 'Physics Teaching'
```

#### Gold rows (1)

| c1 |
| --- |
| 1 |

#### Agent SQL

```sql
SELECT count(*) FROM cube_join_student_club WHERE major_name = 'Physics Teaching'
```

#### Agent rows (1)

| count |
| --- |
| 1 |

- **final**: There is 1 member of the Student_Club with a major in 'Physics Teaching'.

### Q1398

- **db**: `student_club` · **EX**: **FAIL**
- **reason**: 프로젝션 열 개수/구성이 gold와 불일치
- **tools**: 4: list_databases, list_tables, describe_table, execute_sql
- **row set**: gold=1 · agent=1 · both=0 only_gold=1 only_agent=1
- **question**: Name the event with the highest amount spent on advertisement.
- **evidence**: Name of event refers to event_name; highest amount spent on advertisement refers to MAX(spent) where category = 'Advertisement'

#### Gold SQL

```sql
SELECT T2.event_name FROM budget AS T1 INNER JOIN event AS T2 ON T1.link_to_event = T2.event_id WHERE T1.category = 'Advertisement' ORDER BY T1.spent DESC LIMIT 1
```

#### Gold rows (1)

| c1 |
| --- |
| Yearly Kickoff |

#### Agent SQL

```sql
SELECT event_name, budget_spent FROM cube_join_student_club ORDER BY budget_spent DESC LIMIT 1
```

#### Agent rows (1)

| event_name | budget_spent |
| --- | --- |
| Yearly Kickoff | 327.07 |

- **final**: The event with the highest amount spent is Yearly Kickoff.

### Q1399

- **db**: `student_club` · **EX**: **FAIL**
- **reason**: execute_sql은 성공했으나 gold_rows와 불일치(로직/필터/값)
- **tools**: 6: list_databases, list_tables, describe_table, sample_column, execute_sql, execute_sql
- **row set**: gold=14 · agent=1 · both=0 only_gold=2 only_agent=1
- **question**: Did Maya Mclean attend the 'Women's Soccer' event?
- **evidence**: Maya Mclean is the full name; full name refers to first_name, last_name; 'Women's Soccer' is an event_name

#### Gold SQL

```sql
SELECT CASE WHEN T3.event_name = 'Women''s Soccer' THEN 'YES' END AS result FROM member AS T1 INNER JOIN attendance AS T2 ON T1.member_id = T2.link_to_member INNER JOIN event AS T3 ON T2.link_to_event = T3.event_id WHERE T1.first_name = 'Maya' AND T1.last_name = 'Mclean'
```

#### Gold rows (14)

| c1 |
| --- |
| YES |
|  |
|  |
|  |
|  |
|  |
|  |
|  |
|  |
|  |
|  |
|  |
|  |
|  |

#### Agent SQL

```sql
SELECT event_name FROM cube_join_student_club WHERE first_name = 'Maya' AND last_name = 'Mclean' AND event_name = 'Women''s Soccer'
```

- **last_error**: validation failed: SELECT * is not a column on a cube. The * was rejected. Name each dimension or fact instead. SELECT * not allowed in semantic SQL

#### Agent rows (1)

| event_name |
| --- |
| Women's Soccer |

- **final**: Yes, Maya Mclean attended the 'Women's Soccer' event.

### Q1401

- **db**: `student_club` · **EX**: **FAIL**
- **reason**: 성공한 execute_sql 없음(도구/재시도 실패)
- **tools**: 4: list_databases, list_tables, search_columns, search_columns
- **row set**: gold=1 · agent=—
- **question**: Indicate the cost of posters for 'September Speaker' event.
- **evidence**: 'Posters' is the expense description; 'September Speaker' is an event name

#### Gold SQL

```sql
SELECT T3.cost FROM event AS T1 INNER JOIN budget AS T2 ON T1.event_id = T2.link_to_event INNER JOIN expense AS T3 ON T2.budget_id = T3.link_to_budget WHERE T1.event_name = 'September Speaker' AND T3.expense_description = 'Posters'
```

#### Gold rows (1)

| c1 |
| --- |
| 67.81 |

#### Agent SQL

—

#### Agent rows (—)

_없음_

- **final**: I cannot find any tables or columns related to "posters" or "cost" in the available database schema. Therefore, I cannot determine the cost of posters for the 'September Speaker' event.

### Q1403

- **db**: `student_club` · **EX**: **FAIL**
- **reason**: 성공한 execute_sql 없음(도구/재시도 실패)
- **tools**: 4: list_databases, list_tables, describe_table, sample_column
- **row set**: gold=1 · agent=—
- **question**: Indicate the name of the closed event whose cost has exceeded the budget the most.
- **evidence**: closed events refers to event_name where status = 'Closed'; exceed the budget the most refers to MIN(remaining) where remaining < 0

#### Gold SQL

```sql
SELECT T2.event_name FROM budget AS T1 INNER JOIN event AS T2 ON T2.event_id = T1.link_to_event WHERE T1.event_status = 'Closed' AND T1.remaining < 0 ORDER BY T1.remaining LIMIT 1
```

#### Gold rows (1)

| c1 |
| --- |
| September Speaker |

#### Agent SQL

—

#### Agent rows (—)

_없음_

- **final**: content='' additional_kwargs={} response_metadata={'model': 'gemma4:e2b-mlx', 'created_at': '2026-10-29T06:19:30.293222Z', 'done': True, 'done_reason': 'stop', 'total_duration': 29161470250, 'load_duration': 52065042, 'prompt_eval_count': 3243, 'prompt_eval_duration': 47639458, 'eval_count': 228, 'eval_duration': 2105380875, 'logprobs': None, 'model_name': 'gemma4:e2b-mlx', 'model_provider': 'olla…

### Q1404

- **db**: `student_club` · **EX**: **FAIL**
- **reason**: execute_sql은 성공했으나 gold_rows와 불일치(로직/필터/값)
- **tools**: 10: list_databases, list_tables, search_columns, describe_table, sample_column, list_tables, describe_table, sample_column, execute_sql, execute_sql
- **row set**: gold=1 · agent=2 · both=0 only_gold=1 only_agent=2
- **question**: Identify the type of expenses and their total value approved for 'October Meeting' event.
- **evidence**: total value refers to SUM(cost); 'October Meeting' is an event name;

#### Gold SQL

```sql
SELECT T1.type, SUM(T3.cost) FROM event AS T1 INNER JOIN budget AS T2 ON T1.event_id = T2.link_to_event INNER JOIN expense AS T3 ON T2.budget_id = T3.link_to_budget WHERE T1.event_name = 'October Meeting'
```

#### Gold rows (1)

| c1 | c2 |
| --- | --- |
| Meeting | 175.39 |

#### Agent SQL

```sql
SELECT budget_category, SUM(budget_amount) FROM cube_join_student_club WHERE event_name = 'October Meeting' GROUP BY budget_category
```

- **last_error**: not found: cube/student_club

#### Agent rows (2)

| budget_category | sum_budget_amount |
| --- | --- |
| Advertisement | 55 |
| Food | 150 |

- **final**: The total approved expenses for the 'October Meeting' event are as follows:
*   Advertisement: 55
*   Food: 150

### Q1405

- **db**: `student_club` · **EX**: **PASS**
- **tools**: 6: list_databases, list_tables, describe_table, sample_column, execute_sql, execute_sql
- **row set**: gold=4 · agent=4 · both=4 only_gold=0 only_agent=0
- **question**: Calculate the amount budgeted for 'April Speaker' event. List all the budgeted categories for said event in an ascending order based on their amount budgeted.
- **evidence**: 'April Speaker' is an event name; amount budgeted refers to SUM(amount); budget categories refers to category

#### Gold SQL

```sql
SELECT T2.category, SUM(T2.amount) FROM event AS T1 JOIN budget AS T2 ON T1.event_id = T2.link_to_event WHERE T1.event_name = 'April Speaker' GROUP BY T2.category ORDER BY SUM(T2.amount) ASC
```

#### Gold rows (4)

| c1 | c2 |
| --- | --- |
| Parking | 10 |
| Speaker Gifts | 25 |
| Advertisement | 75 |
| Food | 150 |

#### Agent SQL

```sql
SELECT budget_category, SUM(budget_amount) FROM cube_join_student_club WHERE event_name = 'April Speaker' GROUP BY budget_category ORDER BY SUM(budget_amount) ASC
```

- **last_error**: not found: cube/student_club

#### Agent rows (4)

| budget_category | sum_budget_amount |
| --- | --- |
| Parking | 10 |
| Speaker Gifts | 25 |
| Advertisement | 75 |
| Food | 150 |

- **final**: The budgeted categories for the 'April Speaker' event, in ascending order based on the amount budgeted, are:

1. **Parking**: 10
2. **Speaker Gifts**: 25
3. **Advertisement**: 75
4. **Food**: 150

### Q1409

- **db**: `student_club` · **EX**: **FAIL**
- **reason**: execute_sql은 성공했으나 gold_rows와 불일치(로직/필터/값)
- **tools**: 17: list_databases, list_tables, list_tables, describe_table, sample_column, execute_sql, sample_column, execute_sql, execute_sql, execute_sql, execute_sql, execute_sql, execute_sql, execute_sql, execute_sql, sample_column, execute_sql
- **row set**: gold=1 · agent=1 · both=0 only_gold=1 only_agent=1
- **question**: Mention the total expense used on 8/20/2019.
- **evidence**: total expense refers SUM(cost) where expense_date = '2019-08-20'

#### Gold SQL

```sql
SELECT SUM(cost) FROM expense WHERE expense_date = '2019-08-20'
```

#### Gold rows (1)

| c1 |
| --- |
| 122.06 |

#### Agent SQL

```sql
SELECT SUM(expense_cost) FROM cube_join_student_club WHERE event_date = '2019-08-20'
```

#### Agent rows (1)

| sum_expense_cost |
| --- |
|  |

- **final**: Based on the available data, there is no recorded total expense for the date 8/20/2019.

### Q1410

- **db**: `student_club` · **EX**: **FAIL**
- **reason**: 성공한 execute_sql 없음(도구/재시도 실패)
- **max_retries**: 동일 SQL 검증 실패 3회 중단
- **tools**: 7: list_databases, list_tables, describe_table, execute_sql, execute_sql, sample_column, execute_sql
- **row set**: gold=1 · agent=—
- **question**: List out the full name and total cost that member id "rec4BLdZHS2Blfp4v" incurred?
- **evidence**: full name refers to first_name, last name

#### Gold SQL

```sql
SELECT T1.first_name, T1.last_name, SUM(T2.cost) FROM member AS T1 INNER JOIN expense AS T2 ON T1.member_id = T2.link_to_member WHERE T1.member_id = 'rec4BLdZHS2Blfp4v'
```

#### Gold rows (1)

| c1 | c2 | c3 |
| --- | --- | --- |
| Sacha | Harrison | 866.25 |

#### Agent SQL

```sql
SELECT T1.first_name, T1.last_name, SUM(T1.expense_cost) FROM student_club.cube_join_student_club AS T1 WHERE T1.zip_code = 'rec4BLdZHS2Blfp4v' GROUP BY T1.first_name, T1.last_name
```

- **last_error**: not found: cube/student_club

#### Agent rows (—)

_없음_

- **final**: Stopped: execute_sql validation/execution failed 3 times with the same SQL. Aborting instead of retrying.

### Q1411

- **db**: `student_club` · **EX**: **FAIL**
- **reason**: SELECT * 금지 — dimension/metric 명시
- **tools**: 7: list_databases, list_tables, list_tables, describe_table, sample_column, execute_sql, execute_sql
- **row set**: gold=12 · agent=—
- **question**: State what kind of expenses that Sacha Harrison incurred?
- **evidence**: kind of expenses refers to expense_description; Sacha Harrison is the full name; full name refers to first_name, last_name;

#### Gold SQL

```sql
SELECT T2.expense_description FROM member AS T1 INNER JOIN expense AS T2 ON T1.member_id = T2.link_to_member WHERE T1.first_name = 'Sacha' AND T1.last_name = 'Harrison'
```

#### Gold rows (12)

| c1 |
| --- |
| Post Cards, Posters |
| Posters |
| Water, chips, cookies |
| Posters |
| Water, chips, cookies |
| Pizza |
| Water, chips, cookies |
| Travel Mug |
| Alumni Glass |
| Posters |
| Travel Mug |
| Bakery - Donuts, muffins |

#### Agent SQL

```sql
SELECT * FROM superhero WHERE full_name = 'Sacha Harrison'
```

- **last_error**: validation failed: SELECT * is not a column on a cube. The * was rejected. Name each dimension or fact instead. SELECT * not allowed in semantic SQL

#### Agent rows (—)

_없음_

- **final**: I do not have sufficient information in the available database to determine what kind of expenses Sacha Harrison incurred. The available table, `superhero`, contains information about superheroes, not financial records.

### Q1422

- **db**: `student_club` · **EX**: **FAIL**
- **reason**: 결과 행 수가 gold 대비 과도(조인 팬아웃 의심)
- **tools**: 5: list_databases, list_tables, describe_table, sample_column, execute_sql
- **row set**: gold=4 · agent=19 · both=0 only_gold=4 only_agent=3
- **question**: State the category of events were held at MU 215.
- **evidence**: 'MU 215' is the location of event; 

#### Gold SQL

```sql
SELECT DISTINCT T2.category FROM event AS T1 INNER JOIN budget AS T2 ON T1.event_id = T2.link_to_event WHERE T1.location = 'MU 215'
```

#### Gold rows (4)

| c1 |
| --- |
| Advertisement |
| Food |
| Speaker Gifts |
| Parking |

#### Agent SQL

```sql
SELECT event_type FROM cube_join_student_club WHERE event_location = 'MU 215'
```

#### Agent rows (19)

| event_type |
| --- |
| Meeting |
| Election |
| Guest Speaker |
| Guest Speaker |
| Guest Speaker |
| Guest Speaker |
| Guest Speaker |
| Election |
| Meeting |
| Meeting |
| Meeting |
| Meeting |
| Guest Speaker |
| Meeting |
| Guest Speaker |
| Meeting |
| Meeting |
| Meeting |
| Meeting |

- **final**: The events held at MU 215 include: Meeting, Election, and Guest Speaker.

### Q1426

- **db**: `student_club` · **EX**: **PASS**
- **tools**: 5: list_databases, list_tables, describe_table, sample_column, execute_sql
- **row set**: gold=2 · agent=2 · both=2 only_gold=0 only_agent=0
- **question**: List the last name of members with a major in environmental engineering and include its department and college name.
- **evidence**: 'Environmental Engineering' is the major_name;

#### Gold SQL

```sql
SELECT T2.last_name, T1.department, T1.college FROM major AS T1 INNER JOIN member AS T2 ON T1.major_id = T2.link_to_major WHERE T2.position = 'Member' AND T1.major_name = 'Environmental Engineering'
```

#### Gold rows (2)

| c1 | c2 | c3 |
| --- | --- | --- |
| Hewitt | Civil and Environmental Engineering Department | College of Engineering |
| Mclean | Civil and Environmental Engineering Department | College of Engineering |

#### Agent SQL

```sql
SELECT last_name, department, college FROM cube_join_student_club WHERE major_name = 'Environmental Engineering'
```

#### Agent rows (2)

| last_name | department | college |
| --- | --- | --- |
| Hewitt | Civil and Environmental Engineering Department | College of Engineering |
| Mclean | Civil and Environmental Engineering Department | College of Engineering |

- **final**: Hewitt, Civil and Environmental Engineering Department, College of Engineering
Mclean, Civil and Environmental Engineering Department, College of Engineering

### Q1427

- **db**: `student_club` · **EX**: **FAIL**
- **reason**: 프로젝션 열 개수/구성이 gold와 불일치
- **tools**: 7: list_databases, list_tables, describe_table, sample_column, sample_column, execute_sql, execute_sql
- **row set**: gold=4 · agent=16 · both=0 only_gold=4 only_agent=4
- **question**: What are the budget category of the events located at MU 215 and a guest speaker type with a 0 budget spent?
- **evidence**: budget category refers to category; events located at refers to location; type = 'Guest Speaker'; 0 budget spent refers to spent = 0; 

#### Gold SQL

```sql
SELECT DISTINCT T2.category, T1.type FROM event AS T1 INNER JOIN budget AS T2 ON T1.event_id = T2.link_to_event WHERE T1.location = 'MU 215' AND T2.spent = 0 AND T1.type = 'Guest Speaker'
```

#### Gold rows (4)

| c1 | c2 |
| --- | --- |
| Speaker Gifts | Guest Speaker |
| Food | Guest Speaker |
| Parking | Guest Speaker |
| Advertisement | Guest Speaker |

#### Agent SQL

```sql
SELECT budget_category FROM cube_join_student_club WHERE event_location = 'MU 215' AND event_type = 'Guest Speaker' AND budget_spent = 0
```

- **last_error**: not found: cube/student_club

#### Agent rows (16)

| budget_category |
| --- |
| Speaker Gifts |
| Food |
| Parking |
| Advertisement |
| Speaker Gifts |
| Speaker Gifts |
| Food |
| Food |
| Advertisement |
| Speaker Gifts |
| Food |
| Advertisement |
| Parking |
| Parking |
| Advertisement |
| Parking |

- **final**: The budget categories for events located at MU 215 and a guest speaker type with a $0 budget spent are: Speaker Gifts, Food, Parking, and Advertisement.

### Q1432

- **db**: `student_club` · **EX**: **FAIL**
- **reason**: execute_sql은 성공했으나 gold_rows와 불일치(로직/필터/값)
- **tools**: 5: list_databases, list_tables, describe_table, sample_column, execute_sql
- **row set**: gold=1 · agent=1 · both=0 only_gold=1 only_agent=1
- **question**: Among the members with t-shirt size of medium, what is the percentage of the amount 50 received by the Student_Club?
- **evidence**: t_shirt_size = 'Medium' where position = 'Member'; percentage = DIVIDE(COUNT(amount = 50), COUNT(member_id)) * 100

#### Gold SQL

```sql
SELECT CAST(SUM(CASE WHEN T2.amount = 50 THEN 1.0 ELSE 0 END) AS REAL) * 100 / COUNT(T2.income_id) FROM member AS T1 INNER JOIN income AS T2 ON T1.member_id = T2.link_to_member WHERE T1.position = 'Member' AND T1.t_shirt_size = 'Medium'
```

#### Gold rows (1)

| c1 |
| --- |
| 100.0 |

#### Agent SQL

```sql
SELECT CAST(SUM(CASE WHEN t_shirt_size = 'Medium' THEN budget_amount ELSE 0 END) * 100.0 / SUM(budget_amount) AS REAL) FROM cube_join_student_club
```

#### Agent rows (1)

| cast_sum_case_when_t_shirt_size_medium_then_budget_amount_else_0_end_100_0_sum_budget_amount_as_real |
| --- |
| 0.0 |

- **final**: The percentage of the amount $50 received by the Student\_Club among members with a t-shirt size of Medium is 0.0.

### Q1435

- **db**: `student_club` · **EX**: **FAIL**
- **reason**: 시맨틱 식별자 미등록(unknown metric/dimension/column)
- **tools**: 6: list_databases, list_tables, describe_table, search_columns, sample_column, execute_sql
- **row set**: gold=2 · agent=—
- **question**: List the names of closed event as "game" that was closed from 3/15/2019 to 3/20/2020.
- **evidence**: name of events refers event_name; game event that was closed refers to type = 'Game' where status = 'Closed'; event_date BETWEEN '2019-03-15' and '2020-03-20'; 

#### Gold SQL

```sql
SELECT DISTINCT event_name FROM event WHERE type = 'Game' AND date(SUBSTR(event_date, 1, 10)) BETWEEN '2019-03-15' AND '2020-03-20' AND status = 'Closed'
```

#### Gold rows (2)

| c1 |
| --- |
| Women's Soccer |
| Football game |

#### Agent SQL

```sql
SELECT superhero_name FROM superhero WHERE event = 'game' AND status = 'closed' AND event_date BETWEEN '2019-03-15' AND '2020-03-20'
```

- **last_error**: validation failed: That name is not a dimension, fact, or SELECT alias on this cube. Copy the member name from describe_table. unknown identifier in expression: event

#### Agent rows (—)

_없음_

- **final**: SELECT superhero_name FROM superhero WHERE superhero_name = 'game'

### Q1457

- **db**: `student_club` · **EX**: **FAIL**
- **reason**: 성공한 execute_sql 없음(도구/재시도 실패)
- **max_retries**: 동일 SQL 검증 실패 3회 중단
- **tools**: 6: list_databases, list_tables, describe_table, execute_sql, execute_sql, execute_sql
- **row set**: gold=3 · agent=—
- **question**: Give the full name and contact number of members who had to spend more than average on each expense.
- **evidence**: full name refers to first_name, last_name; contact number refers to phone; had spent more than average on each expense refers to cost > AVG(cost)

#### Gold SQL

```sql
SELECT DISTINCT T3.first_name, T3.last_name, T3.phone FROM expense AS T1 INNER JOIN budget AS T2 ON T1.link_to_budget = T2.budget_id INNER JOIN member AS T3 ON T3.member_id = T1.link_to_member WHERE T1.cost > ( SELECT AVG(T1.cost) FROM expense AS T1 INNER JOIN budget AS T2 ON T1.link_to_budget = T2.budget_id INNER JOIN member AS T3 ON T3.member_id = T1.link_to_member )
```

#### Gold rows (3)

| c1 | c2 | c3 |
| --- | --- | --- |
| Sacha | Harrison | 840-555-4781 |
| Phillip | Cullen | 905-555-5035 |
| Elijah | Allen | 724-555-5721 |

#### Agent SQL

```sql
SELECT T1.first_name, T1.last_name, T1.phone FROM student_club.cube_join_student_club AS T1 INNER JOIN (SELECT budget_category, AVG(expense_cost) AS avg_cost FROM student_club.cube_join_student_club GROUP BY budget_category) AS T2 ON T1.budget_category = T2.budget_category WHERE T1.expense_cost > T2.avg_cost
```

- **last_error**: not found: cube/student_club

#### Agent rows (—)

_없음_

- **final**: Stopped: execute_sql validation/execution failed 3 times with the same SQL. Aborting instead of retrying.

### Q1460

- **db**: `student_club` · **EX**: **FAIL**
- **reason**: 결과 행 수가 gold 대비 과도(조인 팬아웃 의심)
- **tools**: 5: list_databases, list_tables, describe_table, sample_column, execute_sql
- **row set**: gold=1 · agent=10 · both=1 only_gold=0 only_agent=8
- **question**: Write the full name of the member who spent money for water, veggie tray and supplies and include the cost of it.
- **evidence**: full name refers to first_name, last name; spent money for refers expense description; expense_description = 'Water, Veggie tray, supplies'

#### Gold SQL

```sql
SELECT T2.first_name, T2.last_name, T1.cost FROM expense AS T1 INNER JOIN member AS T2 ON T1.link_to_member = T2.member_id WHERE T1.expense_description = 'Water, Veggie tray, supplies'
```

#### Gold rows (1)

| c1 | c2 | c3 |
| --- | --- | --- |
| Elijah | Allen | 28.15 |

#### Agent SQL

```sql
SELECT first_name, last_name, expense_cost FROM cube_join_student_club WHERE expense_description LIKE '%water%'
```

#### Agent rows (10)

| first_name | last_name | expense_cost |
| --- | --- | --- |
| Elijah | Allen | 20.2 |
| Elijah | Allen | 20.2 |
| Sacha | Harrison | 50.13 |
| Sacha | Harrison | 59.73 |
| Phillip | Cullen | 69.33 |
| Sacha | Harrison | 61.52 |
| Phillip | Cullen | 50.13 |
| Elijah | Allen | 28.15 |
| Phillip | Cullen | 20.2 |
| Phillip | Cullen | 59.73 |

- **final**: Elijah Allen, cost $20.2
Elijah Allen, cost $20.2
Sacha Harrison, cost $50.13
Sacha Harrison, cost $59.73
Phillip Cullen, cost $69.33
Sacha Harrison, cost $61.52
Phillip Cullen, cost $50.13
Elijah Allen, cost $28.15
Phillip Cullen, cost $20.2
Phillip Cullen, cost $59.73

### Q1464

- **db**: `student_club` · **EX**: **FAIL**
- **reason**: 프로젝션 열 개수/구성이 gold와 불일치
- **tools**: 5: list_databases, list_tables, describe_table, execute_sql, execute_sql
- **row set**: gold=2 · agent=2 · both=0 only_gold=2 only_agent=2
- **question**: Write the full names of students who received funds on the date of 9/9/2019 and include the amount received.
- **evidence**: full name refers to first_name, last_name, amount of funds received refers to amount, received funds on date refers to date_received

#### Gold SQL

```sql
SELECT DISTINCT T3.first_name, T3.last_name, T4.amount FROM event AS T1 INNER JOIN attendance AS T2 ON T1.event_id = T2.link_to_event INNER JOIN member AS T3 ON T3.member_id = T2.link_to_member INNER JOIN income AS T4 ON T4.link_to_member = T3.member_id WHERE T4.date_received = '2019-09-09'
```

#### Gold rows (2)

| c1 | c2 | c3 |
| --- | --- | --- |
| Sacha | Harrison | 50 |
| Carlo | Jacobs | 50 |

#### Agent SQL

```sql
SELECT first_name || ' ' || last_name, income_amount FROM cube_join_student_club WHERE income_date_received = '2019-09-09'
```

- **last_error**: not found: cube/student_club

#### Agent rows (2)

| first_name_last_name | income_amount |
| --- | --- |
| Sacha Harrison | 50 |
| Carlo Jacobs | 50 |

- **final**: Sacha Harrison, 50
Carlo Jacobs, 50

