# 포켓 지그재그 경로 개편 — 헬리컬 진입 · 슬롯 ap 단계 절입 · 정삭 윤곽 (2026-10-19)

앱 기본 작업인 합성 `pocket_zigzag` 를 CAM 포켓 황삭처럼 바꿨다. 예전 경로의 최종 벽은 슬롯형 이동이 남긴 황삭 벽이었고, 인코넬 프리셋에서
공구 휨 0.5 mm · 몸체 응력이 정적 강도에 닿았다 (`docs/pocket-wall-error.md`). 코드(`gcode::pocket_plan`, `GCodeGenerator::pocket_zigzag`)를 먼저
따라가고, 제조사 자료로 값을 정한 뒤, 자료가 갈리거나 없는 값은 감사의 `[UNKNOWN] pocket_path_strategy` 로 남겼다.

## 1. 예전 경로와 새 경로

| | 예전 | 지금 |
|---|---|---|
| 진입 | 직선 플런지로 ap 전체 (인코넬 12 mm = 1·D) | 단계마다 헬리컬 (구멍 1.5·D, 소재별 경사각, 회전당 하강 π·(구멍 − D)·tan α). 포켓이 좁으면 첫 줄 위 직선 램프 (한 번에 0.5·D 이하), 그것도 안 되면 플런지 |
| 깊이 | ap 전체 한 번 | 슬롯형 이동(첫 줄 슬롯 · 줄 끝 피치 이동)이 소재의 슬롯 ap 를 넘지 않게 같은 깊이로 단계 절입 (`WorkpieceProps::slot_ap_ratio`) |
| 이송 | 모든 이동이 프리셋 이송 (피치 이동만 ×0.7) | 슬롯형 이동·진입 = 소재 목표 칩 두께 fz (프리셋 fz 에 든 작은 ae 용 칩 얇아짐 배수를 뺌, `gcode::slot_fz_mm`), 측면 절삭 줄 = 프리셋 이송, 정삭 = 프리셋 이송 (원호 진입은 절반). 안쪽 원호(헬리컬 · 황삭/정삭 모서리 · 원호 진입·이탈)는 공구 중심 이송을 ρ/(ρ + r) 로 줄여 날 끝이 그 이송으로 감 (`gcode::inner_arc_feed`) |
| 줄 | 피치 = 프리셋 ae 를 0.2~0.6·D 로 묶음, 마지막 줄이 경계에 못 닿아 위쪽 스킨이 두꺼워짐 | 같은 피치 상한으로 줄 간격을 고르게 — 첫 줄과 마지막 줄이 황삭 경계에 닿음 |
| 벽 | 황삭 벽이 곧 최종 벽 | 벽에 정삭 여유(= 스톡 여유)를 남기고, 마지막에 전체 깊이 정삭 윤곽 한 바퀴 (하향 — 포켓 안쪽 벽은 반시계, CFRP 만 상향), 아래 벽 가운데에서 원호로 들어가(이송 절반) 원호로 나옴 |
| 모서리 | 공구 반경 그대로 (정삭하면 모서리에서 공구가 약 137° 를 물어 인코넬 공구 휨 449 µm) | 벽 모서리 반경 4/3·r — 정삭 공구 중심은 모서리에서 반경 r/3 원호를 안쪽 원호 이송 보정 F·rc/(rc + r) 로, 황삭 경계 모서리도 같은 중심에서 정삭 여유만큼 작은 원호 (모서리에도 여유가 고름) |
| 포켓 벽 두께 | 스톡 가장자리 1 mm 스킨 (여유 D/2 + 1) | 벽 높이(포켓 깊이, 판이 더 얇으면 판 두께)의 1/3, 0.8 mm 이상 |

