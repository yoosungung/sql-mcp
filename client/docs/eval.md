# BIRD Mini-Dev Eval (문항별)

요약은 [eval_report.md](eval_report.md). 문항마다 **Gold SQL / Gold rows**와 **Agent SQL / Agent rows**를 붙여 비교한다.

- **일시**: `2026-10-28T22:19:12.925229+00:00` · scored `2026-10-28T22:19:17.917486+00:00`
- **모델**: `ollama:gemma4:e2b-mlx` · MCP `http://127.0.0.1:8090/mcp`
- **EX**: 21/52 (40.38%)

## Index

| qid | db_id | EX | reason |
|-----|-------|----|--------|
| [717](#q717) | superhero | PASS |  |
| [719](#q719) | superhero | PASS |  |
| [723](#q723) | superhero | FAIL | execute_sql은 성공했으나 gold_rows와 불일치(로직/필터/값) |
| [724](#q724) | superhero | FAIL | 필터/조인으로 결과 공집합(gold는 비공집합) |
| [726](#q726) | superhero | FAIL | 프로젝션 열 개수/구성이 gold와 불일치 |
| [728](#q728) | superhero | FAIL | 프로젝션 열 개수/구성이 gold와 불일치 |
| [730](#q730) | superhero | FAIL | execute_sql은 성공했으나 gold_rows와 불일치(로직/필터/값) |
| [732](#q732) | superhero | FAIL | execute_sql은 성공했으나 gold_rows와 불일치(로직/필터/값) |
| [733](#q733) | superhero | FAIL | execute_sql은 성공했으나 gold_rows와 불일치(로직/필터/값) |
| [736](#q736) | superhero | FAIL | execute_sql은 성공했으나 gold_rows와 불일치(로직/필터/값) |
| [737](#q737) | superhero | PASS |  |
| [738](#q738) | superhero | FAIL | execute_sql은 성공했으나 gold_rows와 불일치(로직/필터/값) |
| [739](#q739) | superhero | PASS |  |
| [740](#q740) | superhero | FAIL | execute_sql은 성공했으나 gold_rows와 불일치(로직/필터/값) |
| [743](#q743) | superhero | FAIL | execute_sql은 성공했으나 gold_rows와 불일치(로직/필터/값) |
| [744](#q744) | superhero | FAIL | execute_sql은 성공했으나 gold_rows와 불일치(로직/필터/값) |
| [745](#q745) | superhero | FAIL | 결과 행 수가 gold 대비 과도(조인 팬아웃 의심) |
| [747](#q747) | superhero | FAIL | execute_sql은 성공했으나 gold_rows와 불일치(로직/필터/값) |
| [750](#q750) | superhero | PASS |  |
| [751](#q751) | superhero | FAIL | 결과 행 수가 gold 대비 과도(조인 팬아웃 의심) |
| [753](#q753) | superhero | FAIL | execute_sql은 성공했으나 gold_rows와 불일치(로직/필터/값) |
| [758](#q758) | superhero | FAIL | 결과 행 수가 gold 대비 과도(조인 팬아웃 의심) |
| [760](#q760) | superhero | PASS |  |
| [761](#q761) | superhero | PASS |  |
| [764](#q764) | superhero | PASS |  |
| [765](#q765) | superhero | FAIL | execute_sql은 성공했으나 gold_rows와 불일치(로직/필터/값) |
| [766](#q766) | superhero | PASS |  |
| [769](#q769) | superhero | PASS |  |
| [772](#q772) | superhero | FAIL | execute_sql은 성공했으나 gold_rows와 불일치(로직/필터/값) |
| [773](#q773) | superhero | FAIL | execute_sql은 성공했으나 gold_rows와 불일치(로직/필터/값) |
| [775](#q775) | superhero | FAIL | execute_sql은 성공했으나 gold_rows와 불일치(로직/필터/값) |
| [779](#q779) | superhero | PASS |  |
| [781](#q781) | superhero | PASS |  |
| [782](#q782) | superhero | FAIL | execute_sql은 성공했으나 gold_rows와 불일치(로직/필터/값) |
| [785](#q785) | superhero | PASS |  |
| [786](#q786) | superhero | PASS |  |
| [788](#q788) | superhero | FAIL | execute_sql은 성공했으나 gold_rows와 불일치(로직/필터/값) |
| [790](#q790) | superhero | FAIL | execute_sql은 성공했으나 gold_rows와 불일치(로직/필터/값) |
| [791](#q791) | superhero | FAIL | execute_sql은 성공했으나 gold_rows와 불일치(로직/필터/값) |
| [792](#q792) | superhero | PASS |  |
| [794](#q794) | superhero | FAIL | execute_sql은 성공했으나 gold_rows와 불일치(로직/필터/값) |
| [796](#q796) | superhero | PASS |  |
| [797](#q797) | superhero | FAIL | 필터/조인으로 결과 공집합(gold는 비공집합) |
| [798](#q798) | superhero | PASS |  |
| [800](#q800) | superhero | PASS |  |
| [801](#q801) | superhero | FAIL | execute_sql은 성공했으나 gold_rows와 불일치(로직/필터/값) |
| [806](#q806) | superhero | PASS |  |
| [819](#q819) | superhero | FAIL | execute_sql은 성공했으나 gold_rows와 불일치(로직/필터/값) |
| [822](#q822) | superhero | FAIL | execute_sql은 성공했으나 gold_rows와 불일치(로직/필터/값) |
| [824](#q824) | superhero | PASS |  |
| [825](#q825) | superhero | PASS |  |
| [829](#q829) | superhero | FAIL | execute_sql은 성공했으나 gold_rows와 불일치(로직/필터/값) |

## 문항

## superhero

### Q717

- **db**: `superhero` · **EX**: **PASS**
- **tools**: 4: list_databases, list_tables, describe_table, execute_sql
- **row set**: gold=4 · agent=4 · both=4 only_gold=0 only_agent=0
- **question**: Please list all the superpowers of 3-D Man.
- **evidence**: 3-D Man refers to superhero_name = '3-D Man'; superpowers refers to power_name

#### Gold SQL

```sql
SELECT T3.power_name FROM superhero AS T1 INNER JOIN hero_power AS T2 ON T1.id = T2.hero_id INNER JOIN superpower AS T3 ON T2.power_id = T3.id WHERE T1.superhero_name = '3-D Man'
```

#### Gold rows (4)

| c1 |
| --- |
| Agility |
| Super Strength |
| Stamina |
| Super Speed |

#### Agent SQL

```sql
SELECT power_name FROM superhero WHERE superhero_name = '3-D Man'
```

#### Agent rows (4)

| power_name |
| --- |
| Agility |
| Super Strength |
| Stamina |
| Super Speed |

- **final**: The superpowers of 3-D Man are: Agility, Super Strength, Stamina, and Super Speed.

### Q719

- **db**: `superhero` · **EX**: **PASS**
- **tools**: 6: list_databases, list_tables, describe_table, sample_column, sample_column, execute_sql
- **row set**: gold=1 · agent=1 · both=1 only_gold=0 only_agent=0
- **question**: Among the superheroes with the super power of "Super Strength", how many of them have a height of over 200cm?
- **evidence**: super power of "Super Strength" refers to power_name = 'Super Strength'; a height of over 200cm refers to height_cm > 200

#### Gold SQL

```sql
SELECT COUNT(T1.id) FROM superhero AS T1 INNER JOIN hero_power AS T2 ON T1.id = T2.hero_id INNER JOIN superpower AS T3 ON T2.power_id = T3.id WHERE T3.power_name = 'Super Strength' AND T1.height_cm > 200
```

#### Gold rows (1)

| c1 |
| --- |
| 56 |

#### Agent SQL

```sql
SELECT count(*) FROM superhero WHERE power_name = 'Super Strength' AND height_cm > 200
```

#### Agent rows (1)

| count |
| --- |
| 56 |

- **final**: 56

### Q723

- **db**: `superhero` · **EX**: **FAIL**
- **reason**: execute_sql은 성공했으나 gold_rows와 불일치(로직/필터/값)
- **tools**: 6: list_databases, list_tables, describe_table, sample_column, sample_column, execute_sql
- **row set**: gold=1 · agent=1 · both=0 only_gold=1 only_agent=1
- **question**: Among the superheroes with blue eyes, how many of them have the super power of "Agility"?
- **evidence**: blue eyes refers to colour = 'Blue' and eye_colour_id = colour.id; super power of "Agility" refers to power_name = 'Agility'

#### Gold SQL

```sql
SELECT COUNT(T1.id) FROM superhero AS T1 INNER JOIN hero_power AS T2 ON T1.id = T2.hero_id INNER JOIN superpower AS T3 ON T2.power_id = T3.id INNER JOIN colour AS T4 ON T1.eye_colour_id = T4.id WHERE T3.power_name = 'Agility' AND T4.colour = 'Blue'
```

#### Gold rows (1)

| c1 |
| --- |
| 83 |

#### Agent SQL

```sql
SELECT count(superhero_name) FROM superhero WHERE eye_color = 'blue' AND power_name = 'Agility'
```

#### Agent rows (1)

| count_superhero_name |
| --- |
| 0 |

- **final**: 0

### Q724

- **db**: `superhero` · **EX**: **FAIL**
- **reason**: 필터/조인으로 결과 공집합(gold는 비공집합)
- **tools**: 7: list_databases, list_tables, describe_table, sample_column, sample_column, execute_sql, execute_sql
- **row set**: gold=90 · agent=0 · both=0 only_gold=90 only_agent=0
- **question**: Please list the superhero names of all the superheroes that have blue eyes and blond hair.
- **evidence**: blue eyes refers to colour = 'Blue' and eye_colour_id = colour.id; blond hair refers to colour = 'Blond' and hair_colour_id = colour.id; super power of "Agility" refers to power_name = 'Agility'

#### Gold SQL

```sql
SELECT T1.superhero_name FROM superhero AS T1 INNER JOIN colour AS T2 ON T1.eye_colour_id = T2.id INNER JOIN colour AS T3 ON T1.hair_colour_id = T3.id WHERE T2.colour = 'Blue' AND T3.colour = 'Blond'
```

#### Gold rows (90)

| c1 |
| --- |
| Adam Monroe |
| Adam Strange |
| Agent 13 |
| Alan Scott |
| Angel |
| Animal Man |
| Ant-Man |
| Ant-Man II |
| Anti-Venom |
| Aquababy |
| Aquaman |
| Arachne |
| Archangel |
| Batgirl VI |
| Binary |
| Black Canary |
| Black Canary II |
| Black Widow II |
| Boom-Boom |
| Booster Gold |
| Box III |
| Bullseye |
| Cannonball |
| Captain America |
| Captain Britain |
| Captain Mar-vell |
| Captain Marvel |
| Cat |
| Claire Bennet |
| Cypher |
| Dagger |
| Dash |
| Dazzler |
| Doctor Fate |
| Elle Bishop |
| Emma Frost |
| Enchantress |
| Flash II |
| Franklin Richards |
| Genesis |

… +50 rows

#### Agent SQL

```sql
SELECT superhero_name FROM superhero WHERE eye_color = 'blue' AND hair_colour = 'blond'
```

#### Agent rows (0)

_empty_

- **final**: No superheroes were found with blue eyes and blond hair.

### Q726

- **db**: `superhero` · **EX**: **FAIL**
- **reason**: 프로젝션 열 개수/구성이 gold와 불일치
- **tools**: 5: list_databases, list_tables, describe_table, sample_column, execute_sql
- **row set**: gold=387 · agent=387 · both=0 only_gold=386 only_agent=386
- **question**: Rank heroes published by Marvel Comics by their height in descending order.
- **evidence**: name refers to superhero_name; the tallest hero refers to MAX(height_cm); published by Marvel Comics refers to publisher_name = 'Marvel Comics'

#### Gold SQL

```sql
SELECT superhero_name, height_cm, RANK() OVER (ORDER BY height_cm DESC) AS HeightRank FROM superhero INNER JOIN publisher ON superhero.publisher_id = publisher.id WHERE publisher.publisher_name = 'Marvel Comics'
```

#### Gold rows (387)

| c1 | c2 | c3 |
| --- | --- | --- |
| Surtur | 30480 | 1 |
| Ymir | 30480 | 1 |
| Bloodwraith | 3050 | 3 |
| Utgard-Loki | 1520 | 4 |
| Fin Fang Foom | 975 | 5 |
| Galactus | 876 | 6 |
| Groot | 701 | 7 |
| MODOK | 366 | 8 |
| Wolfsbane | 366 | 8 |
| Onslaught | 305 | 10 |
| Sasquatch | 305 | 10 |
| Juggernaut | 287 | 12 |
| Hulk | 244 | 13 |
| Anti-Venom | 229 | 14 |
| Bi-Beast | 229 | 14 |
| Venom III | 229 | 14 |
| Cloak | 226 | 17 |
| Colossus | 226 | 17 |
| Venompool | 226 | 17 |
| Bloodaxe | 218 | 20 |
| Thundra | 218 | 20 |
| Warpath | 218 | 20 |
| Apocalypse | 213 | 23 |
| Hela | 213 | 23 |
| Man-Thing | 213 | 23 |
| Red Hulk | 213 | 23 |
| Ant-Man | 211 | 27 |
| Frenzy | 211 | 27 |
| Omega Red | 211 | 27 |
| Scorpion | 211 | 27 |
| Odin | 206 | 31 |
| Ultron | 206 | 31 |
| A-Bomb | 203 | 33 |
| Abomination | 203 | 33 |
| Cable | 203 | 33 |
| Lizard | 203 | 33 |
| Beta Ray Bill | 201 | 37 |
| Century | 201 | 37 |
| Doctor Doom | 201 | 37 |
| Doctor Doom II | 201 | 37 |

… +347 rows

#### Agent SQL

```sql
SELECT superhero_name, height_cm FROM superhero WHERE publisher_name = 'Marvel Comics' ORDER BY height_cm DESC
```

#### Agent rows (387)

| superhero_name | height_cm |
| --- | --- |
| Surtur | 30480 |
| Ymir | 30480 |
| Bloodwraith | 3050 |
| Utgard-Loki | 1520 |
| Fin Fang Foom | 975 |
| Galactus | 876 |
| Groot | 701 |
| MODOK | 366 |
| Wolfsbane | 366 |
| Onslaught | 305 |
| Sasquatch | 305 |
| Juggernaut | 287 |
| Hulk | 244 |
| Anti-Venom | 229 |
| Bi-Beast | 229 |
| Venom III | 229 |
| Cloak | 226 |
| Colossus | 226 |
| Venompool | 226 |
| Bloodaxe | 218 |
| Thundra | 218 |
| Warpath | 218 |
| Apocalypse | 213 |
| Hela | 213 |
| Man-Thing | 213 |
| Red Hulk | 213 |
| Ant-Man | 211 |
| Frenzy | 211 |
| Omega Red | 211 |
| Scorpion | 211 |
| Odin | 206 |
| Ultron | 206 |
| A-Bomb | 203 |
| Abomination | 203 |
| Cable | 203 |
| Lizard | 203 |
| Beta Ray Bill | 201 |
| Century | 201 |
| Doctor Doom | 201 |
| Doctor Doom II | 201 |

… +347 rows

- **final**: content='' additional_kwargs={} response_metadata={'model': 'gemma4:e2b-mlx', 'created_at': '2026-10-28T22:02:55.915623Z', 'done': True, 'done_reason': 'stop', 'total_duration': 25554608417, 'load_duration': 51941334, 'prompt_eval_count': 6024, 'prompt_eval_duration': 1371960042, 'eval_count': 3076, 'eval_duration': 20236407375, 'logprobs': None, 'model_name': 'gemma4:e2b-mlx', 'model_provider': '…

### Q728

- **db**: `superhero` · **EX**: **FAIL**
- **reason**: 프로젝션 열 개수/구성이 gold와 불일치
- **tools**: 5: list_databases, list_tables, describe_table, sample_column, execute_sql
- **row set**: gold=19 · agent=21 · both=0 only_gold=19 only_agent=21
- **question**: Rank superheroes from Marvel Comics by their eye color popularity, starting with the most common color.
- **evidence**: the superheroes from Marvel Comics refers to publisher_name = 'Marvel Comics'; most common color refers to COUNT(superhero.id) DESC;

#### Gold SQL

```sql
SELECT colour.colour AS EyeColor, COUNT(superhero.id) AS Count, RANK() OVER (ORDER BY COUNT(superhero.id) DESC) AS PopularityRank FROM superhero INNER JOIN colour ON superhero.eye_colour_id = colour.id INNER JOIN publisher ON superhero.publisher_id = publisher.id WHERE publisher.publisher_name = 'Marvel Comics' GROUP BY colour.colour
```

#### Gold rows (19)

| c1 | c2 | c3 |
| --- | --- | --- |
| Blue | 126 | 1 |
| Brown | 89 | 2 |
| No Colour | 47 | 3 |
| Green | 40 | 4 |
| Red | 31 | 5 |
| Yellow | 13 | 6 |
| White | 12 | 7 |
| Black | 11 | 8 |
| Grey | 4 | 9 |
| Hazel | 3 | 10 |
| Purple | 2 | 11 |
| Gold | 2 | 11 |
| Yellow/Blue | 1 | 13 |
| White/Red | 1 | 13 |
| Violet | 1 | 13 |
| Silver | 1 | 13 |
| Green/Blue | 1 | 13 |
| Blue/White | 1 | 13 |
| Amber | 1 | 13 |

#### Agent SQL

```sql
SELECT eye_color, count(hero_id) FROM superhero GROUP BY eye_color ORDER BY count(hero_id) DESC
```

#### Agent rows (21)

| eye_color | count_hero_id |
| --- | --- |
| Blue | 234 |
| No Colour | 169 |
| Brown | 131 |
| Green | 76 |
| Red | 47 |
| Black | 24 |
| Yellow | 22 |
| White | 16 |
| Hazel | 6 |
| Grey | 6 |
| Purple | 4 |
| Gold | 3 |
| Amber | 3 |
| Violet | 2 |
| Yellow/Red | 1 |
| Yellow/Blue | 1 |
| White/Red | 1 |
| Silver | 1 |
| Indigo | 1 |
| Green/Blue | 1 |
| Blue/White | 1 |

- **final**: Based on the data retrieved, here is the ranking of eye colors by popularity (count):

1. **Blue**: 234
2. **No Colour**: 169
3. **Brown**: 131
4. **Green**: 76
5. **Red**: 47
6. **Black**: 24
7. **Yellow**: 22
8. **White**: 16
9. **Hazel**: 6
10. **Grey**: 6
11. **Purple**: 4
12. **Gold**: 3
13. **Amber**: 3
14. **Violet**: 2
15. **Yellow/Red**: 1
16. **Yellow/Blue**: 1
17. **White/Red**: 1
18. *…

### Q730

- **db**: `superhero` · **EX**: **FAIL**
- **reason**: execute_sql은 성공했으나 gold_rows와 불일치(로직/필터/값)
- **tools**: 5: list_databases, list_tables, describe_table, sample_column, execute_sql
- **row set**: gold=201 · agent=358 · both=200 only_gold=0 only_agent=152
- **question**: List the superheroes from Marvel Comics who have the super power of 'Super Strength'.
- **evidence**: the superheroes from Marvel Comics refers to publisher_name = 'Marvel Comics'; super power of "Super Strength" refers to power_name = 'Super Strength';

#### Gold SQL

```sql
SELECT superhero_name FROM superhero AS T1 WHERE EXISTS (SELECT 1 FROM hero_power AS T2 INNER JOIN superpower AS T3 ON T2.power_id = T3.id WHERE T3.power_name = 'Super Strength' AND T1.id = T2.hero_id)AND EXISTS (SELECT 1 FROM publisher AS T4 WHERE T4.publisher_name = 'Marvel Comics' AND T1.publisher_id = T4.id)
```

#### Gold rows (201)

| c1 |
| --- |
| 3-D Man |
| A-Bomb |
| Abomination |
| Abraxas |
| Absorbing Man |
| Agent Zero |
| Air-Walker |
| Ajax |
| Angel |
| Angel Dust |
| Annihilus |
| Ant-Man II |
| Anti-Venom |
| Apocalypse |
| Arachne |
| Archangel |
| Arclight |
| Ardina |
| Ares |
| Armor |
| Atlas |
| Atlas |
| Bantam |
| Battlestar |
| Beast |
| Beta Ray Bill |
| Beyonder |
| Binary |
| Bird-Man |
| Bishop |
| Black Bolt |
| Black Cat |
| Blackout |
| Blackwulf |
| Blade |
| Blob |
| Bloodaxe |
| Bloodwraith |
| Cable |
| Callisto |

… +161 rows

#### Agent SQL

```sql
SELECT superhero_name FROM superhero WHERE power_name = 'Super Strength'
```

#### Agent rows (358)

| superhero_name |
| --- |
| 3-D Man |
| A-Bomb |
| Abe Sapien |
| Abomination |
| Abraxas |
| Absorbing Man |
| Agent Zero |
| Air-Walker |
| Ajax |
| Alex Mercer |
| Alien |
| Amazo |
| Angel |
| Angel |
| Angel Dust |
| Annihilus |
| Ant-Man II |
| Anti-Monitor |
| Anti-Spawn |
| Anti-Venom |
| Apocalypse |
| Aquababy |
| Aqualad |
| Aquaman |
| Arachne |
| Archangel |
| Arclight |
| Ardina |
| Ares |
| Ares |
| Armor |
| Astro Boy |
| Atlas |
| Atlas |
| Atlas |
| Atlas |
| Atom |
| Atom II |
| Azrael |
| Aztar |

… +318 rows

- **final**: 3-D Man

### Q732

- **db**: `superhero` · **EX**: **FAIL**
- **reason**: execute_sql은 성공했으나 gold_rows와 불일치(로직/필터/값)
- **tools**: 7: list_databases, list_tables, describe_table, sample_column, sample_column, sample_column, execute_sql
- **row set**: gold=1 · agent=1 · both=0 only_gold=1 only_agent=1
- **question**: Which publisher published the slowest superhero?
- **evidence**: the slowest superhero refers to attribute_name = 'Speed' where MIN(attribute_value); publisher refers to publisher_name

#### Gold SQL

```sql
SELECT T2.publisher_name FROM superhero AS T1 INNER JOIN publisher AS T2 ON T1.publisher_id = T2.id INNER JOIN hero_attribute AS T3 ON T1.id = T3.hero_id INNER JOIN attribute AS T4 ON T3.attribute_id = T4.id WHERE T4.attribute_name = 'Speed' ORDER BY T3.attribute_value LIMIT 1
```

#### Gold rows (1)

| c1 |
| --- |
| Marvel Comics |

#### Agent SQL

```sql
SELECT publisher_name FROM superhero ORDER BY weight_kg ASC LIMIT 1
```

#### Agent rows (1)

| publisher_name |
| --- |
| Wildstorm |

- **final**: The publisher that published the slowest superhero is Wildstorm.

### Q733

- **db**: `superhero` · **EX**: **FAIL**
- **reason**: execute_sql은 성공했으나 gold_rows와 불일치(로직/필터/값)
- **tools**: 5: list_databases, list_tables, describe_table, sample_column, execute_sql
- **row set**: gold=1 · agent=1 · both=0 only_gold=1 only_agent=1
- **question**: How many gold-eyed superheroes did Marvel Comics publish?
- **evidence**: gold-eyed refers to colour = 'Gold' where eye_colour_id = colour.id; superheroes that Marvel Comics published refers to publisher_name = 'Marvel Comics'

#### Gold SQL

```sql
SELECT COUNT(T1.id) FROM superhero AS T1 INNER JOIN publisher AS T2 ON T1.publisher_id = T2.id INNER JOIN colour AS T3 ON T1.eye_colour_id = T3.id WHERE T2.publisher_name = 'Marvel Comics' AND T3.colour = 'Gold'
```

#### Gold rows (1)

| c1 |
| --- |
| 2 |

#### Agent SQL

```sql
SELECT count(*) FROM superhero WHERE eye_color LIKE '%gold%'
```

#### Agent rows (1)

| count |
| --- |
| 1 |

- **final**: Marvel Comics published 1 gold-eyed superheroes.

### Q736

- **db**: `superhero` · **EX**: **FAIL**
- **reason**: execute_sql은 성공했으나 gold_rows와 불일치(로직/필터/값)
- **tools**: 7: list_databases, list_tables, describe_table, sample_column, execute_sql, sample_column, execute_sql
- **row set**: gold=1 · agent=1 · both=0 only_gold=1 only_agent=1
- **question**: Who is the dumbest superhero?
- **evidence**: the dumbest superhero refers to MIN(attribute_value) where attribute_name = 'Intelligence'

#### Gold SQL

```sql
SELECT T1.superhero_name FROM superhero AS T1 INNER JOIN hero_attribute AS T2 ON T1.id = T2.hero_id INNER JOIN attribute AS T3 ON T2.attribute_id = T3.id WHERE T3.attribute_name = 'Intelligence' ORDER BY T2.attribute_value LIMIT 1
```

#### Gold rows (1)

| c1 |
| --- |
| Ammo |

#### Agent SQL

```sql
SELECT superhero_name FROM superhero ORDER BY self_interest_pct ASC LIMIT 1
```

- **last_error**: validation failed: unknown ORDER BY target: self_interest_pct

#### Agent rows (1)

| superhero_name |
| --- |
| Surtur |

- **final**: SELECT superhero_name FROM superhero

### Q737

- **db**: `superhero` · **EX**: **PASS**
- **tools**: 5: list_databases, list_tables, describe_table, sample_column, execute_sql
- **row set**: gold=1 · agent=1 · both=1 only_gold=0 only_agent=0
- **question**: What is Copycat's race?
- **evidence**: Copycat is the superhero_name;

#### Gold SQL

```sql
SELECT T2.race FROM superhero AS T1 INNER JOIN race AS T2 ON T1.race_id = T2.id WHERE T1.superhero_name = 'Copycat'
```

#### Gold rows (1)

| c1 |
| --- |
| Mutant |

#### Agent SQL

```sql
SELECT race FROM superhero WHERE superhero_name = 'Copycat'
```

#### Agent rows (1)

| race |
| --- |
| Mutant |

- **final**: Mutant

### Q738

- **db**: `superhero` · **EX**: **FAIL**
- **reason**: execute_sql은 성공했으나 gold_rows와 불일치(로직/필터/값)
- **tools**: 6: list_databases, list_tables, describe_table, sample_column, execute_sql, execute_sql
- **row set**: gold=371 · agent=500 · both=313 only_gold=55 only_agent=64
- **question**: Which superheroes have a durability attribute value of less than 50?
- **evidence**: durability of less than 50 refers to attribute_name = 'Durability' AND attribute_value < 50

#### Gold SQL

```sql
SELECT superhero_name FROM superhero AS T1 WHERE EXISTS (SELECT 1 FROM hero_attribute AS T2 INNER JOIN attribute AS T3 ON T2.attribute_id = T3.id WHERE T3.attribute_name = 'Durability' AND T2.attribute_value < 50 AND T1.id = T2.hero_id)
```

#### Gold rows (371)

| c1 |
| --- |
| 3-D Man |
| A-Bomb |
| Abe Sapien |
| Adam Monroe |
| Adam Strange |
| Agent 13 |
| Agent Bob |
| Agent Zero |
| Alex Mercer |
| Alex Woolsly |
| Alfred Pennyworth |
| Alien |
| Ammo |
| Ando Masahashi |
| Angel Dust |
| Angel Salvadore |
| Ant-Man |
| Ant-Man II |
| Apocalypse |
| Aquababy |
| Aqualad |
| Arclight |
| Ares |
| Ariel |
| Armor |
| Atlas |
| Atlas |
| Atlas |
| Atlas |
| Atom Girl |
| Atom II |
| Azrael |
| Bananaman |
| Bane |
| Bantam |
| Batgirl II |
| Batgirl IV |
| Batgirl VI |
| Batman |
| Batman II |

… +331 rows

#### Agent SQL

```sql
SELECT superhero_name FROM superhero WHERE attribute_value < 50
```

#### Agent rows (500)

| superhero_name |
| --- |
| Ammo |
| Ando Masahashi |
| Jack-Jack |
| 3-D Man |
| Abe Sapien |
| Adam Monroe |
| Adam Strange |
| Agent 13 |
| Agent Bob |
| Agent Zero |
| Alex Woolsly |
| Alfred Pennyworth |
| Alien |
| Ammo |
| Ando Masahashi |
| Angel |
| Angel Salvadore |
| Ant-Man |
| Ant-Man II |
| Aquababy |
| Aqualad |
| Archangel |
| Ariel |
| Atom Girl |
| Atom II |
| Aurora |
| Azazel |
| Azrael |
| Bane |
| Banshee |
| Batgirl |
| Batgirl II |
| Batgirl IV |
| Batgirl VI |
| Batman |
| Batman III |
| Batman II |
| Beast Boy |
| Big Daddy |
| Big Man |

… +460 rows

- **final**: The superheroes with a durability attribute value of less than 50 are: Ammo, Ando Masahashi, Jack-Jack, 3-D Man, Abe Sapien, Adam Monroe, Adam Strange, Agent 13, Agent Bob, Agent Zero, Alex Woolsly, Alfred Pennyworth, Alien, Ammo, Ando Masahashi, Angel, Angel Salvadore, Ant-Man, Ant-Man II, Aquababy, Aqualad, Archangel, Ariel, Atom Girl, Atom II, Aurora, Azrael, Bane, Banshee, Batgirl, Batgirl II,…

### Q739

- **db**: `superhero` · **EX**: **PASS**
- **tools**: 5: list_databases, list_tables, describe_table, sample_column, execute_sql
- **row set**: gold=7 · agent=7 · both=7 only_gold=0 only_agent=0
- **question**: What are the names of the superheroes with the power of death touch?
- **evidence**: name of superheroes refers to refers to superhero_name; the power of death touch refers to power_name = 'Death Touch'

#### Gold SQL

```sql
SELECT T1.superhero_name FROM superhero AS T1 INNER JOIN hero_power AS T2 ON T1.id = T2.hero_id INNER JOIN superpower AS T3 ON T2.power_id = T3.id WHERE T3.power_name = 'Death Touch'
```

#### Gold rows (7)

| c1 |
| --- |
| Black Flash |
| Blackwulf |
| Hela |
| Living Tribunal |
| One-Above-All |
| Poison Ivy |
| Spectre |

#### Agent SQL

```sql
SELECT superhero_name FROM superhero WHERE power_name LIKE '%death touch%'
```

#### Agent rows (7)

| superhero_name |
| --- |
| Black Flash |
| Blackwulf |
| Hela |
| Living Tribunal |
| One-Above-All |
| Poison Ivy |
| Spectre |

- **final**: The names of the superheroes with the power of death touch are: Black Flash, Blackwulf, Hela, Living Tribunal, One-Above-All, Poison Ivy, and Spectre.

### Q740

- **db**: `superhero` · **EX**: **FAIL**
- **reason**: execute_sql은 성공했으나 gold_rows와 불일치(로직/필터/값)
- **tools**: 5: list_databases, list_tables, describe_table, sample_column, execute_sql
- **row set**: gold=1 · agent=1 · both=0 only_gold=1 only_agent=1
- **question**: How many female superheroes have a strength value of 100?
- **evidence**: female refers to gender = 'Female'; strength value of 100 refers to attribute_name = 'Strength' AND attribute_value = 100

#### Gold SQL

```sql
SELECT COUNT(T1.id) FROM superhero AS T1 INNER JOIN hero_attribute AS T2 ON T1.id = T2.hero_id INNER JOIN attribute AS T3 ON T2.attribute_id = T3.id INNER JOIN gender AS T4 ON T1.gender_id = T4.id WHERE T3.attribute_name = 'Strength' AND T2.attribute_value = 100 AND T4.gender = 'Female'
```

#### Gold rows (1)

| c1 |
| --- |
| 12 |

#### Agent SQL

```sql
SELECT COUNT(T1.hero_id) FROM superhero AS T1 WHERE T1.gender = 'Female' AND T1.attribute_value = 100
```

#### Agent rows (1)

| count_hero_id |
| --- |
| 46 |

- **final**: There are 46 female superheroes with a strength value of 100.

### Q743

- **db**: `superhero` · **EX**: **FAIL**
- **reason**: execute_sql은 성공했으나 gold_rows와 불일치(로직/필터/값)
- **tools**: 5: list_databases, list_tables, describe_table, sample_column, execute_sql
- **row set**: gold=1 · agent=1 · both=0 only_gold=1 only_agent=1
- **question**: What is the percentage of superheroes who act in their own self-interest or make decisions based on their own moral code? Indicate how many of the said superheroes were published by Marvel Comics.
- **evidence**: published by Marvel Comics refers to publisher_name = 'Marvel Comics'; superheroes who act in their own self-interest or make decisions based on their own moral code refers to alignment = 'Bad'; calculation = MULTIPLY(DIVIDE(SUM(alignment = 'Bad); count(id)), 100)

#### Gold SQL

```sql
SELECT (CAST(COUNT(*) AS REAL) * 100 / (SELECT COUNT(*) FROM superhero)), CAST(SUM(CASE WHEN T2.publisher_name = 'Marvel Comics' THEN 1 ELSE 0 END) AS REAL) FROM superhero AS T1 INNER JOIN publisher AS T2 ON T1.publisher_id = T2.id INNER JOIN alignment AS T3 ON T3.id = T1.alignment_id WHERE T3.alignment = 'Bad'
```

#### Gold rows (1)

| c1 | c2 |
| --- | --- |
| 28.266666666666666 | 118.0 |

#### Agent SQL

```sql
SELECT (SUM(CASE WHEN alignment = 'Bad' THEN 1 ELSE 0 END) * 100.0 / COUNT(hero_id)) AS percentage, COUNT(CASE WHEN alignment = 'Bad' AND publisher_name = 'Marvel Comics' THEN hero_id ELSE NULL END) AS marvel_bad_count FROM superhero
```

#### Agent rows (1)

| percentage | marvel_bad_count |
| --- | --- |
| 28.60998650472335 | 118 |

- **final**: The percentage of superheroes who act in their own self-interest or make decisions based on their own moral code is 28.61%. Of those, 118 were published by Marvel Comics.

### Q744

- **db**: `superhero` · **EX**: **FAIL**
- **reason**: execute_sql은 성공했으나 gold_rows와 불일치(로직/필터/값)
- **tools**: 6: list_databases, list_tables, describe_table, sample_column, execute_sql, execute_sql
- **row set**: gold=1 · agent=1 · both=0 only_gold=1 only_agent=1
- **question**: Between DC and Marvel Comics, which publisher has published more superheroes? Find the difference in the number of superheroes they have published.
- **evidence**: DC refers to publisher_name = 'DC Comics'; Marvel Comics refers to publisher_name = 'Marvel Comics'; calculation = SUBTRACT(SUM(publisher_name = 'Marvel Comics'), SUM(publisher_name = 'DC Comics'))

#### Gold SQL

```sql
SELECT SUM(CASE WHEN T2.publisher_name = 'Marvel Comics' THEN 1 ELSE 0 END) - SUM(CASE WHEN T2.publisher_name = 'DC Comics' THEN 1 ELSE 0 END) FROM superhero AS T1 INNER JOIN publisher AS T2 ON T1.publisher_id = T2.id
```

#### Gold rows (1)

| c1 |
| --- |
| 163 |

#### Agent SQL

```sql
SELECT COUNT(hero_id) FROM superhero WHERE publisher_name = 'DC'
```

#### Agent rows (1)

| count_hero_id |
| --- |
| 0 |

- **final**: Marvel Comics has published 387 superheroes, and DC has published 0. Marvel Comics has published 387 more superheroes than DC.

### Q745

- **db**: `superhero` · **EX**: **FAIL**
- **reason**: 결과 행 수가 gold 대비 과도(조인 팬아웃 의심)
- **tools**: 7: list_databases, list_tables, list_tables, describe_table, search_columns, sample_column, execute_sql
- **row set**: gold=1 · agent=6 · both=0 only_gold=1 only_agent=6
- **question**: Give the publisher ID of Star Trek.
- **evidence**: Star Trek is the publisher_name;

#### Gold SQL

```sql
SELECT id FROM publisher WHERE publisher_name = 'Star Trek'
```

#### Gold rows (1)

| c1 |
| --- |
| 20 |

#### Agent SQL

```sql
SELECT hero_id FROM superhero WHERE publisher_name = 'Star Trek'
```

#### Agent rows (6)

| hero_id |
| --- |
| 216 |
| 364 |
| 369 |
| 393 |
| 551 |
| 650 |

- **final**: The publisher IDs for Star Trek are: 216, 364, 369, 393, 551, 650.

### Q747

- **db**: `superhero` · **EX**: **FAIL**
- **reason**: execute_sql은 성공했으나 gold_rows와 불일치(로직/필터/값)
- **tools**: 4: list_databases, describe_table, sample_column, execute_sql
- **row set**: gold=1 · agent=1 · both=0 only_gold=1 only_agent=1
- **question**: What is the total number of superheroes without full name?
- **evidence**: superheroes without full name refers to full_name IS NULL

#### Gold SQL

```sql
SELECT COUNT(id) FROM superhero WHERE full_name IS NULL
```

#### Gold rows (1)

| c1 |
| --- |
| 122 |

#### Agent SQL

```sql
SELECT COUNT(*) FROM superhero WHERE full_name = '-'
```

#### Agent rows (1)

| count |
| --- |
| 125 |

- **final**: The total number of superheroes without a full name is 125.

### Q750

- **db**: `superhero` · **EX**: **PASS**
- **tools**: 5: list_databases, list_tables, describe_table, sample_column, execute_sql
- **row set**: gold=1 · agent=1 · both=1 only_gold=0 only_agent=0
- **question**: What is the average weight of all female superheroes?
- **evidence**: female refers to gender = 'Female'; average weight refers to AVG(weight_kg)

#### Gold SQL

```sql
SELECT AVG(T1.weight_kg) FROM superhero AS T1 INNER JOIN gender AS T2 ON T1.gender_id = T2.id WHERE T2.gender = 'Female'
```

#### Gold rows (1)

| c1 |
| --- |
| 60.77956989247312 |

#### Agent SQL

```sql
SELECT AVG(weight_kg) FROM superhero WHERE gender = 'Female'
```

#### Agent rows (1)

| avg_weight_kg |
| --- |
| 60.77956989247312 |

- **final**: The average weight of all female superheroes is 60.77956989247312 kg.

### Q751

- **db**: `superhero` · **EX**: **FAIL**
- **reason**: 결과 행 수가 gold 대비 과도(조인 팬아웃 의심)
- **tools**: 5: list_databases, list_tables, describe_table, sample_column, execute_sql
- **row set**: gold=5 · agent=162 · both=5 only_gold=0 only_agent=157
- **question**: List down at least five superpowers of male superheroes.
- **evidence**: male refers to gender = 'Male'; superpowers refers to power_name;

#### Gold SQL

```sql
SELECT T3.power_name FROM superhero AS T1 INNER JOIN hero_power AS T2 ON T1.id = T2.hero_id INNER JOIN superpower AS T3 ON T3.id = T2.power_id INNER JOIN gender AS T4 ON T4.id = T1.gender_id WHERE T4.gender = 'Male' LIMIT 5
```

#### Gold rows (5)

| c1 |
| --- |
| Agility |
| Super Strength |
| Stamina |
| Super Speed |
| Accelerated Healing |

#### Agent SQL

```sql
SELECT DISTINCT power_name FROM superhero WHERE gender = 'Male'
```

#### Agent rows (162)

| power_name |
| --- |
| Agility |
| Super Strength |
| Stamina |
| Super Speed |
| Accelerated Healing |
| Durability |
| Longevity |
| Camouflage |
| Self-Sustenance |
| Cold Resistance |
| Underwater breathing |
| Marksmanship |
| Weapons Master |
| Intelligence |
| Telepathy |
| Immortality |
| Reflexes |
| Enhanced Sight |
| Sub-Mariner |
| Lantern Power Ring |
| Invulnerability |
| Animation |
| Super Breath |
| Dimensional Awareness |
| Flight |
| Size Changing |
| Teleportation |
| Magic |
| Dimensional Travel |
| Molecular Manipulation |
| Energy Manipulation |
| Power Cosmic |
| Energy Absorption |
| Elemental Transmogrification |
| Fire Resistance |
| Natural Armor |
| Heat Resistance |
| Regeneration |
| Stealth |
| Power Suit |

… +122 rows

- **final**: Superpowers of male superheroes include: Agility, Super Strength, Stamina, Super Speed, Accelerated Healing. Other examples include Durability, Flight, Teleportation, Energy Manipulation, and Time Travel.

### Q753

- **db**: `superhero` · **EX**: **FAIL**
- **reason**: execute_sql은 성공했으나 gold_rows와 불일치(로직/필터/값)
- **tools**: 5: list_databases, list_tables, describe_table, sample_column, execute_sql
- **row set**: gold=15 · agent=15 · both=0 only_gold=15 only_agent=11
- **question**: Among the superheroes with height from 170 to 190, list the names of the superheroes with no eye color.
- **evidence**: height from 170 to 190 refers to height_cm BETWEEN 170 AND 190; no eye color refers to colour = 'No Colour'

#### Gold SQL

```sql
SELECT DISTINCT T1.superhero_name FROM superhero AS T1 INNER JOIN colour AS T2 ON T1.eye_colour_id = T2.id WHERE T1.height_cm BETWEEN 170 AND 190 AND T2.colour = 'No Colour'
```

#### Gold rows (15)

| c1 |
| --- |
| Chuck Norris |
| Destroyer |
| Flash III |
| Goku |
| Green Goblin III |
| Indiana Jones |
| Judge Dredd |
| Mister Freeze |
| One Punch Man |
| Paul Blart |
| Robocop |
| Space Ghost |
| Stormtrooper |
| T-1000 |
| Two-Face |

#### Agent SQL

```sql
SELECT full_name FROM superhero WHERE height_cm >= 170 AND height_cm <= 190 AND eye_color = 'No Colour'
```

#### Agent rows (15)

| full_name |
| --- |
|  |
| - |
| Wallace Rudolph West |
| Kakarot |
|  |
| Henry Walton Jones Jr. |
| Joseph Dredd |
| Victor Fries |
| - |
| - |
| Alexander James Murphy |
| Thaddeus Bach |
| - |
| Cyberdyne Systems Series 1000 Terminator |
| Harvey Dent |

- **final**: Wallace Rudolph West, Kakarot, Henry Walton Jones Jr., Joseph Dredd, Victor Fries, Alexander James Murphy, Thaddeus Bach, Cyberdyne Systems Series 1000 Terminator, Harvey Dent

### Q758

- **db**: `superhero` · **EX**: **FAIL**
- **reason**: 결과 행 수가 gold 대비 과도(조인 팬아웃 의심)
- **tools**: 4: list_databases, list_tables, describe_table, execute_sql
- **row set**: gold=5 · agent=36 · both=5 only_gold=0 only_agent=2
- **question**: Provide the hair colour of the human superhero who is 185 cm tall.
- **evidence**: 185 cm tall refers to height_cm = 185; human superhero refers to race = 'human'; hair colour refers to colour where hair_colour_id = colour.id;

#### Gold SQL

```sql
SELECT DISTINCT T3.colour FROM superhero AS T1 INNER JOIN race AS T2 ON T1.race_id = T2.id INNER JOIN colour AS T3 ON T1.hair_colour_id = T3.id WHERE T1.height_cm = 185 AND T2.race = 'Human'
```

#### Gold rows (5)

| c1 |
| --- |
| Blond |
| Brown |
| Black |
| Brown/White |
| No Colour |

#### Agent SQL

```sql
SELECT hair_colour FROM superhero WHERE height_cm = 185
```

#### Agent rows (36)

| hair_colour |
| --- |
| No Colour |
| Blond |
| Blond |
| Brown |
| No Colour |
| Black |
| Red |
| Red/Grey |
| Brown |
| Brown |
| No Colour |
| No Colour |
| Red |
| Brown |
| Blond |
| Black |
| Brown |
| Brown |
| Red |
| Black |
| No Colour |
| Black |
| Brown |
| No Colour |
| Brown/White |
| Brown |
| Black |
| No Colour |
| Brown |
| Brown |
| No Colour |
| Black |
| Black |
| No Colour |
| Brown |
| Brown |

- **final**: The hair colours of superheroes who are 185 cm tall are: No Colour, Blond, Blond, Brown, No Colour, Black, Red, Red/Grey, Brown, Brown, No Colour, No Colour, Red, Brown, Blond, Black, Brown, Brown, Red, Black, No Colour, Black, Brown, No Colour, Brown/White, Brown, Black, No Colour, Brown, Brown, No Colour, Black, Black, No Colour, Brown, Brown.

### Q760

- **db**: `superhero` · **EX**: **PASS**
- **tools**: 6: list_databases, list_tables, describe_table, sample_column, sample_column, execute_sql
- **row set**: gold=1 · agent=1 · both=1 only_gold=0 only_agent=0
- **question**: In superheroes with height between 150 to 180, what is the percentage of heroes published by Marvel Comics?
- **evidence**: height between 150 to 180 refers to height_cm BETWEEN 150 AND 180; heroes published by Marvel Comics refers to publisher_name = 'Marvel Comics'; calculation = MULTIPLY(DIVIDE(SUM(publisher.id = 13)), COUNT(publisher.id), 100)

#### Gold SQL

```sql
SELECT CAST(COUNT(CASE WHEN T2.publisher_name = 'Marvel Comics' THEN 1 ELSE NULL END) AS REAL) * 100 / COUNT(T1.id) FROM superhero AS T1 INNER JOIN publisher AS T2 ON T1.publisher_id = T2.id WHERE T1.height_cm BETWEEN 150 AND 180
```

#### Gold rows (1)

| c1 |
| --- |
| 60.869565217391305 |

#### Agent SQL

```sql
SELECT CAST(SUM(CASE WHEN height_cm >= 150 AND height_cm <= 180 AND publisher_name = 'Marvel Comics' THEN 1 ELSE 0 END) AS REAL) * 100.0 / COUNT(CASE WHEN height_cm >= 150 AND height_cm <= 180 THEN 1 ELSE NULL END) FROM superhero
```

#### Agent rows (1)

| cast_sum_case_when_height_cm_150_and_height_cm_180_and_publisher_name_marvel_comics_then_1_else_0_end_as_real_100_0_count_case_when_height_cm_150_and_height_cm_180_then_1_else_null_end |
| --- |
| 60.869565217391305 |

- **final**: 60.87%

### Q761

- **db**: `superhero` · **EX**: **PASS**
- **tools**: 6: list_databases, list_tables, describe_table, execute_sql, sample_column, execute_sql
- **row set**: gold=1 · agent=1 · both=1 only_gold=0 only_agent=0
- **question**: Among the male superheroes, list the super hero names of superheroes with weight greater than the 79% average weight of all superheroes.
- **evidence**: super hero names refers to superhero_name;male superheros refers to gender = 'Male';Calculation = weight_kg > MULTIPLY(AVG(weight_kg), 0.79)

#### Gold SQL

```sql
SELECT T1.superhero_name FROM superhero AS T1 INNER JOIN gender AS T2 ON T1.gender_id = T2.id WHERE T2.gender = 'Male' AND T1.weight_kg * 100 > ( SELECT AVG(weight_kg) FROM superhero ) * 79
```

#### Gold rows (1)

| c1 |
| --- |
| King Kong |

#### Agent SQL

```sql
SELECT superhero_name FROM superhero WHERE gender = 'Male' AND weight_kg > (SELECT 0.79 * AVG(weight_kg) FROM superhero)
```

- **last_error**: validation failed: semantic SELECT must have exactly one FROM item

#### Agent rows (1)

| superhero_name |
| --- |
| King Kong |

- **final**: King Kong

### Q764

- **db**: `superhero` · **EX**: **PASS**
- **tools**: 5: list_databases, list_tables, describe_table, sample_column, execute_sql
- **row set**: gold=4 · agent=4 · both=4 only_gold=0 only_agent=0
- **question**: What are the superpowers of heroes with ID 1?
- **evidence**: superpowers refers to power_name; heroes with ID 1 refers to hero_id = 1;

#### Gold SQL

```sql
SELECT DISTINCT T2.power_name FROM hero_power AS T1 INNER JOIN superpower AS T2 ON T1.power_id = T2.id WHERE T1.hero_id = 1
```

#### Gold rows (4)

| c1 |
| --- |
| Agility |
| Super Strength |
| Stamina |
| Super Speed |

#### Agent SQL

```sql
SELECT power_name FROM superhero WHERE hero_id = '1'
```

#### Agent rows (4)

| power_name |
| --- |
| Agility |
| Super Strength |
| Stamina |
| Super Speed |

- **final**: The superpowers of heroes with ID 1 are: Agility, Super Strength, Stamina, and Super Speed.

### Q765

- **db**: `superhero` · **EX**: **FAIL**
- **reason**: execute_sql은 성공했으나 gold_rows와 불일치(로직/필터/값)
- **tools**: 5: list_databases, list_tables, describe_table, sample_column, execute_sql
- **row set**: gold=1 · agent=1 · both=0 only_gold=1 only_agent=1
- **question**: How many heroes have stealth power?
- **evidence**: stealth power refers to power_name = 'Stealth';

#### Gold SQL

```sql
SELECT COUNT(T1.hero_id) FROM hero_power AS T1 INNER JOIN superpower AS T2 ON T1.power_id = T2.id WHERE T2.power_name = 'Stealth'
```

#### Gold rows (1)

| c1 |
| --- |
| 127 |

#### Agent SQL

```sql
SELECT count(hero_id) FROM superhero WHERE power_name = 'stealth'
```

#### Agent rows (1)

| count_hero_id |
| --- |
| 0 |

- **final**: There is 1 hero with stealth power.

### Q766

- **db**: `superhero` · **EX**: **PASS**
- **tools**: 6: list_databases, list_tables, describe_table, sample_column, execute_sql, execute_sql
- **row set**: gold=1 · agent=1 · both=1 only_gold=0 only_agent=0
- **question**: What is the hero's full name with the highest attribute in strength?
- **evidence**: highest attribute in strength refers to MAX(attribute_value) WHERE attribute_name = 'strength';

#### Gold SQL

```sql
SELECT T1.full_name FROM superhero AS T1 INNER JOIN hero_attribute AS T2 ON T1.id = T2.hero_id INNER JOIN attribute AS T3 ON T2.attribute_id = T3.id WHERE T3.attribute_name = 'Strength' ORDER BY T2.attribute_value DESC LIMIT 1
```

#### Gold rows (1)

| c1 |
| --- |
| Richard Milhouse Jones |

#### Agent SQL

```sql
SELECT full_name FROM superhero WHERE attribute_name = 'Strength' ORDER BY attribute_value DESC LIMIT 1
```

#### Agent rows (1)

| full_name |
| --- |
| Richard Milhouse Jones |

- **final**: Richard Milhouse Jones

### Q769

- **db**: `superhero` · **EX**: **PASS**
- **tools**: 6: list_databases, list_tables, describe_table, sample_column, sample_column, execute_sql
- **row set**: gold=1 · agent=1 · both=1 only_gold=0 only_agent=0
- **question**: Which superhero has the most durability published by Dark Horse Comics?
- **evidence**: which superhero refers to superhero_name; most durability refers to MAX(attribute_value) WHERE attribute_name = 'durability'; published by Dark Horse Comics refers to publisher_name = 'Dark Horse Comics';

#### Gold SQL

```sql
SELECT T1.superhero_name FROM superhero AS T1 INNER JOIN hero_attribute AS T2 ON T1.id = T2.hero_id INNER JOIN attribute AS T3 ON T3.id = T2.attribute_id INNER JOIN publisher AS T4 ON T4.id = T1.publisher_id WHERE T4.publisher_name = 'Dark Horse Comics' AND T3.attribute_name = 'Durability' ORDER BY T2.attribute_value DESC LIMIT 1
```

#### Gold rows (1)

| c1 |
| --- |
| Dash |

#### Agent SQL

```sql
SELECT T1.superhero_name FROM superhero AS T1 WHERE T1.publisher_name = 'Dark Horse Comics' AND T1.attribute_name = 'Durability' ORDER BY T1.attribute_value DESC LIMIT 1
```

#### Agent rows (1)

| superhero_name |
| --- |
| Dash |

- **final**: Dash

### Q772

- **db**: `superhero` · **EX**: **FAIL**
- **reason**: execute_sql은 성공했으나 gold_rows와 불일치(로직/필터/값)
- **tools**: 6: list_databases, list_tables, describe_table, sample_column, sample_column, execute_sql
- **row set**: gold=5 · agent=5 · both=0 only_gold=5 only_agent=5
- **question**: List the eyes, hair and skin colour of all female superheroes published by Dark Horse Comics.
- **evidence**: eyes refers to eye_colour_id; hair refers to hair_colour_id; skin colour refers to skin_colour_id; female superheroes refers to gender = 'Female'; published by Dark Horse Comics refers to publisher_name = 'Dark Horse Comics';

#### Gold SQL

```sql
SELECT T1.eye_colour_id, T1.hair_colour_id, T1.skin_colour_id FROM superhero AS T1 INNER JOIN publisher AS T2 ON T2.id = T1.publisher_id INNER JOIN gender AS T3 ON T3.id = T1.gender_id WHERE T2.publisher_name = 'Dark Horse Comics' AND T3.gender = 'Female'
```

#### Gold rows (5)

| c1 | c2 | c3 |
| --- | --- | --- |
| 14 | 6 | 1 |
| 9 | 9 | 1 |
| 1 | 1 | 1 |
| 1 | 1 | 28 |
| 30 | 4 | 1 |

#### Agent SQL

```sql
SELECT eye_color, hair_colour, skin_color FROM superhero WHERE publisher_name = 'Dark Horse Comics' AND gender = 'Female'
```

#### Agent rows (5)

| eye_color | hair_colour | skin_color |
| --- | --- | --- |
| Green | Blond | No Colour |
| Brown | Brown | No Colour |
| No Colour | No Colour | No Colour |
| No Colour | No Colour | Silver |
| Violet | Black | No Colour |

- **final**: The eye color, hair color, and skin color of female superheroes published by Dark Horse Comics are:
- Green eyes, Blond hair, No Colour skin
- Brown eyes, Brown hair, No Colour skin
- No Colour eyes, No Colour hair, No Colour skin
- No Colour eyes, No Colour hair, Silver skin
- Violet eyes, Black hair, No Colour skin

### Q773

- **db**: `superhero` · **EX**: **FAIL**
- **reason**: execute_sql은 성공했으나 gold_rows와 불일치(로직/필터/값)
- **tools**: 6: list_databases, list_tables, describe_table, sample_column, sample_column, execute_sql
- **row set**: gold=157 · agent=56 · both=2 only_gold=155 only_agent=54
- **question**: Which superhero has the same eyes, hair and skin colour? Indicate the publisher of the superhero.
- **evidence**: which superhero refers to superhero_name; the same eyes, hair and skin colour refers to hair_colour_id = skin_colour_id AND hair_colour_id = eye_colour_id; publisher refers to publisher_name;

#### Gold SQL

```sql
SELECT T1.superhero_name, T2.publisher_name FROM superhero AS T1 INNER JOIN publisher AS T2 ON T1.publisher_id = T2.id WHERE T1.eye_colour_id = T1.hair_colour_id AND T1.eye_colour_id = T1.skin_colour_id
```

#### Gold rows (157)

| c1 | c2 |
| --- | --- |
| Agent Zero | Marvel Comics |
| Alex Mercer | Wildstorm |
| Alex Woolsly | NBC - Heroes |
| Allan Quatermain | Wildstorm |
| Ando Masahashi | NBC - Heroes |
| Angel | Dark Horse Comics |
| Angela | Image Comics |
| Anti-Spawn | Image Comics |
| Arsenal | DC Comics |
| Aztar | DC Comics |
| Bananaman |  |
| Bane | DC Comics |
| Batgirl III | DC Comics |
| Batgirl V | DC Comics |
| Beast | Marvel Comics |
| Beast Boy | DC Comics |
| Beetle | Marvel Comics |
| Ben 10 | DC Comics |
| Beta Ray Bill | Marvel Comics |
| Beyonder | Marvel Comics |
| Big Daddy | Icon Comics |
| Bill Harken | SyFy |
| Billy Kincaid | Image Comics |
| Bird-Brain | Marvel Comics |
| Bird-Man | Marvel Comics |
| Bird-Man II | Marvel Comics |
| Birdman | Hanna-Barbera |
| Black Flash | DC Comics |
| Black Goliath | Marvel Comics |
| Bling! | Marvel Comics |
| Bolt | Marvel Comics |
| Bomb Queen | Image Comics |
| Boomer | Marvel Comics |
| Box | Marvel Comics |
| Brundlefly |  |
| Bumbleboy | Marvel Comics |
| Bushido | DC Comics |
| Cameron Hicks | SyFy |
| Captain Midnight | Dark Horse Comics |
| Captain Universe | Marvel Comics |

… +117 rows

#### Agent SQL

```sql
SELECT T1.superhero_name, T1.publisher_name FROM superhero AS T1 GROUP BY T1.eye_color, T1.hair_colour, T1.skin_color HAVING count(*) > 1
```

#### Agent rows (56)

| superhero_name | publisher_name |
| --- | --- |
| Lucifer Morningstar | DC Comics |
| Armor | Marvel Comics |
| Bi-Beast | Marvel Comics |
| Beak | Marvel Comics |
| Electro | Marvel Comics |
| Abraxas | Marvel Comics |
| Archangel | Marvel Comics |
| Adam Monroe | NBC - Heroes |
| Atlas | DC Comics |
| Franklin Storm | Marvel Comics |
| Abe Sapien | Dark Horse Comics |
| Leonardo | IDW Publishing |
| Absorbing Man | Marvel Comics |
| Psylocke | Marvel Comics |
| Atom | DC Comics |
| Man of Miracles | Image Comics |
| Crimson Crusader | Marvel Comics |
| Air-Walker | Marvel Comics |
| Atom II | DC Comics |
| Ajax | Marvel Comics |
| Danny Cooper | HarperCollins |
| Agent Bob | Marvel Comics |
| Brother Voodoo | Marvel Comics |
| 3-D Man | Marvel Comics |
| Bishop | Marvel Comics |
| Atlas | Marvel Comics |
| Black Widow | Marvel Comics |
| Batgirl IV | DC Comics |
| Black Cat | Marvel Comics |
| Cheetah II | DC Comics |
| Beast Boy | DC Comics |
| Lyja | Marvel Comics |
| Plantman | Marvel Comics |
| Brainiac | DC Comics |
| Abomination | Marvel Comics |
| Batgirl | DC Comics |
| Doctor Strange | Marvel Comics |
| Colin Wagner | HarperCollins |
| James T. Kirk | Star Trek |
| Captain Hindsight | South Park |

… +16 rows

- **final**: Based on the data retrieved, there are several superheroes who share the same eye color, hair color, and skin color. Here are some examples of these matches:

*   **Lucifer Morningstar** (Publisher: DC Comics)
*   **Armor** (Publisher: Marvel Comics)
*   **Bi-Beast** (Publisher: Marvel Comics)
*   **Beast** (Publisher: Marvel Comics)
*   **Electro** (Publisher: Marvel Comics)
*   **Abraxas** (Publ…

### Q775

- **db**: `superhero` · **EX**: **FAIL**
- **reason**: execute_sql은 성공했으나 gold_rows와 불일치(로직/필터/값)
- **tools**: 5: list_databases, list_tables, describe_table, sample_column, execute_sql
- **row set**: gold=1 · agent=1 · both=0 only_gold=1 only_agent=1
- **question**: What is the percentage of blue female superheroes among all female superheroes?
- **evidence**: percentage = MULTIPLY(DIVIDE(SUM(colour = 'Blue' WHERE gender = 'Female'), COUNT(gender = 'Female')), 100); blue refers to the color = 'Blue' WHERE skin_colour_id = colour.id; female refers to gender = 'Female';

#### Gold SQL

```sql
SELECT CAST(COUNT(CASE WHEN T3.colour = 'Blue' THEN T1.id ELSE NULL END) AS REAL) * 100 / COUNT(T1.id) FROM superhero AS T1 INNER JOIN gender AS T2 ON T1.gender_id = T2.id INNER JOIN colour AS T3 ON T1.skin_colour_id = T3.id WHERE T2.gender = 'Female'
```

#### Gold rows (1)

| c1 |
| --- |
| 2.4630541871921183 |

#### Agent SQL

```sql
SELECT CAST(SUM(CASE WHEN gender = 'female' AND hair_colour = 'blue' THEN 1 ELSE 0 END) AS REAL) * 100.0 / SUM(CASE WHEN gender = 'female' THEN 1 ELSE 0 END) FROM superhero
```

#### Agent rows (1)

| cast_sum_case_when_gender_female_and_hair_colour_blue_then_1_else_0_end_as_real_100_0_sum_case_when_gender_female_then_1_else_0_end |
| --- |
|  |

- **final**: Based on the retrieved data, the percentage of blue female superheroes among all female superheroes is null.

### Q779

- **db**: `superhero` · **EX**: **PASS**
- **tools**: 5: list_databases, list_tables, describe_table, sample_column, execute_sql
- **row set**: gold=1 · agent=1 · both=1 only_gold=0 only_agent=0
- **question**: How many powers does Amazo hero have?
- **evidence**: Amazo hero refers to superhero_name = 'Amazo';

#### Gold SQL

```sql
SELECT COUNT(T1.power_id) FROM hero_power AS T1 INNER JOIN superhero AS T2 ON T1.hero_id = T2.id WHERE T2.superhero_name = 'Amazo'
```

#### Gold rows (1)

| c1 |
| --- |
| 44 |

#### Agent SQL

```sql
SELECT count(DISTINCT power_name) FROM superhero WHERE superhero_name = 'Amazo'
```

#### Agent rows (1)

| count_distinct_power_name |
| --- |
| 44 |

- **final**: Amazo hero has 44 powers.

### Q781

- **db**: `superhero` · **EX**: **PASS**
- **tools**: 5: list_databases, list_tables, describe_table, sample_column, execute_sql
- **row set**: gold=3 · agent=3 · both=3 only_gold=0 only_agent=0
- **question**: Provide the heights of the heroes whose eye colours are amber.
- **evidence**: heights of the heroes refers to height_cm; eye colours are amber refers to colour.colour = 'Amber' WHERE eye_colour_id = colour.id;

#### Gold SQL

```sql
SELECT T1.height_cm FROM superhero AS T1 INNER JOIN colour AS T2 ON T1.eye_colour_id = T2.id WHERE T2.colour = 'Amber'
```

#### Gold rows (3)

| c1 |
| --- |
| 178 |
| 198 |
| 175 |

#### Agent SQL

```sql
SELECT height_cm FROM superhero WHERE eye_color = 'Amber'
```

#### Agent rows (3)

| height_cm |
| --- |
| 178 |
| 198 |
| 175 |

- **final**: The heights of the heroes whose eye colours are amber are 178, 198, and 175.

### Q782

- **db**: `superhero` · **EX**: **FAIL**
- **reason**: execute_sql은 성공했으나 gold_rows와 불일치(로직/필터/값)
- **tools**: 5: list_databases, list_tables, describe_table, sample_column, execute_sql
- **row set**: gold=11 · agent=11 · both=1 only_gold=10 only_agent=8
- **question**: List the heroes' names whose eyes and hair colours are both black.
- **evidence**: heroes' names refers to superhero_name; eyes and hair colours are both black refers to eye_colour_id AND hair_colour_id WHERE colour.colour = 'Black';

#### Gold SQL

```sql
SELECT T1.superhero_name FROM superhero AS T1 INNER JOIN colour AS T2 ON T1.eye_colour_id = T2.id AND T1.hair_colour_id = T2.id WHERE T2.colour = 'Black'
```

#### Gold rows (11)

| c1 |
| --- |
| Armor |
| Atom Girl |
| Bizarro |
| Galactus |
| General Zod |
| Match |
| Negasonic Teenage Warhead |
| Phantom Stranger |
| Shadow Lass |
| Sinestro |
| Vulcan |

#### Agent SQL

```sql
SELECT full_name FROM superhero WHERE eye_color = 'Black' AND hair_colour = 'Black'
```

#### Agent rows (11)

| full_name |
| --- |
| Hisako Ichiki |
| Salu Digby |
| Bizarro |
|  |
| Dru-Zod |
| Kent Connor |
| - |
| - |
| - |
| Thaal Sinestro |
| Gabriel Summers |

- **final**: Hisako Ichiki, Salu Digby, Bizarro

### Q785

- **db**: `superhero` · **EX**: **PASS**
- **tools**: 5: list_databases, list_tables, describe_table, sample_column, execute_sql
- **row set**: gold=28 · agent=28 · both=28 only_gold=0 only_agent=0
- **question**: Describe the names of neutral alignment superheroes.
- **evidence**: names of superheroes refers to superhero_name; neutral alignment refers to alignment = 'Neutral';

#### Gold SQL

```sql
SELECT T1.superhero_name FROM superhero AS T1 INNER JOIN alignment AS T2 ON T1.alignment_id = T2.id WHERE T2.alignment = 'Neutral'
```

#### Gold rows (28)

| c1 |
| --- |
| Ares |
| Bizarro |
| Black Flash |
| Brundlefly |
| Captain Cold |
| Copycat |
| Deadpool |
| Deathstroke |
| Etrigan |
| Galactus |
| Gladiator |
| Indigo |
| Juggernaut |
| Living Tribunal |
| Lobo |
| Lucifer Morningstar |
| Man-Bat |
| One-Above-All |
| Phantom Stranger |
| Raven |
| Red Hood |
| Red Hulk |
| Robin VI |
| Sandman |
| Sinestro |
| The Comedian |
| The Presence |
| Toad |

#### Agent SQL

```sql
SELECT superhero_name FROM superhero WHERE alignment = 'Neutral'
```

#### Agent rows (28)

| superhero_name |
| --- |
| Ares |
| Bizarro |
| Black Flash |
| Brundlefly |
| Captain Cold |
| Copycat |
| Deadpool |
| Deathstroke |
| Etrigan |
| Galactus |
| Gladiator |
| Indigo |
| Juggernaut |
| Living Tribunal |
| Lobo |
| Lucifer Morningstar |
| Man-Bat |
| One-Above-All |
| Phantom Stranger |
| Raven |
| Red Hood |
| Red Hulk |
| Robin VI |
| Sandman |
| Sinestro |
| The Comedian |
| The Presence |
| Toad |

- **final**: The names of neutral alignment superheroes are: Ares, Bizarro, Black Flash, Brundlefly, Captain Cold, Copycat, Deadpool, Deathstroke, Etrigan, Galactus, Gladiator, Indigo, Juggernaut, Living Tribunal, Lobo, Lucifer Morningstar, Man-Bat, One-Above-All, Phantom Stranger, Raven, Red Hood, Red Hulk, Robin VI, Sandman, Sinestro, The Comedian, The Presence, and Toad.

### Q786

- **db**: `superhero` · **EX**: **PASS**
- **tools**: 5: list_databases, list_tables, describe_table, sample_column, execute_sql
- **row set**: gold=1 · agent=1 · both=1 only_gold=0 only_agent=0
- **question**: How many heroes have the highest attribute value in strength?
- **evidence**: highest attribute value in strength refers to MAX(attribute_value) WHERE attribute_name = 'Strength';

#### Gold SQL

```sql
SELECT COUNT(T1.hero_id) FROM hero_attribute AS T1 INNER JOIN attribute AS T2 ON T1.attribute_id = T2.id WHERE T2.attribute_name = 'Strength' AND T1.attribute_value = ( SELECT MAX(attribute_value) FROM hero_attribute )
```

#### Gold rows (1)

| c1 |
| --- |
| 63 |

#### Agent SQL

```sql
SELECT COUNT(hero_id) FROM superhero WHERE attribute_name = 'Strength' AND attribute_value = (SELECT MAX(attribute_value) FROM superhero WHERE attribute_name = 'Strength')
```

#### Agent rows (1)

| count_hero_id |
| --- |
| 63 |

- **final**: There are 63 heroes with the highest attribute value in strength.

### Q788

- **db**: `superhero` · **EX**: **FAIL**
- **reason**: execute_sql은 성공했으나 gold_rows와 불일치(로직/필터/값)
- **tools**: 5: list_databases, list_tables, describe_table, sample_column, execute_sql
- **row set**: gold=1 · agent=1 · both=0 only_gold=1 only_agent=1
- **question**: How many percent of female heroes were published by Marvel Comics?
- **evidence**: percent = MULTIPLY(DIVIDE(SUM(gender = 'Female' WHERE publisher_name = 'Marvel Comics'), COUNT(publisher_name = 'Marvel Comics')), 100); female heroes refers to gender = 'Female'; Marvel Comics refers to publisher_name = 'Marvel Comics';

#### Gold SQL

```sql
SELECT CAST(COUNT(CASE WHEN T3.gender = 'Female' AND T2.publisher_name = 'Marvel Comics' THEN 1 ELSE NULL END) AS REAL) / COUNT(CASE WHEN T2.publisher_name = 'Marvel Comics' THEN 1 ELSE NULL END) * 100 FROM superhero AS T1 INNER JOIN publisher AS T2 ON T1.publisher_id = T2.id INNER JOIN gender AS T3 ON T1.gender_id = T3.id
```

#### Gold rows (1)

| c1 |
| --- |
| 28.68217054263566 |

#### Agent SQL

```sql
SELECT CAST(SUM(CASE WHEN publisher_name = 'Marvel Comics' AND gender = 'female' THEN 1 ELSE 0 END) AS REAL) * 100.0 / COUNT(CASE WHEN gender = 'female' THEN 1 END) FROM superhero
```

#### Agent rows (1)

| cast_sum_case_when_publisher_name_marvel_comics_and_gender_female_then_1_else_0_end_as_real_100_0_count_case_when_gender_female_then_1_end |
| --- |
|  |

- **final**: 0.0%

### Q790

- **db**: `superhero` · **EX**: **FAIL**
- **reason**: execute_sql은 성공했으나 gold_rows와 불일치(로직/필터/값)
- **tools**: 5: list_databases, list_tables, describe_table, sample_column, execute_sql
- **row set**: gold=1 · agent=2 · both=0 only_gold=1 only_agent=2
- **question**: Calculate the difference between Emil Blonsky's weight and Charles Chandler's weight.
- **evidence**: difference = SUBTRACT(SUM(weight_kg WHERE full_name = 'Emil Blonsky'), SUM(weight_kg WHERE full_name = 'Charles Chandler')); Emil Blonsky is the full name of superhero; Charles Chandler is the full name of superhero;

#### Gold SQL

```sql
SELECT ( SELECT weight_kg FROM superhero WHERE full_name LIKE 'Emil Blonsky' ) - ( SELECT weight_kg FROM superhero WHERE full_name LIKE 'Charles Chandler' ) AS CALCULATE
```

#### Gold rows (1)

| c1 |
| --- |
| 351 |

#### Agent SQL

```sql
SELECT weight_kg FROM superhero WHERE full_name IN ('Emil Blonsky', 'Charles Chandler')
```

#### Agent rows (2)

| weight_kg |
| --- |
| 90 |
| 441 |

- **final**: The difference between Emil Blonsky's weight (90 kg) and Charles Chandler's weight (441 kg) is 351 kg.

### Q791

- **db**: `superhero` · **EX**: **FAIL**
- **reason**: execute_sql은 성공했으나 gold_rows와 불일치(로직/필터/값)
- **tools**: 4: list_databases, list_tables, describe_table, execute_sql
- **row set**: gold=1 · agent=1 · both=0 only_gold=1 only_agent=1
- **question**: Calculate the average height for all superhero.
- **evidence**: average = DIVIDE(SUM(height_cm), COUNT(all heros));

#### Gold SQL

```sql
SELECT CAST(SUM(height_cm) AS REAL) / COUNT(id) FROM superhero
```

#### Gold rows (1)

| c1 |
| --- |
| 247.04533333333333 |

#### Agent SQL

```sql
SELECT AVG(height_cm) FROM superhero
```

#### Agent rows (1)

| avg_height_cm |
| --- |
| 267.7514450867052 |

- **final**: The average height for all superheroes is 267.7514450867052.

### Q792

- **db**: `superhero` · **EX**: **PASS**
- **tools**: 7: list_databases, list_tables, list_tables, describe_table, sample_column, sample_column, execute_sql
- **row set**: gold=8 · agent=8 · both=8 only_gold=0 only_agent=0
- **question**: What is Abomination's superpower?
- **evidence**: Abomination refers to superhero_name = 'Abomination'; superpower refers to power_name;

#### Gold SQL

```sql
SELECT T3.power_name FROM superhero AS T1 INNER JOIN hero_power AS T2 ON T1.id = T2.hero_id INNER JOIN superpower AS T3 ON T2.power_id = T3.id WHERE T1.superhero_name = 'Abomination'
```

#### Gold rows (8)

| c1 |
| --- |
| Accelerated Healing |
| Intelligence |
| Super Strength |
| Stamina |
| Super Speed |
| Invulnerability |
| Animation |
| Super Breath |

#### Agent SQL

```sql
SELECT power_name FROM superhero WHERE superhero_name = 'Abomination'
```

#### Agent rows (8)

| power_name |
| --- |
| Accelerated Healing |
| Intelligence |
| Super Strength |
| Stamina |
| Super Speed |
| Invulnerability |
| Animation |
| Super Breath |

- **final**: The superpowers listed for Abomination are: Accelerated Healing, Intelligence, Super Strength, Stamina, Super Speed, Invulnerability, Animation, and Super Breath.

### Q794

- **db**: `superhero` · **EX**: **FAIL**
- **reason**: execute_sql은 성공했으나 gold_rows와 불일치(로직/필터/값)
- **tools**: 4: list_databases, describe_table, sample_column, execute_sql
- **row set**: gold=1 · agent=1 · both=0 only_gold=1 only_agent=1
- **question**: Which hero was the fastest?
- **evidence**: which hero refers to superhero_name; fastest refers to MAX(attribute_value) WHERE attribute_name = 'Speed';

#### Gold SQL

```sql
SELECT T1.superhero_name FROM superhero AS T1 INNER JOIN hero_attribute AS T2 ON T1.id = T2.hero_id INNER JOIN attribute AS T3 ON T2.attribute_id = T3.id WHERE T3.attribute_name = 'Speed' ORDER BY T2.attribute_value DESC LIMIT 1
```

#### Gold rows (1)

| c1 |
| --- |
| Air-Walker |

#### Agent SQL

```sql
SELECT superhero_name FROM superhero WHERE attribute_name = 'Speed' LIMIT 1
```

#### Agent rows (1)

| superhero_name |
| --- |
| 3-D Man |

- **final**: 3-D Man

### Q796

- **db**: `superhero` · **EX**: **PASS**
- **tools**: 5: list_databases, list_tables, describe_table, sample_column, execute_sql
- **row set**: gold=6 · agent=6 · both=6 only_gold=0 only_agent=0
- **question**: State all of 3-D Man's attributes along with their values.
- **evidence**: 3-D Man is the superhero_name. attributes refers to attribute_name; values refers to attribute_value;

#### Gold SQL

```sql
SELECT T3.attribute_name, T2.attribute_value FROM superhero AS T1 INNER JOIN hero_attribute AS T2 ON T1.id = T2.hero_id INNER JOIN attribute AS T3 ON T2.attribute_id = T3.id WHERE T1.superhero_name = '3-D Man'
```

#### Gold rows (6)

| c1 | c2 |
| --- | --- |
| Intelligence | 80 |
| Strength | 35 |
| Speed | 45 |
| Durability | 45 |
| Power | 45 |
| Combat | 45 |

#### Agent SQL

```sql
SELECT attribute_name, attribute_value FROM superhero WHERE superhero_name = '3-D Man'
```

#### Agent rows (6)

| attribute_name | attribute_value |
| --- | --- |
| Intelligence | 80 |
| Strength | 35 |
| Speed | 45 |
| Durability | 45 |
| Power | 45 |
| Combat | 45 |

- **final**: The attributes and their values for "3-D Man" are:
*   Intelligence: 80
*   Strength: 35
*   Speed: 45
*   Durability: 45
*   Power: 45
*   Combat: 45

### Q797

- **db**: `superhero` · **EX**: **FAIL**
- **reason**: 필터/조인으로 결과 공집합(gold는 비공집합)
- **tools**: 5: list_databases, list_tables, describe_table, sample_column, execute_sql
- **row set**: gold=22 · agent=0 · both=0 only_gold=21 only_agent=0
- **question**: Which superheroes have blue eyes with brown hair?
- **evidence**: which superheroes refers to superhero_name; blue eyes refers to color = 'Blue' and color.id = eye_colour_id; brown hair refers to color = 'Brown' and color.id = hair_colour_id;

#### Gold SQL

```sql
SELECT T1.superhero_name FROM superhero AS T1 INNER JOIN colour AS T2 ON T1.eye_colour_id = T2.id INNER JOIN colour AS T3 ON T1.hair_colour_id = T3.id WHERE T2.colour = 'Blue' AND T3.colour = 'Brown'
```

#### Gold rows (22)

| c1 |
| --- |
| Atlas |
| Atlas |
| Big Man |
| Bloodaxe |
| Blue Beetle |
| Blue Beetle II |
| Captain Epic |
| Chewbacca |
| Fabian Cortez |
| Gravity |
| Hawkman |
| Hercules |
| Jack-Jack |
| Mister Knife |
| Molecule Man |
| Multiple Man |
| Rick Flag |
| Scarecrow |
| Scarlet Witch |
| Spider-Girl |
| Toxin |
| X-Man |

#### Agent SQL

```sql
SELECT superhero_name FROM superhero WHERE eye_color = 'blue' AND hair_colour = 'brown'
```

#### Agent rows (0)

_empty_

- **final**: Based on the available data, no superheroes were found with blue eyes and brown hair.

### Q798

- **db**: `superhero` · **EX**: **PASS**
- **tools**: 5: list_databases, list_tables, describe_table, sample_column, execute_sql
- **row set**: gold=3 · agent=3 · both=1 only_gold=0 only_agent=0
- **question**: What is the publisher for Hawkman, Karate Kid and Speedy?
- **evidence**: publisher refers to publisher_name; Hawkman refers to superhero_name = 'Hawkman'; Karate Kid refers to superhero_name = 'Karate Kid'; Speedy refers to superhero_name = 'Speedy';

#### Gold SQL

```sql
SELECT T2.publisher_name FROM superhero AS T1 INNER JOIN publisher AS T2 ON T1.publisher_id = T2.id WHERE T1.superhero_name IN ('Hawkman', 'Karate Kid', 'Speedy')
```

#### Gold rows (3)

| c1 |
| --- |
| DC Comics |
| DC Comics |
| DC Comics |

#### Agent SQL

```sql
SELECT publisher_name FROM superhero WHERE superhero_name = 'Hawkman' OR superhero_name = 'Karate Kid' OR superhero_name = 'Speedy'
```

#### Agent rows (3)

| publisher_name |
| --- |
| DC Comics |
| DC Comics |
| DC Comics |

- **final**: The publisher for Hawkman, Karate Kid, and Speedy is DC Comics.

### Q800

- **db**: `superhero` · **EX**: **PASS**
- **tools**: 5: list_databases, list_tables, describe_table, sample_column, execute_sql
- **row set**: gold=1 · agent=1 · both=1 only_gold=0 only_agent=0
- **question**: Calculate the percentage of superheroes with blue eyes.
- **evidence**: percentage = MULTIPLY(DIVIDE(SUM(superhero_name WHERE color = 'Blue'), COUNT(superhero_name)), 100.0); blue eyes refers to color = 'Blue' and color.id =  eye_colour_id = 7;

#### Gold SQL

```sql
SELECT CAST(COUNT(CASE WHEN T2.colour = 'Blue' THEN 1 ELSE NULL END) AS REAL) * 100 / COUNT(T1.id) FROM superhero AS T1 INNER JOIN colour AS T2 ON T1.eye_colour_id = T2.id
```

#### Gold rows (1)

| c1 |
| --- |
| 31.2 |

#### Agent SQL

```sql
SELECT CAST(SUM(CASE WHEN eye_color = 'Blue' THEN 1 ELSE 0 END) AS REAL) * 100.0 / COUNT(hero_id) FROM superhero
```

#### Agent rows (1)

| cast_sum_case_when_eye_color_blue_then_1_else_0_end_as_real_100_0_count_hero_id |
| --- |
| 31.2 |

- **final**: 31.2% of superheroes have blue eyes.

### Q801

- **db**: `superhero` · **EX**: **FAIL**
- **reason**: execute_sql은 성공했으나 gold_rows와 불일치(로직/필터/값)
- **tools**: 5: list_databases, list_tables, describe_table, sample_column, execute_sql
- **row set**: gold=1 · agent=1 · both=0 only_gold=1 only_agent=1
- **question**: Find the ratio between male superheroes and female superheroes.
- **evidence**: ratio = DIVIDE(SUM(gender_id = 1) / SUM(gender_id = 2)); male superheroes refers to gender = 'Female'; female superheroes refers to gender = 'Male';

#### Gold SQL

```sql
SELECT CAST(COUNT(CASE WHEN T2.gender = 'Male' THEN T1.id ELSE NULL END) AS REAL) / COUNT(CASE WHEN T2.gender = 'Female' THEN T1.id ELSE NULL END) FROM superhero AS T1 INNER JOIN gender AS T2 ON T1.gender_id = T2.id
```

#### Gold rows (1)

| c1 |
| --- |
| 2.5566502463054186 |

#### Agent SQL

```sql
SELECT CAST(SUM(CASE WHEN gender = 'Male' THEN 1 ELSE 0 END) AS REAL) / SUM(CASE WHEN gender = 'Female' THEN 1 ELSE 0 END) FROM superhero
```

#### Agent rows (1)

| cast_sum_case_when_gender_male_then_1_else_0_end_as_real_sum_case_when_gender_female_then_1_else_0_end |
| --- |
| 1.0 |

- **final**: The ratio between male superheroes and female superheroes is 1.0.

### Q806

- **db**: `superhero` · **EX**: **PASS**
- **tools**: 5: list_databases, list_tables, describe_table, sample_column, execute_sql
- **row set**: gold=1 · agent=1 · both=1 only_gold=0 only_agent=0
- **question**: Provide the eye colour of the superhero who has Karen Beecher-Duncan as their full name.
- **evidence**: eye colour refers to colour.colour where eye_colour_id = colour.id; Karen Beecher-Duncan is the full name of superhero;

#### Gold SQL

```sql
SELECT T2.colour FROM superhero AS T1 INNER JOIN colour AS T2 ON T1.eye_colour_id = T2.id WHERE T1.full_name = 'Karen Beecher-Duncan'
```

#### Gold rows (1)

| c1 |
| --- |
| Brown |

#### Agent SQL

```sql
SELECT eye_color FROM superhero WHERE full_name = 'Karen Beecher-Duncan'
```

#### Agent rows (1)

| eye_color |
| --- |
| Brown |

- **final**: Brown

### Q819

- **db**: `superhero` · **EX**: **FAIL**
- **reason**: execute_sql은 성공했으나 gold_rows와 불일치(로직/필터/값)
- **tools**: 6: list_databases, list_tables, describe_table, sample_column, execute_sql, execute_sql
- **row set**: gold=1 · agent=1 · both=0 only_gold=1 only_agent=1
- **question**: In superheroes with missing weight data, calculate the difference between the number of superheroes with blue eyes and no eye color.
- **evidence**: missing weight data refers to weight_kg = 0 OR T1.weight_kg = NULL; difference = SUBTRACT(SUM(colour.id = 7), SUM(colour.id = 1)); blue eyes refers to eye_colour_id WHERE colour.id = 7; no eye color refers to eye_colour_id WHERE colour.id = 1;

#### Gold SQL

```sql
SELECT SUM(CASE WHEN T2.id = 7 THEN 1 ELSE 0 END) - SUM(CASE WHEN T2.id = 1 THEN 1 ELSE 0 END) FROM superhero AS T1 INNER JOIN colour AS T2 ON T1.eye_colour_id = T2.id WHERE T1.weight_kg = 0 OR T1.weight_kg is NULL
```

#### Gold rows (1)

| c1 |
| --- |
| -122 |

#### Agent SQL

```sql
SELECT SUM(CASE WHEN eye_color = 'Blue' THEN 1 ELSE 0 END) - SUM(CASE WHEN eye_color = 'No Colour' THEN 1 ELSE 0 END) FROM superhero
```

- **last_error**: sql parse error: A '(' has no matching ')'. Close it before the token the parser found. Parser said: sql parser error: Expected: ), found: FROM at Line: 1, Column: 117

#### Agent rows (1)

| sum_case_when_eye_color_blue_then_1_else_0_end_sum_case_when_eye_color_no_colour_then_1_else_0_end |
| --- |
| 0 |

- **final**: 0

### Q822

- **db**: `superhero` · **EX**: **FAIL**
- **reason**: execute_sql은 성공했으나 gold_rows와 불일치(로직/필터/값)
- **tools**: 6: list_databases, list_tables, describe_table, sample_column, sample_column, execute_sql
- **row set**: gold=1 · agent=1 · both=0 only_gold=1 only_agent=1
- **question**: How many green-skinned villains are there in the superhero universe?
- **evidence**: green-skinned refers to colour.colour = 'Green' WHERE skin_colour_id = colour.id; villains refers to alignment = 'Bad';

#### Gold SQL

```sql
SELECT COUNT(T1.id) FROM superhero AS T1 INNER JOIN alignment AS T2 ON T1.alignment_id = T2.id INNER JOIN colour AS T3 ON T1.skin_colour_id = T3.id WHERE T2.alignment = 'Bad' AND T3.colour = 'Green'
```

#### Gold rows (1)

| c1 |
| --- |
| 6 |

#### Agent SQL

```sql
SELECT COUNT(skin_color) FROM superhero WHERE skin_color = 'green'
```

#### Agent rows (1)

| count_skin_color |
| --- |
| 0 |

- **final**: There are 0 green-skinned villains in the superhero universe based on the available data.

### Q824

- **db**: `superhero` · **EX**: **PASS**
- **tools**: 6: list_databases, list_tables, describe_table, sample_column, sample_column, execute_sql
- **row set**: gold=3 · agent=3 · both=3 only_gold=0 only_agent=0
- **question**: Identify superheroes who can control wind and list their names in alphabetical order.
- **evidence**: superheroes refers to superhero_name; can control wind refers to power_name = 'Wind Control';

#### Gold SQL

```sql
SELECT T1.superhero_name FROM superhero AS T1 INNER JOIN hero_power AS T2 ON T1.id = T2.hero_id INNER JOIN superpower AS T3 ON T2.power_id = T3.id WHERE T3.power_name = 'Wind Control' ORDER BY T1.superhero_name
```

#### Gold rows (3)

| c1 |
| --- |
| Crystal |
| Red Tornado |
| Tempest |

#### Agent SQL

```sql
SELECT superhero_name FROM superhero WHERE power_name LIKE '%wind%' ORDER BY superhero_name
```

#### Agent rows (3)

| superhero_name |
| --- |
| Crystal |
| Red Tornado |
| Tempest |

- **final**: Crystal, Red Tornado, Tempest

### Q825

- **db**: `superhero` · **EX**: **PASS**
- **tools**: 5: list_databases, list_tables, describe_table, sample_column, execute_sql
- **row set**: gold=1 · agent=1 · both=1 only_gold=0 only_agent=0
- **question**: Identify the gender of the superhero who has the ability of Phoenix Force.
- **evidence**: ability of Phoenix Force refers to power_name = 'Phoenix Force';

#### Gold SQL

```sql
SELECT T4.gender FROM superhero AS T1 INNER JOIN hero_power AS T2 ON T1.id = T2.hero_id INNER JOIN superpower AS T3 ON T2.power_id = T3.id INNER JOIN gender AS T4 ON T1.gender_id = T4.id WHERE T3.power_name = 'Phoenix Force'
```

#### Gold rows (1)

| c1 |
| --- |
| Female |

#### Agent SQL

```sql
SELECT gender FROM superhero WHERE power_name = 'Phoenix Force'
```

#### Agent rows (1)

| gender |
| --- |
| Female |

- **final**: Female

### Q829

- **db**: `superhero` · **EX**: **FAIL**
- **reason**: execute_sql은 성공했으나 gold_rows와 불일치(로직/필터/값)
- **tools**: 8: list_databases, list_tables, describe_table, sample_column, execute_sql, execute_sql, execute_sql, execute_sql
- **row set**: gold=1 · agent=1 · both=0 only_gold=1 only_agent=1
- **question**: Which publisher created more superheroes: DC or Marvel Comics? Find the difference in the number of superheroes.
- **evidence**: DC refers to publisher_name = 'DC Comics'; Marvel Comics refers to publisher_name = 'Marvel Comics'; difference = SUBTRACT(SUM(publisher_name = 'DC Comics'), SUM(publisher_name = 'Marvel Comics'));

#### Gold SQL

```sql
SELECT SUM(CASE WHEN T2.publisher_name = 'DC Comics' THEN 1 ELSE 0 END) - SUM(CASE WHEN T2.publisher_name = 'Marvel Comics' THEN 1 ELSE 0 END) FROM superhero AS T1 INNER JOIN publisher AS T2 ON T1.publisher_id = T2.id
```

#### Gold rows (1)

| c1 |
| --- |
| -163 |

#### Agent SQL

```sql
SELECT SUM(CASE WHEN publisher_name = 'Marvel Comics' THEN 1 ELSE 0 END) - SUM(CASE WHEN publisher_name = 'DC Comics' THEN 1 ELSE 0 END) FROM superhero
```

#### Agent rows (1)

| sum_case_when_publisher_name_marvel_comics_then_1_else_0_end_sum_case_when_publisher_name_dc_comics_then_1_else_0_end |
| --- |
| 0 |

- **final**: SELECT SUM(CASE WHEN publisher_name = 'Marvel Comics' THEN 1 ELSE 0 END) - SUM(CASE WHEN publisher_name = 'DC Comics' THEN 1 ELSE 0 END) FROM superhero
rows:[[163]]