최종 벽 판정도 거기에 맞췄다: `loadsim::exclude_recut_walls` — 벽을 만든 세그먼트 뒤에 **실행된** 이송 경로의 공구(반경 r 원통)가 벽 점 높이 아래까지
내려간 채로 축을 벽 점에서 r − 셀/4 안으로 가져오면 그 벽은 깎여 나간 것으로 본다. 단계 절입 황삭 벽은 z 가운데가 단계마다 달라 전체 깊이
정삭 윤곽의 벽과 `flag_final_walls` 의 높이 키가 다르고, 정삭 여유가 탐침 거리(1.5 셀)보다 얇으면(뷰포트 격자에서 고경도 0.15 mm ≈ 1 셀) 탐침도
정삭 벽 너머 소재에 닿아 황삭 벽이 최종 벽으로 남았다. 멈춘 작업의 남은 경로가 다시 깎을 벽은 이미 main 의 `exclude_uncut_boundary` (f6f5a36) 가
뺀다 — 두 규칙이 같은 조각(`feed_pieces`)·색인(`PieceIndex`)·거리(`tip_reach`)를 쓴다. 정삭 윤곽 전에 멈춘 작업은 남은 정삭이 황삭 벽을 모두
다시 깎으므로 최종 벽이 없고, 그때는 벽 위치 오차·측벽 Ra 통계를 내지 않는다 (0, 경고 한 줄 — 예전에는 과도 벽 전체로 통계를 내 인코넬 정지에서 314 µm 가 찍혔다).

## 2. 리서치 (제조사 자료 · 초경 솔리드 엔드밀)

### 2.1 슬롯(ae = D) ap

| 소재 | 제조사 값 (ap/D) | 쓴 값 |
|---|---|---|
| 알루미늄 2~3날 | SGS S-Carb · YG-1 Alu-Power HPC · Haas HSAM2 1.0 (같은 양식) · Helical H45AL-C-3 1.0~1.5 · Guhring Uni-Pro 0.5~1.0 · Mitsubishi Alimaster 0.5 (Ø12~25) | 1.0 |
| 탄소강 4날 Ø10 | SGS Z-Carb AP · OSG AE-VMS · Haas HTPM 1.0 · Kennametal HARVI I TE P2 1.25 · Helical HSV-4 0.75~1.25 · Guhring 0.5~1.0 | 1.0 |
| 합금강 ≤ 30 HRC / 30~45 / > 45 | 탄소강과 같음 / Kennametal HPHVBN P4 0.75 · OSG AE-VMS 1.0 · Helical 0.6~1.0 · OSG WXL 0.5 · SGS Z-Carb MD 0.4 / SGS Z-Carb MD 0.4 · OSG WXL 0.5 · GARANT Diabolo 70 (60~70 HRC) 0.05 · NS Tool '측면 ap 의 50 % 이하' | 1.0 / 0.75 / 0.4 |
| SUS304 Ø8 | OSG AE-VMS 1.0 (D > 6) · SGS · Haas 1.0 · Helical HSV-3 (3날 부등분할) 1.0~1.5 · Kennametal M1 1.25 · Guhring 0.5~1.0 | 1.0 |
| Ti-6Al-4V Ø10 | OSG AE-VMS 0.25 · Haas HTPM 0.5 · Guhring 0.5~1.0 · SGS ≤ 1.0 · Helical HSV-4 0.6~1.25 · Kennametal HPHVBN/UKDV 1.0 · HARVI I TE 1.25 | 1.0 (가운데값 — 퍼짐은 `[UNKNOWN]`) |
| 인코넬 718 · 내열합금 | Kennametal HARVI II TE 5날 Ø12 0.75 · Seco JHP780 0.6 · Helical HEV-5 0.25~0.5 · Mitsubishi VQN6MVRB 0.5 · Haas 0.5 · OSG 0.25 · Kennametal HARVI I TE S3 0.5 · HPHVBN 0.3 | 0.75 (`pocket-wall-error.md` 의 값, 같은 급 공구) |
| CFRP 다이아몬드 | Amana CVD 12날 라우터 1.0 (2·D 는 칩 하중 −25 %) · Harvey 다이아몬드 컷 1.0 · OSG AERO-REC < 1.0 | 1.0 |
| 볼 엔드밀 폭 전체 (58 HRC) | 자료 없음 — 윤곽 가공 자료뿐 (NS Tool MRBH230 ap 0.1~0.3 mm, Harvey 반황삭 0.2·D × 0.12·D) | 스퀘어 공구 값 0.4 (`[UNKNOWN]`) |

슬롯 fz 는 제조사마다 측면 fz 대비 0.5~1.0 배 (Kennametal −20 %, SGS·Haas 같음, OSG 0.65~0.7 — 칩 얇아짐을 뺀 값과 비슷, Helical 0.5~0.6).
이 경로는 칩 얇아짐 배수만 뺀다 (소재 목표 칩 두께 = `recommend_conditions` 가 배수를 곱하기 전 값).

### 2.2 헬리컬 진입

* 경사각은 공구 중심 경로 기준 (Tungaloy · Kennametal · Walter): 회전당 하강 P = π·(구멍 − D)·tan α.
* 경사각: 알루미늄 3.0° (IMCO 3~5° · Seco JHP490 회전당 5 % Dc), 탄소강·합금강 1.2° (Seco JS554 · JH120 · JHP170 · JH111 2 % Dc · IMCO 1~2.5°),
  스테인리스 · Ti · 인코넬 · 내열합금 1.8° (Seco JS553 · JHP770 · JHP780 3 % Dc · IMCO M525 1~2.5°), CFRP 자료 없음 → 1.2° (`[UNKNOWN]`).
* 구멍 지름 1.5·D — Kennametal 예제 (Ø16 에 24 mm), 허용 1.15~1.9·D (HARVI I TE: 최소 1.1·D + 2·Re, 최대 2·D − 2·Re), Seco 권장 1.2~1.3·Dc,
  Harvey 1.1~1.2·D 초과, Sandvik · Walter 2·D 까지. 1.2·D 가 안 들어가는 포켓은 직선 램프.
* 진입 깊이: 헬리컬은 제조사가 따로 묶지 않음 (IMCO: 원하는 깊이까지 나선으로 내려간 뒤 넓힘) → 단계 깊이(슬롯 ap)만. 직선 램프는 IMCO 'Max Ramp
  Depth 50 % of D'.
* 이송: 날 끝 = 슬롯 이송 (IMCO 헬리컬 '슬롯 속도·이송', SGS 경사 1~2° 슬롯 이송), 공구 중심 = ×(구멍 − D)/구멍 (Kennametal M443 · Tungaloy — 구멍 1.5·D 면 1/3).
  Seco 는 날 끝 이송을 스테인리스 · Ti · 내열합금 35~40 %, 고경도 볼 20 % 로, Kennametal 은 구멍/D 1.35~1.6 에서 −10 % 로 더 줄인다 (`[UNKNOWN]`).
  같은 보정을 모든 안쪽 원호에 쓴다 — 보정 없이 반경 0.5 mm 황삭 모서리를 돌면 Ø6 공구의 날 끝 이송이 7배가 되어 알루미늄 황삭 프리셋이 8 % 에서 과부하로 부러졌다.
* 직선 플런지: Harvey "공구 파손으로 이어지기 쉬움", SGS "비철·짧은 칩 소재에서만, 슬롯 이송의 25 %", Walter "헬리컬 또는 시작 구멍".

### 2.3 정삭 여유 · 정삭 윤곽 · 벽

* 정삭 여유 = 작업물의 스톡 여유 (`rect_profile` 의 ae 와 같은 값, D/2 이하). 제조사 측면 정삭 ae: Seco 소재별 2~4 % D, Haas ≤ 2 % D, Helical 4~6 % D,
  CAM 예제 0.3~0.5 mm (Autodesk), Onshape 0.8 mm.
* 정삭은 전체 깊이 한 번 (Seco 0.8~1.5·Dc, Helical 날 길이까지, IMCO 1.5·D, Sandvik: 여러 번 나누면 이음 자국). 하향이 첫 선택 (Sandvik · Iscar ·
  Harvey · Autodesk), CFRP 는 상향 (Sandvik CoroMill Plura 복합재). 원호로 굴러 들어감 (Sandvik), 들어가는 동안 이송 절반 (Harvey).
* 지그재그 황삭 뒤 경계 정삭: FeatureCAM 경계 정리, Creo 'Clean Up Cut', NX 'Zig with Contour', Vectric 래스터 + 프로파일.
* 포켓 벽 두께: RapidDirect '최적 H/t 3:1', Protolabs '3:1', SendCutSend t ≥ H/8, Boeing 정삭 ap ≤ 8·t, Xometry 10:1, Sandvik 15:1 미만 '작음'.
  최소 두께 Protolabs Network 0.8 mm (금속), RapidDirect 강 1.5~3 · Ti 2~3 mm. 예전 1 mm × 12 mm 스킨은 모두 어김.
* 모서리: Sandvik 정삭 '공구 지름 ≤ 1.5 × 모서리 반경' (R ≥ 1.33·r), RapidDirect · Mechanical-Engineering.com '공구 반경보다 30 % 크게', Xometry '10~20 %'
  → 4/3·r. 모서리 원호의 공구 중심 이송은 날 끝이 벽에서 직선 구간과 같은 이송으로 가게 F·rc/(rc + r) (Kennametal 카탈로그 M443 'F2 = F1·(D − d1)/D',
  Tungaloy 'tool-center feed = feed × (Dh − Dc)/Dh'). 황삭 피치 이동의 모서리 원호(반경 = 정삭 원호 − 정삭 여유)도 같은 보정으로 날 끝 = 슬롯 이송.

## 3. 전후 (내장 8 프리셋 · pocket_zigzag · `examples/pocket_probe.rs`)

이전 = main (f6f5a36 미절삭 경계 제외까지) + 얇은 벽 하중 수정, 옛 경로. 이후 = 이 경로. 최종 벽 위치 오차는 뷰포트 격자의 최종 벽 점 기준 (최대 / 90 % 분위).
알루미늄 · Ti · 탄소강 · 스테인리스 · CFRP · 알루미늄 정삭은 네 정책 모두 결과가 같아(수명 판정 전에 끝남) 한 줄로 적었다.

| 프리셋 · 정책 | 이전: 결과 · 절삭 · 최종 벽 오차 최대 / p90 | 이후: 결과 · 절삭 · 최종 벽 오차 최대 / p90 |
|---|---|---|
| 알루미늄 황삭 (모든 정책) | 완료 · 2.76 min · 51.7 / 50.7 µm | 완료 · 3.49 min · 51.6 / 28.8 µm |
| 티타늄 정삭 (모든 정책) | 완료 · 4.51 min · 176.8 / 158.0 µm | 완료 · 7.94 min · 125.2 / 55.0 µm |
| 탄소강 범용 (모든 정책) | 완료 · 3.43 min · 231.8 / 171.5 µm | 완료 · 4.81 min · 163.6 / 86.5 µm |
| 스테인리스 정삭 (모든 정책) | 완료 · 9.37 min · 159.7 / 121.6 µm | 완료 · 11.27 min · 85.1 / 36.6 µm |
| 인코넬 황삭 · 정지 (기본) | 61 % 에서 정지 (수명 판정) · 2.60 min · 스킨 117점 510.3 / 478.3 µm | 12 % 에서 정지 (수명 판정) · 4.60 min · 최종 벽 없음 (정삭 전) |
| 인코넬 황삭 · 교체 | 공구 2개로 완료 · 4.25 min · 521.2 / 421.4 µm | 공구 4개로 완료 · 16.42 min · 296.7 / 159.7 µm |
| 인코넬 황삭 · 끝까지 · 감시 없음 | 71 % 에서 과부하 파손 · 3.02 min | 13 % 에서 날끝 붕괴 · 4.74 min |
| 금형강 58 HRC 정삭 (Ø6 볼) · 정지 | 0 % 에서 과부하 파손 | 1 % 에서 치핑 감지로 정지 · 2.47 min |
| 금형강 · 교체 | 0 % 에서 과부하 파손 | 공구 52개 · 33 % 에서 과부하 파손 · 35.3 min |
| 금형강 · 끝까지 · 감시 없음 | 0 % 에서 과부하 파손 | 1 % 에서 치핑 파손 · 2.47 min |
| CFRP 트리밍 (모든 정책) | 완료 · 4.90 min · 55.6 / 52.0 µm | 완료 · 5.89 min · 26.3 / 26.2 µm |
| 알루미늄 정삭 (모든 정책) | 완료 · 1.42 min · 99.0 / 84.0 µm | 완료 · 1.60 min · 78.9 / 49.7 µm |

* 최종 벽은 모두 정삭 윤곽이 만든다 (프로브 `[POCKET-F]`: 정삭 윤곽 밖의 최종 벽 0점). 최대 오차는 대개 모서리(벽 반경 4/3·r 원호 — 결합이
  직선 벽의 약 R/rc = 4배 반경 절입)나 소재 열팽창(알루미늄 정삭 51 µm)이 정한다.
* 절삭 시간이 는 몫의 대부분은 헬리컬 진입이다 — 공구 중심 이송이 슬롯 이송의 1/3 이라 인코넬은 단계마다 약 4.7 분 (두 단계), Ti 약 3.4 분.
  인코넬은 경로 수명 3~5 분 안에서 진입·첫 줄을 깎다가 수명 판정에 닿아 정지 정책이 12 % 에서 멈춘다 (교체 정책은 공구 4개).
* 고경도 볼 포켓은 여전히 깎을 수 없는 일 — 즉시 과부하 대신 치핑 감지·치핑 파손이 먼저 오고, 교체 정책은 공구 52개를 쓰고도 둘째 단계에서 부러진다.
* N:N 행렬 (`pair_matrix -- <폴더> pocket_zigzag --check`, 공구 8 × 사용 사례 8, 207 s): 완료 28 · 수명 판정 정지 35 · 파손 1 (스테인리스 × Ø6 볼 롱넥,
  12 % 과부하) · 판정 적합 13 / 주의 26 / 부적합 25. 검사 위반 2건은 모두 '현실 확인'(계산 결함 0) — 의도된 짝 인코넬 × Ø12 AlTiN 12 % 정지와
  금형강 × Ø6 볼 1 % 정지로, 위 표의 두 경우다. 나머지 의도된 짝 6개는 완료 (최종 벽 오차 최대: 알루미늄 51.6 · 탄소강 160.8 · 합금강 185.7 ·
  스테인리스 85.1 · Ti 126.3 · CFRP 26.3 µm).

### 시험 기대값 (사용자 합의 2026-10-19: DFM 모서리 반영, 정삭 전 정지는 '최종 벽 없음')

* `tool_life_matrix::a_finishing_ball_cannot_rough_a_hardened_pocket` — 예전: 모든 정책 '경로 20 % 안에 파손'. 지금: 어느 정책으로도 끝내지 못함
  (완료 50 % 미만), 정지는 치핑 감지로 5 % 안에 멈춤(파손 아님), 끝까지·감시 없음은 5 % 안에 치핑 파손, 교체는 공구 10개 이상.
* `tool_life_matrix::pocket_skin_carries_only_its_own_chip_force…` · `a_stopped_pocket_counts_only_part_walls_as_final` — 옛 경로가 만든 스킨으로 얇은 벽
  하중 모델·미절삭 경계 판정을 보는 시험이라 그 경로를 시험 안 고정 경로(`legacy_zigzag`)로 옮김 (슬롯형 이동 경고 단언은 슬롯 패턴 단위 시험으로).
* `tool_life_matrix::a_pocket_stopped_before_its_finishing_contour_has_no_part_walls_yet` (새 시험) — 기본 경로 인코넬 정지: 최종 벽 0점, 미절삭 경계는
  모두 남은 경로가 덮는 벽 (독립 계산), 벽 오차·Ra 통계 0, 경고, 품질 보고·세그먼트 벽·감사가 같은 판정.
* `tool_damage_continue::a_damaged_tool_stop_offers_to_continue_and_finishing_with_it_marks_the_degraded_walls` — 인코넬은 정삭 전에 멈춰 품질 저하가 없으므로,
  품질 저하 표시는 한계 가까이 쓴 공구(VB 0.95 × 한계)로 탄소강 포켓을 깎아 본다: 끝까지 가면 정삭 윤곽 벽 전체가 품질 저하, 예비 공구면 정상.
* `wear_compensation::titanium_pocket_plans_a_reduced_second_pass_…` (예전 `titanium_pocket_holds_…`) — 예전: 보류 (옛 경로의 큰 잔여량으로 2차 휨
  불확도가 공차 절반을 넘는 게이트 3등급, 상향 줄 벽의 과삭 게이트 2등급). 지금: 보정이 필요한 구간은 정삭 윤곽 9개뿐, 하향 정삭이라 과삭 0등급,
  2차 휨 불확도 4.3 µm < 5 µm → 규칙이 감속 2차(`ExecuteReduced`)를 계획하고 1차 프로그램은 그대로. 항복한 벽 하위 시나리오(1차 무효 → 보류)는 같음.
* `wear_compensation::laya_cannot_lower_a_rule_hold_…` — 규칙 보류 시나리오를 항복한 벽(1차 유효성 3등급)으로 만든다 (Ti 포켓 그대로는 이제 2차를 깎음).
* `wear_compensation::titanium_finishing_holds_…every_cut_is_blocked` · `the_choice_card_…` — '모든 절삭이 막힌' 시나리오의 이송 배수 0.70 → 0.60:
  잔여량이 줄어(상위 10 % 약 43 µm) 0.70 배에서는 분할 마지막 칩 3.84 µm 가 최소 칩 3.72 µm 를 넘어 분할이 열렸다 (0.60 배: 분할 3.34 · 1회 4.44 µm).
* `wear_compensation::far_walls_gain_stock_from_thermal_expansion_…` — 곧은 벽만 본다 (모서리 원호는 법선이 두 축에 걸침): 먼 벽 > 20 µm, 원점 쪽 벽 < 1 µm,
  오른쪽/위 벽 비 1.2~1.9 (≈ W/H 1.5, 탄소강 39.4 / 25.1 µm).

### 새 경로가 드러낸 하류 모델 결함 (같이 고침)

탄소강·Ti·알루미늄 포켓 첫 부품과 그 공구로 깎은 다음 부품 (`examples/wear_probe.rs`):

| 프리셋 | 스칼라 VB 성장 | 그중 헬리컬 진입 (ap 0.3~0.5 mm · 바닥 전폭) | ap 띠 평균 (마모장) | 다음 부품 실제 | 예전 부품당 예측 | 지금 |
|---|---|---|---|---|---|---|
| 티타늄 정삭 | 0.101 mm | 70 % (대부분 새 날 길들이기 항) | 0.031 mm | 0.048 mm | 0.020 (×0.41) | 0.049 (×1.03) |
| 탄소강 범용 | 0.040 mm | 40 % | 0.025 mm | 0.019 mm | 0.015 (×0.80) | 0.019 (×1.00) |
| 알루미늄 황삭 | 0.028 mm | 19 % | 0.022 mm | 0.013 mm | 0.011 (×0.83) | 0.014 (×1.07) |

* **부품당 VB 증가** (`wearlog::analyze_ext`, 화면 '다음 부품당 VB 증가'): 작업 끝 구간의 시계열 기울기를 작업 시간만큼 늘려 썼는데, 끝이 가벼운
  정삭 윤곽이면 부품 하나를 크게 낮춰 봤다. 이제 부품당 성장은 경로 물리 궤적(이번 작업 성장으로 맞춘 경로 계수, 지금 VB 에서 한 부품 시간)으로
  잡고 시계열 융합(TTM·Holt·물리)은 P90/P50 폭에만 쓴다. `tool_state::tool_change_parts_left_follows_…` 의 허용 범위를 0.6~1.6 → 0.8~1.25 배로 좁힘.
* **참조 작업 묶음** (`store::jobs::reference_summary`): 기록을 작업 id 로 묶어, 1차 질의에 정삭 체제가 생기자 재계획 작업의 2차 기록이 같은 부품의
  두 번째 '참조 작업'으로 잡혔다. 이제 뿌리 작업(부품) 단위로 묶고, 실측 비도 부품 단위로 쓴다 (`tool_state::second_pass_wear_is_charged_…`).
* **깊이 띠 VB** (`tool_state` 의 저장 VB 시험 3개): 공구 장부는 깊이별 마모장의 ap 띠 평균을 저장하고 진입 마모는 날끝 칸에만 쌓이므로, 예전
  '저장값 ≈ 경로 끝 스칼라 VB (2 % 안)' 는 맞지 않는다. 이제 저장값 = 시뮬레이션 마모장(1차 + 이 공구의 2차) 띠 평균, 그리고 띠 평균이
  체제별 dvb × min(ap/띠 ap, 1) 합과 깊이 칸 하나 안에서 맞는지(깊이 회계) 본다. 작업 안의 VB(수명 판정·벽 마모 보정)는 그대로 깊이를 가리지
  않는 값이라 `[UNKNOWN] shallow_cut_wear_depth` 로 남긴다 (아래 4절).
* **잔열 기록** (`loadsim`, `environment_tools::residual_heat_lives_only_after_contact_and_decays`): 헬리컬 구멍 바닥(첫 단계 깊이)을 지나는 직선 이동이 격자
  잡음만 스친 스텝(결합 깊이·폭 0, 절삭력 0 — Ti 정삭 추천 조건 4,191 샘플 중 1개)에 주변 잔열 3.5 °C 가 기록됐다. 잔열은 칩을 만드는 접촉(힘 > 0)에서만
  읽고 열 계산에 넣는다.

## 4. 파악이 안 된 부분 (`[UNKNOWN] pocket_path_strategy`)

* 슬롯 ap 퍼짐 (Ti 0.25~1.25·D, Ni 0.25~0.75·D, 58 HRC 0.05~0.5·D) — 범위 끝이면 단계 수·절삭 시간·공구 응력이 움직임.
* 볼 엔드밀의 폭 전체 ap (58 HRC) — 자료 없음, 스퀘어 값 0.4·D 로 나눔.
* 헬리컬 이송 (슬롯 이송의 20~100 %), CFRP 헬리컬 경사·구멍 지름.
* 정삭 여유를 스톡 여유로 둠 (제조사 정삭 ae 2~6 % D 보다 큰 프리셋도 있음 — 탄소강 1.0 mm = 10 % D).
* 모서리 반경 4/3·r 은 부품 설계값으로 가정 (실제 도면이 더 작으면 모서리 결합이 커짐).
* 확인: 실제 CAM G-code 를 불러와 시뮬레이션 — 슬롯 ap · 헬리컬 이송은 쓰는 공구의 제조사 자료로.
* `[UNKNOWN] shallow_cut_wear_depth` — 헬리컬 진입처럼 얕은 절입의 마모는 마모장의 날끝 칸에만 쌓이지만 작업 안의 VB(수명 판정 · 반경 마모 보정)는
  깊이를 가리지 않고 더하고, 새 날 길들이기도 공구 나이로만 세어 처음 물린 곳(진입이면 날끝)에 다 들어간다. 날 위치별 길들이기 · 코너 마모 배분
  자료가 없어 그대로 두었다 — 인코넬이 첫 단계 진입·첫 줄에서 수명 판정에 닿는 것(정지 정책 12 %)도 진입 마모를 다 넣은 판정이다.
  확인: 진입이 긴 작업 하나 뒤 공구 현미경으로 날끝(코너)과 띠 중간 VB 를 따로 측정.

## 5. 출처

Kennametal Innovations 2020|02 · 2021 (HARVI I TE 슬롯·램핑·헬리컬 표, KenFeed), Kennametal B174 · B162, Kennametal 카탈로그 M443~M444 ·
Seco Solid End Mills GT16-501 (2016) pp. 336~346, Seco Jabro 2009 측면 정삭 표, Seco TechTalk · IMCO Technical Resources pp. 105, 130~132 ·
Kyocera SGS 2021 카탈로그 pp. 22~24, Z-Carb 브로슈어 (2022), 알루미늄 브로슈어 (2018) · Haas Tooling HTPM · HSAM2 · HM42 · YG-1 Alu-Power HPC 2023 ·
OSG 카탈로그 pp. 1338~1339, 1406~1407, 1435, 1508~1509 · Helical HSV-3 · HSV-4 · H45AL-C-3 · HVTI-6 · Harvey Performance (tool entry, ramping, thin-wall,
climb vs conventional) · Guhring Uni-Pro p. 302 · Mitsubishi Alimaster B264B · VQT5MVRB B230G · GARANT Diabolo 70 · NS Tool MHRH430R · MRBH230 ·
Amana CVD 차트 · Sandvik Coromant (ramping, holes/pockets, shoulder milling, inside corners, down vs up milling, CoroMill Plura 복합재) · Iscar 밀링 가이드 ·
Walter (CTE 2006, MD133) · Tungaloy 헬리컬 팁 · Haimer F1004NN · F1005LL · Autodesk Fusion · FeatureCAM · PTC Creo · Vectric · Onshape · Protolabs ·
Protolabs Network (Hubs) · Xometry · SendCutSend · RapidDirect · Boeing 8:1 (MMS) · Mechanik 2017 · Zawada-Michałowska 2025.
리서치 원문 (값·쪽수·URL): 세션 공유 노트 `pocket-path-research/` (research_slot_ap.md · research_entry.md · research_finish.md).
