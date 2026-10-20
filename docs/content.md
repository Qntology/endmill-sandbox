시계열 모델과 결정 모델의 유스케이스 정도로 가볍게 읽어주시면 감사드립니다

---

금속 절삭 회전 공구 '엔드밀'을 가상 환경에서 절차에 따라 검증하는 가설입니다.

1. 공구·소재: 공구 모양·코팅과 절삭 소재를 골라 조합 계산
2. 절삭 조건: 회전수와 이송 속도를 물리 공식으로 계산
3. 1차 예측: 가공 경로를 따라 힘·열·마모를 한 걸음씩 계산
4. 2차 보정: 공구 교체 시점과, 치수를 맞추려고 한 번 절삭 여부를 제안

■ 프로젝트 내용 · □ 개인 생각

---

### 시작하면서

□ 시작은 사실적인 실제 민간 항공사에서 사용하는 항공 시뮬레이터 같이 현장의 절삭 시뮬레이션의 가능성으로 시작하였으며, 현장의 거리감을 보완하기 위해, 공구·소재 간의 N:N 매칭 기반의 테스트 자료 및 논문에 대입하여 현실적인 안정적인 범위 값 도출 시도하였습니다.

■ 다음 절차를 하나로 이어서 돌리는 도구가 없었습니다. 특히 "한 번 깎고 → 공구가 얼마나 닳았나 보고 → 한 번 더 깎아서 치수를 맞출지 말지 결정하고 → 필요하면 공구를 바꾸고 다시 깎는" 이 과정을 자동으로 이어주는 건 없었습니다.

1. 깎는 순서를 짜주는 프로그램이 있습니다. 공구가 어디로 지나갈지 경로는 만들어주는데, 공구가 닳는 건 신경 안 씁니다. 깎고 나면 그걸로 끝입니다.
2. 힘이나 온도를 정밀하게 계산해주는 해석 도구가 있습니다. 정확은 한데, 하나에 수억 원이고 계산 한 번 돌리는 데 몇 시간이 걸립니다. 한 번 쓰고 마는 거라, 깎을 때마다 다시 돌릴 수가 없습니다.
3. 공구에 센서를 붙여서 깎는 도중에 이상이 생기면 알려주는 방식이 있습니다. 근데 이건 이미 깎고 있는 중에야 아는 겁니다. 닳아서 치수가 틀어진 다음에야 경보가 울립니다.

■ 1 µm 는 1 mm 의 1,000분의 1 이고, 머리카락 굵기가 70 µm 정도입니다.)

---

### 시행착오

□ 엔드밀 브랜드를 저작권 이슈로 참조할 수 없어서 커스텀 소재 프리셋으로 우회하여 엔드밀 반영하였습니다

![엔드밀 커스텀 프리셋 이미지](https://media.licdn.com/dms/image/v2/D5612AQF5XaFWaOAbHA/article-inline_image-shrink_1000_1488/B56aFO1jejJwAI-/0/1792324024994?e=1793836800&v=beta&t=dPudZHosgRCUOmmGB5JRCA-TIm9SWIGHkREFREcLQP0)

□ 공구 8종과 소재 8종을 서로 짝지은 64가지 조합(N:N 매칭)을 만들고, 조합마다 나온 숫자를 테스트 기록과 논문·측정 자료에 대조하여 현장 데이터가 어긋나는 곳을 찾고 예외 케이스 발생시 자료를 더 찾아서 테스트를 하나씩 늘리는 식으로 메웠습니다.

**1. 소재가 달라도 같은 숫자를 쓰던 문제**

▲ 처음에는 칼날(공구)과 소재 사이의 마찰, 처음 닳는 양 같은 값이 소재가 바뀌어도 거의 같았습니다. 그래서 단단한 금형강을 깎을 때 날끝 온도가 745 °C 로 나왔는데, 실제로 재 본 자료는 350~600 °C 였습니다. 알루미늄은 반대로 공구 수명이 실측보다 몇 배 길게 나왔습니다.

■ 현장에서는 소재마다 공구와 속도를 다르게 고릅니다. 무른 알루미늄은 칼날에 들러붙고, 단단한 금형강은 날끝을 빨리 상하게 합니다. 그래서 소재별로 실제 측정 자료를 모아 기준값을 따로 만들고, 코팅 같은 공구 차이는 작은 보정으로만 얹었습니다. 금형강은 542 °C 로 측정 범위 안에 들어왔고, 알루미늄 공구 수명도 실측 범위로 맞췄습니다.

**2. 공구 흔들림을 두 배로 읽던 문제**

▲ 공구를 기계에 물리면 아무리 잘 물려도 수 µm 쯤 흔들립니다. 측정기에 나오는 흔들림 값은 실제로 중심이 어긋난 양의 두 배인데, 이 값을 그대로 쓰는 바람에 순간 힘이 평균의 4배 넘게 튀었습니다.

■ 현장에서는 공구를 물린 뒤 흔들림을 재고, 크면 다시 물립니다. 측정값과 실제 어긋남의 관계를 바로잡자 날 여러 개 중 하나가 일을 더 하는 현상까지 계산됐고, 실제 마무리 가공면이 이론보다 19~38배 거칠다는 측정 결과도 같은 원리로 설명됐습니다. 같은 제품, 같은 조건이라도 공구 한 자루마다 수명이 12~19 % 다르다는 실측도 넣어, '공구를 바꾸기 전까지 몇 개 더 깎을 수 있나'를 넉넉하게 잡게 했습니다.

**3. 다 닳은 공구로 끝까지 깎던 문제**

▲ 테스트 기록을 보면, 내열 합금(인코넬)을 거칠게 깎는 작업에서 공구가 수명 한계에 닿은 뒤에도 2분 넘게 더 깎았고, 칼날에 걸리는 힘은 처음의 32배(252 N → 8,064 N)까지 올랐습니다. 금형강을 깎는 가는 볼 공구는 한계를 넘기고도 26분을 더 깎았습니다.

■ 현장이라면 그 전에 멈춥니다. 기계가 부하가 커진 것을 보고 세우거나(부하 감시), 정해진 시간을 다 쓴 공구는 예비 공구로 바꿉니다(공구 수명 관리). 그런데 시뮬레이터에는 '멈춤'도 '부러짐'도 없었습니다. 기계 회사 매뉴얼·특허와 국제 표준(ISO 8688-2)의 마모 기준으로 멈추고 바꾸는 방식 4가지를 만들고, 다 닳아 갈수록 더 빨리 닳는 구간과 힘이 공구 강도를 넘으면 부러지는 판단을 넣었습니다.

■ 이제 같은 인코넬 작업은 날이 상할 무렵 멈추거나 새 공구로 바꿔 끝까지 깎고, 가는 볼 공구로 무리하게 파 들어가면 4.4초 만에 부러지는 것으로 나옵니다.

![시뮬레이션 그래프 이미지](https://media.licdn.com/dms/image/v2/D5612AQFCwMRktNnX4w/article-inline_image-shrink_400_744/B56aFRv64EG0AM-/0/1792372879198?e=1793836800&v=beta&t=TPVhUX5HxIACe6_1DlqC2TMedYe4gGwoiLTY7aukT30)

**4. 어떤 공구든 어떤 소재에나 잘 맞는다고 나온 문제**

▲ 64가지 조합을 처음 돌렸을 때, 알루미늄용 공구를 다른 소재에 대도 꽤 잘 맞는다고 나왔습니다. 칼날 모양(날 각도, 날끝 둥글기)을 입력하지 않은 공구는 깎는 소재에 맞는 모양을 자동으로 받았기 때문입니다. 소재를 바꿀 때마다 그 소재 전용 공구로 바꿔 끼운 셈이었습니다.

■ 현장에서는 알루미늄용, 금형강용 공구를 따로 씁니다. 그래서 공구는 원래 만든 용도의 모양으로 고정하고, 공구 회사 카탈로그와 논문에서 소재마다 맞는 날 각도·날 수 범위를 정리해 범위를 벗어나면 빨리 닳거나 이가 빠지게 했습니다. 판정이 너무 엄하거나 순위가 이상하게 나오는 것도 논문 사례에 맞춰 몇 번 고쳤고, 지금은 각 소재용으로 만든 공구가 그 소재에서 1~2위로 나옵니다.

**5. 감시하는 쪽이 같은 공구로 더 깎던 문제**

▲ 날 이빠짐은 확률로 계산합니다. 그런데 같은 공구로 같은 작업을 할 때, 감시 없이 깎으면 작업의 36 % 를 마친 지점에서 이가 빠져 멈추는데, 감시 장치를 켠 설정에서는 같은 공구로 52 % 까지 더 깎는 모순이 나왔습니다.

■ 감시 장치가 있다고 공구가 튼튼해지지는 않습니다. 감시는 문제를 빨리 알아챌 뿐입니다. 그래서 공구가 겪는 위험은 똑같이 두고, 감시가 있으면 이가 빠질 무렵 그 자리에서 멈추거나 공구를 바꾸게 했습니다. '감시하는 쪽이 감시 없는 쪽보다 더 멀리 깎으면 안 된다'는 조건은 테스트로 남겼습니다.

**6. 새 공구인데 경보가 울리던 문제**

▲ 현장의 부하 감시는 새 공구일 때보다 부하가 1.2배쯤 커지면 경보를 울립니다. 이것을 흉내 냈더니, 공구가 소재에 들어가고 나오는 짧은 순간마다 값이 튀어 거의 새 공구인데도 경보가 울렸습니다.

■ 실제 장치처럼 0.5초 동안의 평균으로 보게 했습니다. 반대로 아주 얇게 깎는 마무리 가공은 공구가 닳아도 부하가 별로 늘지 않아 감시 장치가 못 잡는 경우가 있어, 그런 조건은 따로 표시합니다.

**7. 닳은 양을 치수로 바꿀 때 날 각도 하나를 빠뜨린 문제**

▲ 날끝이 닳은 양을 '벽이 얼마나 덜 깎이나'로 바꾸는 계산에서 날 각도 하나를 빠뜨려, 실제보다 3~7 % 작게 읽었습니다. µm 단위로 치수를 맞추는 정밀 가공에서는 불량이 갈리는 차이입니다.

■ 현장에서는 공구 지름을 재서 닳은 만큼 보정값을 넣습니다. 이 보정이 맞으려면 계산식부터 맞아야 해서, 교과서에 나오는 식(Kronenberg)으로 바로잡고 벽 오차·보정 가공·공구 기록을 모두 같은 식으로 맞췄습니다.

---

### 벤치마크

□ 64가지 조합 대조는 샌드박스가 낸 숫자를 자료의 범위에 대어 보는 방식이었습니다. 이번에는 거꾸로, 참조 논문이 실제로 깎은 공구·소재·절삭 조건을 그대로 샌드박스의 공구·소재 데이터로 옮겨 계산하고, 그 논문이 잰 값과 나란히 놓았습니다.

■ 참조 논문 9편의 공구 13종과 샌드박스 공구 8종을, 논문의 소재·조건 16가지(저탄소강·티타늄 합금·알루미늄 합금·합금강·금형강)에 모두 대어 336가지 조합(N:N)을 계산하고, 그중 논문에 실측이 있는 31건을 비교했습니다. 숫자를 그대로 비교한 것(공구 수명·날끝 온도·힘·표면 거칠기) 18건, 같은 논문 안에서 코팅이나 조건만 바꾼 두 결과의 비 6건, '속도를 올리면 수명이 얼마나 주는가' 같은 기울기 7건입니다. 공구 수명의 절대값은 날끝 상태처럼 논문에 적히지 않은 값에 크게 흔들리지만, 비와 기울기에서는 그런 값이 약분되어 물리 법칙 자체를 시험할 수 있어 따로 봤습니다.

■ 원칙은 하나였습니다. 조합마다 숫자를 맞추는 보정 계수를 넣지 않고, 어긋난 원인을 교과서·논문의 물리에서 찾아 식을 고쳤습니다. 하나를 맞추면 다른 조합이 무너지는 수정은 되돌렸습니다. 논문 수치는 초록과 본문 일부에서 읽었고(작업 환경에서 논문 원문 사이트 접속이 막혀 검색 결과로 대조), 논문에 적히지 않은 공구 모양·조건은 가정으로 두고 그 비교의 확신도를 낮췄습니다.

**1. 속도를 올리면 날끝 온도가 내려가던 문제**

▲ 샌드박스의 금형강 정삭 프리셋은 속도를 올리면 날끝 온도가 오히려 내려가고 공구 수명이 조금 늘었습니다. 같은 고경도강(H13, 52 HRC)을 볼 공구로 깎으며 온도를 잰 논문은 속도와 함께 온도가 오릅니다. 원인을 따라가 보니 칩이 떨어져 나가는 면(전단면)의 온도가 1,100~1,400 °C, 거의 소재가 녹는 온도로 계산되고 있었습니다. 아주 얇게 깎을 때 날끝이 소재를 밀어 내는 데 드는 힘까지 전단면의 열로 셈한 탓이었습니다.

■ 전단면의 열은 소재가 실제로 미끄러지며 변형하는 데 드는 일만으로 계산하도록 고전 절삭 이론(Oxley)의 방식으로 바꿨습니다. 소재는 뜨거울수록 물러지고, 물러지면 열이 덜 나므로 둘이 맞아떨어지는 온도를 찾습니다. 금형강 프리셋의 전단면은 1,125 °C 에서 391 °C 가 되었고, 날끝 온도는 속도를 15 % 올리면 539 °C 에서 558 °C 로 오르게 되었습니다(보완 전에는 542 °C 에서 519 °C 로 내려갔습니다).

**2. 속도를 올려도 공구 수명이 덜 줄던 문제**

▲ 코팅 없는 초경 공구로 저탄소강을 깎은 논문에서는 속도를 2.5배 올리면 수명이 1/7 로 줄었는데, 샌드박스는 1/5 정도만 줄었습니다. 공구가 닳는 원인 네 가지(긁힘·들러붙음·확산·산화) 가운데 확산만 온도에 민감하게 계산하고 있었습니다.

■ 공구 마모 예측의 고전식(Usui)은 들러붙었다 뜯겨 나가는 마모도 온도가 오를수록 빨라진다고 봅니다. 철·티타늄·니켈 합금에서는 들러붙음 마모도 같은 온도 법칙을 따르게 했더니, 이 논문들의 속도–수명 기울기 3건이 모두 실측 범위 안에 들어왔습니다(실측 −2.15 · −2.80 · −2.69, 샌드박스 −2.04 · −2.39 · −2.18).

**3. 날 뒷면 각도가 큰 공구를 약한 날로 보던 문제**

▲ 같은 논문의 인서트 공구(날 앞면 각도 0°, 뒷면 각도 15°)는 샌드박스 수명이 실측의 1/14~1/6 이었습니다. 뒷면 각도(여유각)만 보고 '날이 얇아 이가 빠진다'고 판정해 마모를 1.8배로 매기고 있었습니다.

■ 날이 얼마나 튼튼한지는 앞면과 뒷면 사이에 남은 쐐기 각도가 정합니다. 이 인서트의 쐐기는 75°로, 강을 깎는 보통 엔드밀(74°)과 같습니다. 그래서 쐐기 각도로 판정하게 바꾸고, 뒷면 각도가 크면 같은 깊이만큼 닳아도 닳은 띠(VB)가 덜 넓어지는 기하(Kronenberg 식, 시행착오 7번과 같은 식)도 넣었습니다.

**4. 날 앞면 각도의 효과를 두 배로 읽던 문제**

▲ 같은 조건에서 앞면 각도 0° 공구가 10° 공구보다 절삭 저항이 21 % 크고 날끝이 93 °C 뜨겁게 나왔습니다. 쓰던 이론식(Merchant)은 각도 1° 에 저항을 약 1.9 % 바꾸는데, 공구 회사의 밀링 계산식은 1° 에 1 % 입니다.

■ 코팅에 따른 마찰 차이는 이론식으로 두고, 공구 각도가 소재 기본값에서 벗어난 몫만 공구 회사 계산식으로 계산하게 했습니다. 3번과 함께, 실측의 1/14~1/6 이던 인서트 수명이 1/3~3/5 수준까지 왔고 5건 중 1건이 실측 범위 안에 들어왔습니다. 남은 차이의 상당 부분은 샌드박스 소재 목록에 저탄소강(AISI 1018)이 없어 중탄소강(C45)으로 계산한 몫입니다.

**5. 시도했지만 넣지 않은 것**

▲ 티타늄 합금은 속도를 올릴 때 수명이 주는 정도가 실측(기울기 −3.3~−4)보다 약합니다(−2.4~−2.5). 티타늄 마모의 온도 민감도를 두 배 가까이 올리면 기울기는 실측과 맞았지만, 같은 논문의 다른 조건 수명이 실측의 1/10 로 무너지고, 냉각만 내부 급유로 바꿨을 때의 수명 차이가 11~48배로 커졌습니다. 현장에서 냉각으로 얻는 수명 차이는 몇 배 수준입니다.

■ 기울기 하나를 맞추려고 다른 조합이 무너지는 것은 물리를 고친 것이 아니라 숫자를 데이터에 맞춘 것이라 되돌렸습니다. 닳은 띠에서 나는 마찰열의 분배(Loewen·Shaw)도 알루미늄의 코팅 효과는 나아졌지만 합금강 조합과 기존 시험 하나가 나빠져 넣지 않았습니다.

■ 그 결과 실측 범위 안에 든 비교가 31건 중 13건에서 15건으로 늘었고, 공구 수명 절대값의 오차(중앙값)는 8.6배에서 3.0배로 줄었습니다. 기존 시험 181개는 모두 그대로 통과하고, 이 결과를 지키는 시험 3개를 더했습니다.

| 비교 (논문) | 실측 | 보완 전 | 보완 후 |
|---|---|---|---|
| 저탄소강 인서트 수명, Vc 89.8 · 224.4 · 374 m/min (Karandikar) | 255 · 35.5 · 8.5 분 | 18.0 · 3.47 · 0.99 분 | 77.5 · 12.0 · 3.53 분 |
| 저탄소강 속도–수명 기울기 3건 (Karandikar) | −2.15 · −2.80 · −2.69 | −1.80 · −2.46 · −1.92 | −2.04 · −2.39 · −2.18 |
| 금형강 볼 날끝 온도의 속도 기울기 (Dewes) | +0.4 (범위 0.1~0.8) | +0.11 | +0.26 |
| 티타늄 슬롯 평균 힘, fz 0.05 · 0.10 mm (Li) | 212 · 321 N | 202 · 298 N | 200 · 284 N |
| 티타늄 속도–수명 기울기 (Tatar) | −4.0 | −2.22 | −2.44 |
| 알루미늄 DLC 코팅 수명 이득 (Grigoriev) | 3.45배 | 1.04배 | 1.04배 |

■ 고치지 않고 남긴 차이도 기록했습니다. 티타늄의 속도 민감도, 알루미늄을 아주 얇게 깎을 때의 수명(실측의 약 10배)과 DLC 코팅 이득, 합금강을 아주 얇은 칩으로 다듬을 때의 수명(약 4배), 절삭 저항이 속도에 따라 줄어드는 정도(실측 기울기 −0.18, 샌드박스 −0.01), 고경도강 수명의 속도 민감도입니다. 논문 원문의 조건을 더 확인하거나 실측이 더 있어야 원인을 가를 수 있어 '측정 없이는 알 수 없는 항목'으로 남겼습니다. 시행착오 1번에서 542 °C 이던 금형강 날끝 온도는 이번 보완 뒤에도 540 °C 로 측정 범위(350~600 °C) 안에 있습니다.

**6. 2차 보완 — 논문 원문으로 다시 대조하고 비교를 73건으로**

□ 1차에서는 작업 환경이 논문 원문 사이트를 막아 검색 결과에 보이는 문장으로 수치를 맞춰 봤습니다. 2차에서는 조사를 여러 갈래로 나눠 논문 전문과 표를 직접 읽었고, 그러자 1차에 가정했던 공구가 실제와 다른 경우가 꽤 나왔습니다(티타늄 논문의 공구 지름 10 → 12 mm, 다른 논문의 지름 16 → 20 mm, 알루미늄 논문의 공구 재질이 분말 고속도강이었다는 것 등).

■ 공구 30종과 샌드박스 공구 8종을 논문 조건 43가지에 모두 대어 1,634가지 조합을 계산하고, 실측이 있는 73건을 비교했습니다. 새로 넣은 것은 스테인리스(316L·SUS304), 인코넬 718, 고경도강 정삭의 마모 진행 데이터(PHM 2010 공개 데이터), 같은 공구로 냉각만 바꾼 시험(건식·미스트·플러드), 그리고 '속도를 올리면 절삭 저항이 얼마나 주는가' 같은 기울기들입니다.

▲ 저탄소강(AISI 1018)을 중탄소강(C45)으로 계산하던 몫. 샌드박스 소재 목록에 저탄소강을 새로 넣었습니다. 다만 수명의 기준값을 새로 만들지 않고, 같은 철·탄화철·초경 조합이라 마모 법칙의 상수는 C45 와 같다고 두었습니다. 두 강의 수명 차이는 소재가 무른 정도와 단단한 탄화물 양이 만들게 했습니다. 실측의 1/3~3/5 이던 1018 수명이 1/2~1 수준으로 올라왔고, 아래의 이중 계산까지 고치자 7건이 모두 실측 범위 안에 들어왔습니다.

▲ 티타늄은 속도를 올릴 때 수명이 주는 정도가 고속에서는 너무 약하고(Tatar 실측 −4.0, 샌드박스 −2.4) 저온 플러드에서는 너무 강했습니다(Luo 실측 −1.1, 샌드박스 −3.0). 공구 마모가 온도에 얼마나 민감한지를 강에서 빌린 구조로 두고 있었는데, 티타늄 마모를 직접 분석한 연구(ETH 2025)는 높은 온도에서 훨씬 민감한 값(약 16,000 K)을, 같은 연구가 정리한 문헌 범위의 아래 끝은 낮은 온도의 둔한 값(2,500 K)을 줍니다. 두 영역을 가르는 온도(550 °C)만은 기울기 자료에서 정한 값입니다. 이렇게 나눴더니 고속 쪽 기울기 3건(Tatar · Liu · Matuszak)이 모두 실측 범위 안에 들고, 값을 정할 때 쓰지 않은 다른 논문(Mohruni)의 기울기도 −2.40 에서 −3.76 (실측 −3.95)으로 맞았습니다. 저온 플러드 쪽(Luo)은 −3.0 에서 −2.5 로 나아졌지만 아직 범위 밖입니다.

▲ 계산 방법 자체의 오류도 하나 찾았습니다. 공구 마모 곡선을 그릴 때 걸음을 너무 크게 잡아, 수명이 긴 조건에서는 '새 공구가 처음 몇 분 빨리 닳는 구간'이 한 걸음에 몇 배로 쌓였습니다. 고경도강 데이터가 실측과 맞아 보이던 것은 이 오류 덕이었고, 고치고 나니 실제로는 마모를 약 5배 느리게 보고 있다는 것이 드러났습니다. 맞던 숫자가 틀려졌지만, 틀린 이유로 맞던 것을 드러낸 것이라 그대로 기록했습니다.

▲ 공구 모양이 소재에 맞는지 보는 규칙이 '날 앞면 각도가 작으면 마모 증가'를 따로 곱하고 있었는데, 그 효과는 이미 절삭 저항과 날끝 온도 계산에 들어 있었습니다. 같은 몫을 두 번 센 것이라 뺐습니다.

■ 수명의 기준값(공구 하나가 기준 조건에서 몇 분 가는가)도 의심했습니다. 티타늄 칸이 고르게 실측보다 짧았기 때문입니다. 하지만 기준값을 비교할 칸에 맞춰 올리면 그 칸들은 더 이상 시험이 아니게 됩니다. 그래서 비교에 쓰지 않은 다른 논문들에서 기준 조건 근처의 실측을 따로 찾았고, 기준 조건으로 옮겨 보면 티타늄 20~60 분 · 인코넬 30~60 분 · 스테인리스 30~90 분 정도로, 지금 기준값(35 · 25 · 45 분)이 그 안에 있거나(티타늄 · 스테인리스) 조금 보수적인 아래 끝(인코넬)이었습니다. 기준값은 그대로 두고, 그 실측들은 새 비교 칸으로 넣었습니다. 티타늄의 차이는 기준값이 아니라 기준 조건에서 멀리 간 조건(아주 빠른 속도, 아주 얇은 반경 절입)에서 난다는 것을 확인한 셈입니다.

■ 저탄소강을 넣은 상태를 기준으로, 티타늄 두 영역 · 적분 오류 · 이중 계산을 고친 뒤 같은 73건 가운데 실측 범위 안에 든 비교가 27건에서 33건으로 늘었고, 수명 기울기가 실측과 벌어진 정도(중앙값)는 0.85 에서 0.43 으로 줄었습니다. 기존 시험 184개는 모두 그대로 통과합니다.

| 비교 (논문) | 실측 | 저탄소강까지 넣은 뒤 | 최종 |
|---|---|---|---|
| 저탄소강 인서트 수명, Vc 89.8 · 224.4 · 374 m/min (Karandikar) | 255 · 35.5 · 8.5 분 | 123 · 17.3 · 5.19 분 | 144 · 18.9 · 5.64 분 |
| 티타늄 속도–수명 기울기 (Tatar · Liu · Matuszak) | −4.0 · −3.5 · −3.3 | −2.43 · −1.65 · −2.45 | −3.92 · −2.85 · −4.05 |
| 티타늄 속도–수명 기울기, 값을 정할 때 쓰지 않은 논문 (Mohruni) | −3.95 | −2.40 | −3.76 |
| 티타늄 저온 플러드 기울기 (Luo) | −1.13 | −3.01 | −2.53 (남음) |
| 티타늄 기준 조건 근처 수명 (Gross · Denkena · Shokrani) | 16.8 · 33 · 3.3 분 | 4.48 · 19.2 · 1.90 분 | 8.68 · 25.6 · 2.65 분 |
| 고경도강 정삭 마모, 컷 315 의 VB (PHM 2010) | 0.206 mm | 0.189 mm (적분 오류로 맞음) | 0.049 mm (남음) |
| 인코넬 플러드/건식 수명 비 (Grguraš) | 2.61 | 1.74 | 1.75 |
| 인코넬 무코팅 · 얇은 반경 절입 수명 (Hronek · Vavruska) | 32.4 · 46 분 | 1.52 · 0.69 분 | 1.49 · 0.67 분 (남음) |
| 스테인리스 플러드/건식 수명 비 (Grguraš) | 14.5 이상 | 1.13 | 1.14 (남음) |
| 알루미늄 DLC/무코팅 수명 비 (Grigoriev) | 3.45 | 1.57 | 1.63 (남음) |

□ 남은 차이도 이유와 함께 기록했습니다. 반경 절입이 아주 얇을 때 수명이 실측보다 훨씬 짧은 것은 '한 번 물리는 시간이 짧으면 날끝이 덜 뜨거워지는' 효과가 빠진 몫으로 보이는데, 계산해 보면 넣는 순간 다른 기울기가 틀어져 열 계산 전체를 실측으로 다시 맞춰야 합니다. 스테인리스를 건식으로 깎으면 습식의 1/14 에도 못 미치는 수명(실측), 알루미늄에서 DLC 코팅의 이득, 무코팅 공구로 인코넬을 얇게 깎을 때의 수명도 원인 후보는 찾았지만 값을 정할 실측이 부족해 넣지 않고 '측정이 더 있어야 알 수 있는 항목'으로 남겼습니다.

■ 비교 73건 전체 표와 계산 방법은 저장소의 `docs/literature-benchmark.md` (8장이 2차 보완), 조사 기록은 `docs/literature-benchmark-sources.md` 에 정리했습니다.

---

### 마무리하면서

□ AI로 개발하였으며 진짜 많은 문서 참조하였고, 테스트 한것 같다는 생각하였습니다. 이전에 산업 문서 전처리에서는 합성 데이터로 검증 가능하였으나 현장의 정밀 가공 데이터가 정밀도 영향에 크기 때문에 이번에 처음으로 많은 참조·대조 경험이 가장 새로웠습니다.

□ 추후에 공구·소재를 구하는게 가능하다면 저속 회전 중인 공구를 촬영하여 실제 마모와 시뮬레이션 결과값과 대조하여 개선해보고 싶습니다. 아직 검증되지 않은 시뮬레이션이라 현장 경험이 있으신 분들께서 피드백주시면 정말 큰 도움이 될것 같습니다. 

■ **참조/공구 마모·수명·파손**
- Usui, E., Shirakashi, T., Kitagawa, T. (1984). Analytical prediction of cutting tool wear. *Wear*, 100(1–3), 129–151. [https://doi.org/10.1016/0043-1648(84)90010-3](https://doi.org/10.1016/0043-1648(84)90010-3)
- Hovsepian, P.Eh., Luo, Q., Robinson, G., et al. (2006). TiAlN/VN superlattice structured PVD coatings: A new alternative in machining of aluminium alloys for aerospace and automotive components. *Surface and Coatings Technology*, 201(1–2), 265–272. [https://doi.org/10.1016/j.surfcoat.2005.11.106](https://doi.org/10.1016/j.surfcoat.2005.11.106)
- Karandikar, J.M., Abbas, A.E., Schmitz, T.L. (2013). Tool life prediction using random walk Bayesian updating. *Machining Science and Technology*, 17(3), 410–442. [https://doi.org/10.1080/10910344.2013.806103](https://doi.org/10.1080/10910344.2013.806103)
- Martin, B., Morrow, J., Heaney, P., et al. (2013). Observation of Tool Life of Micro End Mills. *8th International Conference on MicroManufacturing (ICOMM 2013)*, Victoria, BC, Paper No. 116. [https://minds.wisconsin.edu/handle/1793/65296](https://minds.wisconsin.edu/handle/1793/65296)
- Wei, C., Song, X., Fu, J., et al. (2013). Simultaneously high fracture toughness and transverse rupture strength in ultrafine cemented carbide. *CrystEngComm*, 15(17), 3305. [https://doi.org/10.1039/C3CE40211H](https://doi.org/10.1039/C3CE40211H)
- Karandikar, J.M., Abbas, A.E., Schmitz, T.L. (2014). Tool life prediction using Bayesian updating. Part 1: Milling tool life model using a discrete grid method. *Precision Engineering*, 38(1), 9–17. [https://mtrc.utk.edu/wp-content/uploads/sites/45/2019/09/tool_life_bayesian_part_1.pdf](https://mtrc.utk.edu/wp-content/uploads/sites/45/2019/09/tool_life_bayesian_part_1.pdf)
- Kuczmaszewski, J., Zaleski, K., Matuszak, J., et al. (2017). Wpływ średnicy frezu na jego zużycie podczas obróbki stopu tytanu Ti6Al4V (The influence of tool diameter on wear during milling of titanium alloy Ti6Al4V). *Mechanik*, 3/2017, 198–200. [https://doi.org/10.17814/mechanik.2017.3.40](https://doi.org/10.17814/mechanik.2017.3.40)
- Sánchez-Hechavarría, Y., Mariño-Cala, M., Brito-Sauvanell, L.Á., et al. (2017). Supervisión del desgaste y deterioro de flancos en fresado extremo de alta velocidad en superficies inclinadas de acero endurecido AISI D6. *Minería y Geología*, 33(1), 58–75. [https://www.redalyc.org/articulo.oa?id=223549947005](https://www.redalyc.org/articulo.oa?id=223549947005)
- Uddin, M.S., Pham, B., Sarhan, A., et al. (2017). Comparative study between wear of uncoated and TiAlN-coated carbide tools in milling of Ti6Al4V. *Advances in Manufacturing*, 5(1), 83–91. [https://doi.org/10.1007/s40436-016-0166-1](https://doi.org/10.1007/s40436-016-0166-1)
- Liu, D., Zhang, Y., Luo, M., et al. (2019). Investigation of Tool Wear and Chip Morphology in Dry Trochoidal Milling of Titanium Alloy Ti–6Al–4V. *Materials*, 12(12), 1937. [https://doi.org/10.3390/ma12121937](https://doi.org/10.3390/ma12121937)
- Grigoriev, S.N., Volosova, M.A., Fedorov, S.V., et al. (2020). Influence of DLC Coatings Deposited by PECVD Technology on the Wear Resistance of Carbide End Mills and Surface Roughness of AlCuMg2 and 41Cr4 Workpieces. *Coatings*, 10(11), 1038. [https://doi.org/10.3390/coatings10111038](https://doi.org/10.3390/coatings10111038)
- Tatar, K., Sjöberg, S., Andersson, N. (2020). Investigation of cutting conditions on tool life in shoulder milling of Ti6Al4V using PVD coated micro-grain carbide insert based on design of experiments. *Heliyon*, 6(6), e04217. [https://doi.org/10.1016/j.heliyon.2020.e04217](https://doi.org/10.1016/j.heliyon.2020.e04217)
- Karandikar, J., Schmitz, T., Smith, S. (2021). Logistic classification for tool life modeling in machining. *Procedia CIRP*, 101, 106–109. [https://doi.org/10.1016/j.procir.2021.03.125](https://doi.org/10.1016/j.procir.2021.03.125)
- Matuszak, J., Zaleski, K., Zyśko, A. (2023). Investigation of the Impact of High-Speed Machining in the Milling Process of Titanium Alloy on Tool Wear, Surface Layer Properties, and Fatigue Life of the Machined Object. *Materials*, 16(15), 5361. [https://doi.org/10.3390/ma16155361](https://doi.org/10.3390/ma16155361)

■ **참조/공구 교체·감시 전략**
- Kelly, K., O'Donnell, G.E. (2007). Decision Making under Uncertainty in Tool Condition Monitoring. *24th International Manufacturing Conference*, Waterford, Ireland, 965–976. [https://www.tara.tcd.ie/bitstreams/2195963a-5d41-418e-8ff4-3717a1cba118/download](https://www.tara.tcd.ie/bitstreams/2195963a-5d41-418e-8ff4-3717a1cba118/download)
- Colantonio, L., Moroncini, G., Sénéchal, O., et al. (2025). Cost and Environmental Assessment of Tool Replacement Strategies Under Imperfect Wear Monitoring in Ti6Al4V Milling. *International Journal of Prognostics and Health Management*, 16(4). [https://doi.org/10.36001/IJPHM.2025.v16i4.4269](https://doi.org/10.36001/IJPHM.2025.v16i4.4269)

■ **참조/표면 무결성·표면 거칠기**
- Wojciechowski, S. (2011). Machined surface roughness including cutter displacements in milling of hardened steel. *Metrology and Measurement Systems*, 18(3), 429–440. [https://doi.org/10.2478/v10178-011-0009-7](https://doi.org/10.2478/v10178-011-0009-7)
- Wojciechowski, S., Twardowski, P., Pelic, M., et al. (2016). Precision surface characterization for finish cylindrical milling with dynamic tool displacements model. *Precision Engineering*, 46, 158–165. [https://doi.org/10.1016/j.precisioneng.2016.04.010](https://doi.org/10.1016/j.precisioneng.2016.04.010)
- Dai, X., Zhuang, K., Pu, D., et al. (2020). An Investigation of the Work Hardening Behavior in Interrupted Cutting Inconel 718 under Cryogenic Conditions. *Materials*, 13(9), 2202. [https://doi.org/10.3390/ma13092202](https://doi.org/10.3390/ma13092202)
- Abbas, A.T., Al Bahkali, E.A., Alqahtani, S.M., et al. (2021). Fundamental Investigation into Tool Wear and Surface Quality in High-Speed Machining of Ti6Al4V Alloy. *Materials*, 14(23), 7128. [https://doi.org/10.3390/ma14237128](https://doi.org/10.3390/ma14237128)
- Tan, L., Yao, C., Zhang, D., et al. (2021). Effects of tool wear on machined surface integrity during milling of Inconel 718. *The International Journal of Advanced Manufacturing Technology*, 116(7–8), 2497–2509. [https://doi.org/10.1007/s00170-021-07626-x](https://doi.org/10.1007/s00170-021-07626-x)
- Mičietová, A., Čilliková, M., Čep, R., et al. (2023). Study of Residual Stresses and Austenite Gradients in the Surface after Hard Turning as a Function of Flank Wear and Cutting Speed. *Materials*, 16(4), 1709. [https://doi.org/10.3390/ma16041709](https://doi.org/10.3390/ma16041709)
- Nowakowski, L., Blasiak, S., Skrzyniarz, M. (2023). Influence of the relative displacements and the minimum chip thickness on the surface texture in shoulder milling. *Materials*, 16(24), 7661. [https://doi.org/10.3390/ma16247661](https://doi.org/10.3390/ma16247661)
- Wojciechowski, S., Krajewska-Śpiewak, J., Maruda, R.W., et al. (2023). Study on ploughing phenomena in tool flank face – workpiece interface including tool wear effect during ball-end milling. *Tribology International*, 181, 108313. [https://doi.org/10.1016/j.triboint.2023.108313](https://doi.org/10.1016/j.triboint.2023.108313)

■ **참조/절삭력 계수·칩·마찰**
- Merchant, M.E. (1945). Mechanics of the Metal Cutting Process. I. Orthogonal Cutting and a Type 2 Chip. *Journal of Applied Physics*, 16(5), 267–275. [https://doi.org/10.1063/1.1707586](https://doi.org/10.1063/1.1707586)
- Kienzle, O. (1952). Die Bestimmung von Kräften und Leistungen an spanenden Werkzeugen und Werkzeugmaschinen. *VDI-Z*, 94(11/12), 299–305.
- Budak, E., Altintas, Y., Armarego, E.J.A. (1996). Prediction of milling force coefficients from orthogonal cutting data. *Journal of Manufacturing Science and Engineering*, 118(2), 216–224. [https://doi.org/10.1115/1.2831014](https://doi.org/10.1115/1.2831014)
- Gradišek, J., Kalveram, M., Weinert, K. (2004). Mechanistic identification of specific force coefficients for a general end mill. *International Journal of Machine Tools and Manufacture*, 44(4), 401–414. [https://doi.org/10.1016/j.ijmachtools.2003.10.001](https://doi.org/10.1016/j.ijmachtools.2003.10.001)
- Marcicano, J.P.P., Almeida, L.O., Machado, I.F., Hassui, A. (2005). Determination of the Mechanical Behavior of AISI 304 austenitic stainless steel in Machining Processes. *18th International Congress of Mechanical Engineering (COBEM 2005)*, Ouro Preto, MG, COBEM2005-1123. [https://abcm.org.br/app/webroot/anais/cobem/2005/PDF/COBEM2005-1123.pdf](https://abcm.org.br/app/webroot/anais/cobem/2005/PDF/COBEM2005-1123.pdf)
- Li, X., Zhao, W., Li, L., et al. (2014). Modeling, identification and application of process damping in milling of titanium alloy. *Journal of Vibroengineering*, 16(6), 2657–2675. [https://doi.org/10.21595/jve.2014.15081](https://doi.org/10.21595/jve.2014.15081)
- Rubeo, M.A., Schmitz, T.L. (2016). Mechanistic force model coefficients: A comparison of linear regression and nonlinear optimization. *Precision Engineering*, 45, 311–321. [https://doi.org/10.1016/j.precisioneng.2016.03.008](https://doi.org/10.1016/j.precisioneng.2016.03.008)
- Zhan, G., He, L., Jiang, H., et al. (2017). Research on Stress and Strain of 304 Stainless Steel in Turning Process Based on Oxley-Welsh Theory. *Proceedings of ICMEIM 2017*, Advances in Engineering Research 100, 444–449. [https://doi.org/10.2991/icmeim-17.2017.75](https://doi.org/10.2991/icmeim-17.2017.75)
- Gao, S., Duan, X., Zhu, K., et al. (2022). Generic Cutting Force Modeling with Comprehensively Considering Tool Edge Radius, Tool Flank Wear and Tool Runout in Micro-End Milling. *Micromachines*, 13(11), 1805. [https://doi.org/10.3390/mi13111805](https://doi.org/10.3390/mi13111805)
- Klippel, H., Süssmaier, S., Kuffa, M., et al. (2022). Dry Cutting Experiments Database Ti6Al4V and Ck45. arXiv:2209.04197. [https://doi.org/10.48550/arXiv.2209.04197](https://doi.org/10.48550/arXiv.2209.04197) (학술지판: *The International Journal of Advanced Manufacturing Technology*, 135(5–6), 2871–2908, 2024. [https://doi.org/10.1007/s00170-024-14597-2](https://doi.org/10.1007/s00170-024-14597-2))
- Kovalčík, J., Mašek, P., Malý, J., et al. (2022). The Effect of Coatings on Cutting Force in Turning of C45 Steel. *Materials*, 15(2), 590. [https://doi.org/10.3390/ma15020590](https://doi.org/10.3390/ma15020590)
- Schmitz, T., Nazario, J., No, T., et al. (2022). Time-domain simulation and testing for dynamic force and stability prediction in milling: Collaboration with feed rate scheduling software. *IFAC-PapersOnLine*, 55(36), 103–108. [https://doi.org/10.1016/j.ifacol.2022.11.341](https://doi.org/10.1016/j.ifacol.2022.11.341)
- Sun, H., Ding, H., Deng, C., et al. (2023). Efficient Prediction of Stability Boundaries in Milling Considering the Variation of Tool Features and Workpiece Materials. *Sensors*, 23(21), 8954. [https://doi.org/10.3390/s23218954](https://doi.org/10.3390/s23218954)
- Andersson Lassila, A., Svensson, D., Wang, W., et al. (2024). Numerical evaluation of cutting strategies for thin-walled parts. *Scientific Reports*, 14, 1459. [https://doi.org/10.1038/s41598-024-51883-1](https://doi.org/10.1038/s41598-024-51883-1)
- Tymczyszyn, J., Szajna, A., Mrówka-Nowotnik, G. (2025). Influence of Nanocomposite PVD Coating on Cutting Tool Wear During Milling of 316L Stainless Steel Under Air Cooling Conditions. *Materials*, 18(9), 1959. [https://doi.org/10.3390/ma18091959](https://doi.org/10.3390/ma18091959)
- Tian, W., Zhou, J., Ren, J., et al. (2026). Modeling and experimental study of cutting forces of a variable pitch ball-end cutter in five-axis milling. *Scientific Reports*, 16, 6541. [https://doi.org/10.1038/s41598-026-36982-5](https://doi.org/10.1038/s41598-026-36982-5)
- Zou, Z., Xin, Y., Ma, C. (2026). Influence of Cutting-Edge Micro-Geometry on Material Separation and Minimum Cutting Thickness in the Turning of 304 Stainless Steel. *Materials*, 19(3), 591. [https://doi.org/10.3390/ma19030591](https://doi.org/10.3390/ma19030591)

■ **참조/떨림(채터)·동역학·런아웃·공구 휨**
- Kline, W.A., DeVor, R.E. (1983). The effect of runout on cutting geometry and forces in end milling. *International Journal of Machine Tool Design and Research*, 23(2–3), 123–140. [https://doi.org/10.1016/0020-7357(83)90012-4](https://doi.org/10.1016/0020-7357(83)90012-4)
- Kops, L., Vo, D.T. (1990). Determination of the equivalent diameter of an end mill based on its compliance. *CIRP Annals*, 39(1), 93–96. [https://doi.org/10.1016/S0007-8506(07)61010-5](https://doi.org/10.1016/S0007-8506(07)61010-5)
- Altintas, Y., Budak, E. (1995). Analytical prediction of stability lobes in milling. *CIRP Annals*, 44(1), 357–362. [https://doi.org/10.1016/S0007-8506(07)62342-7](https://doi.org/10.1016/S0007-8506(07)62342-7)
- Insperger, T., Stépán, G. (2002). Semi-discretization method for delayed systems. *International Journal for Numerical Methods in Engineering*, 55(5), 503–518. [https://doi.org/10.1002/nme.505](https://doi.org/10.1002/nme.505)
- Insperger, T., Stépán, G. (2004). Updated semi-discretization method for periodic delay-differential equations with discrete delay. *International Journal for Numerical Methods in Engineering*, 61(1), 117–141. [https://doi.org/10.1002/nme.1061](https://doi.org/10.1002/nme.1061)
- Merdol, S.D., Altintas, Y. (2004). Multi frequency solution of chatter stability for low immersion milling. *Journal of Manufacturing Science and Engineering*, 126(3), 459–466. [https://doi.org/10.1115/1.1765139](https://doi.org/10.1115/1.1765139)
- Gradišek, J., Kalveram, M., Insperger, T., et al. (2005). On stability prediction for milling. *International Journal of Machine Tools and Manufacture*, 45(7–8), 769–781. [https://doi.org/10.1016/j.ijmachtools.2004.11.015](https://doi.org/10.1016/j.ijmachtools.2004.11.015)
- Schmitz, T.L., Duncan, G.S. (2005). Three-component receptance coupling substructure analysis for tool point dynamics prediction. *Journal of Manufacturing Science and Engineering*, 127(4), 781–790. [https://doi.org/10.1115/1.2039102](https://doi.org/10.1115/1.2039102)
- Zatarain, M., Muñoa, J., Peigné, G., Insperger, T. (2006). Analysis of the influence of mill helix angle on chatter stability. *CIRP Annals*, 55(1), 365–368. [https://doi.org/10.1016/S0007-8506(07)60436-3](https://doi.org/10.1016/S0007-8506(07)60436-3)
- Schmitz, T.L., Couey, J., Marsh, E., et al. (2007). Runout effects in milling: Surface finish, surface location error, and stability. *International Journal of Machine Tools and Manufacture*, 47(5), 841–851. [https://doi.org/10.1016/j.ijmachtools.2006.06.014](https://doi.org/10.1016/j.ijmachtools.2006.06.014)
- Altintas, Y., Stépán, G., Merdol, D., Dombovari, Z. (2008). Chatter stability of milling in frequency and discrete time domain. *CIRP Journal of Manufacturing Science and Technology*, 1(1), 35–44. [https://doi.org/10.1016/j.cirpj.2008.06.003](https://doi.org/10.1016/j.cirpj.2008.06.003)
- Schmitz, T.L., Ziegert, J.C., Canning, J.S., Zapata, R. (2008). Case study: A comparison of error sources in high-speed milling. *Precision Engineering*, 32(2), 126–133. [https://doi.org/10.1016/j.precisioneng.2007.06.001](https://doi.org/10.1016/j.precisioneng.2007.06.001)
- Budak, E., Tunç, L.T. (2010). Identification and modeling of process damping in turning and milling using a new approach. *CIRP Annals*, 59(1), 403–408. [https://doi.org/10.1016/j.cirp.2010.03.078](https://doi.org/10.1016/j.cirp.2010.03.078)
- Zhang, J., Schmitz, T., Zhao, W., Lu, B. (2011). Receptance coupling for tool point dynamics prediction on machine tools. *Chinese Journal of Mechanical Engineering*, 24(3), 340–345. [https://doi.org/10.3901/CJME.2011.03.340](https://doi.org/10.3901/CJME.2011.03.340)
- Ozturk, E., Kumar, U., Turner, S., Schmitz, T. (2012). Investigation of spindle bearing preload on dynamics and stability limit in milling. *CIRP Annals*, 61(1), 343–346. [https://doi.org/10.1016/j.cirp.2012.03.134](https://doi.org/10.1016/j.cirp.2012.03.134)
- Tyler, C.T., Schmitz, T.L. (2012). Process damping analytical stability analysis and validation. *Transactions of NAMRI/SME*, 40, 396–405. [https://mtrc.utk.edu/wp-content/uploads/sites/45/2019/09/NAMRC40-7758.pdf](https://mtrc.utk.edu/wp-content/uploads/sites/45/2019/09/NAMRC40-7758.pdf)
- Tyler, C.T., Schmitz, T.L. (2014). Process damping milling model database. *Transactions of NAMRI/SME*, 42, 325–334. [https://mtrc.utk.edu/wp-content/uploads/sites/45/2019/09/ProcessDamping_NAMRC_2014_rev1.pdf](https://mtrc.utk.edu/wp-content/uploads/sites/45/2019/09/ProcessDamping_NAMRC_2014_rev1.pdf)
- Honeycutt, A., Schmitz, T.L. (2018). Milling bifurcations: A review of literature and experiment. *Journal of Manufacturing Science and Engineering*, 140(12), 120801. [https://doi.org/10.1115/1.4041325](https://doi.org/10.1115/1.4041325)

■ **참조/절삭 열·냉각·기계 열변위**
- Loewen, E.G., Shaw, M.C. (1954). On the Analysis of Cutting-Tool Temperatures. *Transactions of the ASME*, 76(2), 217–225. [https://doi.org/10.1115/1.4014799](https://doi.org/10.1115/1.4014799)
- Boothroyd, G. (1963). Temperatures in Orthogonal Metal Cutting. *Proceedings of the Institution of Mechanical Engineers*, 177(1), 789–810. [https://doi.org/10.1243/PIME_PROC_1963_177_058_02](https://doi.org/10.1243/PIME_PROC_1963_177_058_02)
- Dewes, R.C., Ng, E., Chua, K.S., et al. (1999). Temperature measurement when high speed machining hardened mould/die steel. *Journal of Materials Processing Technology*, 92–93, 293–301. [https://doi.org/10.1016/S0924-0136(99)00116-8](https://doi.org/10.1016/S0924-0136(99)00116-8)
- Kops, L., Arenson, M. (1999). Determination of Convective Cooling Conditions in Turning. *CIRP Annals*, 48(1), 47–52. [https://doi.org/10.1016/S0007-8506(07)63129-1](https://doi.org/10.1016/S0007-8506(07)63129-1)
- Hosokawa, A., Oda, K., Yamada, K., et al. (2000). 断続切削における工具逃げ面温度 — 高速エンドミル加工に関する研究(第1報) (Temperature of Tool Flank in Intermittent Cutting: Studies on High Speed Endmilling, 1st Report). *精密工学会誌*, 66(11), 1786–1791. [https://doi.org/10.2493/jjspe.66.1786](https://doi.org/10.2493/jjspe.66.1786)
- Rai, J.K., Xirouchakis, P. (2009). FEM-based prediction of workpiece transient temperature distribution and deformations during milling. *The International Journal of Advanced Manufacturing Technology*, 42(5–6), 429–449. [https://doi.org/10.1007/s00170-008-1610-6](https://doi.org/10.1007/s00170-008-1610-6)
- Brandão, L.C., Coelho, R.T., Malavolta, A.T. (2010). Experimental and theoretical study on workpiece temperature when tapping hardened AISI H13 using different cooling systems. *Journal of the Brazilian Society of Mechanical Sciences and Engineering*, 32(2), 154–159. [https://doi.org/10.1590/S1678-58782010000200008](https://doi.org/10.1590/S1678-58782010000200008)
- Li, Y., Yu, M., Bai, Y., et al. (2021). A Review of Thermal Error Modeling Methods for Machine Tools. *Applied Sciences*, 11(11), 5216. [https://doi.org/10.3390/app11115216](https://doi.org/10.3390/app11115216)
- Shokrani, A., Betts, J., Carnevale, M. (2021). Thermal analysis in MQL end milling operations. *Procedia CIRP*, 101, 358–361. [https://doi.org/10.1016/j.procir.2020.07.011](https://doi.org/10.1016/j.procir.2020.07.011)
- Uhlmann, E., Polte, M., Triebel, F., et al. (2022). Reduction of warm-up period after machine downtime by means of a thermoelectric tempered motorized milling spindle. *euspen Special Interest Group Meeting: Thermal Issues*, ETH Zurich, TI22105. [https://euspen.eu/knowledge-base/TI22105.pdf](https://euspen.eu/knowledge-base/TI22105.pdf)
- Stegmann, J., Baumert, M., Kabelac, S., et al. (2024). Assessment of the Heat Transfer Conditions in the Cavity of a Rotating Circular Saw. *Energies*, 17(13), 3189. [https://doi.org/10.3390/en17133189](https://doi.org/10.3390/en17133189)

■ **참조/재료 모델·입자 계산(MPM)**
- Taylor, G.I., Quinney, H. (1934). The latent energy remaining in a metal after cold working. *Proceedings of the Royal Society of London A*, 143(849), 307–326. [https://doi.org/10.1098/rspa.1934.0004](https://doi.org/10.1098/rspa.1934.0004)
- Johnson, G.R., Cook, W.H. (1983). A constitutive model and data for metals subjected to large strains, high strain rates and high temperatures. *Proceedings of the 7th International Symposium on Ballistics*, The Hague, 541–547.
- Jiang, C., Schroeder, C., Selle, A., et al. (2015). The affine particle-in-cell method. *ACM Transactions on Graphics*, 34(4), 51. [https://doi.org/10.1145/2766996](https://doi.org/10.1145/2766996)
- Hu, Y., Fang, Y., Ge, Z., et al. (2018). A moving least squares material point method with displacement discontinuity and two-way rigid body coupling. *ACM Transactions on Graphics*, 37(4), 150. [https://doi.org/10.1145/3197517.3201293](https://doi.org/10.1145/3197517.3201293)

■ **참조/교과서·고전**
- Taylor, F.W. (1907). On the Art of Cutting Metals. *Transactions of the ASME*, 28, 31–279. [https://doi.org/10.1115/1.4060388](https://doi.org/10.1115/1.4060388)
- Kronenberg, M. (1966). *Machining Science and Application: Theory and Practice for Operation and Development of Machining Processes*. Pergamon Press.
- Oxley, P.L.B. (1989). *The Mechanics of Machining: An Analytical Approach to Assessing Machinability*. Ellis Horwood.
- Trent, E.M., Wright, P.K. (2000). *Metal Cutting*, 4th ed. Butterworth-Heinemann.
- Boothroyd, G., Knight, W.A. (2006). *Fundamentals of Machining and Machine Tools*, 3rd ed. CRC Press.
- Altintas, Y. (2012). *Manufacturing Automation: Metal Cutting Mechanics, Machine Tool Vibrations, and CNC Design*, 2nd ed. Cambridge University Press. [https://doi.org/10.1017/CBO9780511843723](https://doi.org/10.1017/CBO9780511843723)

■ **참조/표준**
- ISO 8688-2:1989. Tool life testing in milling — Part 2: End milling. [https://www.iso.org/standard/16092.html](https://www.iso.org/standard/16092.html)
- ISO 3685:1993. Tool-life testing with single-point turning tools. [https://www.iso.org/standard/9151.html](https://www.iso.org/standard/9151.html)
- ISO 230-3:2020. Test code for machine tools — Part 3: Determination of thermal effects. [https://www.iso.org/standard/73291.html](https://www.iso.org/standard/73291.html)

■ **참조/특허**
- Norberg, R., Svensson, T. (Santrade Ltd) (1986). Programmable multi-channel tool monitor with multiple alarm limits and sensor taring. US 4,633,424 A. [https://patents.google.com/patent/US4633424A/en](https://patents.google.com/patent/US4633424A/en)
- Beyer, P.J., Murphy, T.F., Heckel, D.W. (AMCA International Corp.) (1988). Programmed path for automatic tool retraction and return responsive to degradation threshold. US 4,723,219 A. [https://patents.google.com/patent/US4723219A/en](https://patents.google.com/patent/US4723219A/en)
- Tanaka, K., Onishi, Y., Sato, K. (Fanuc Ltd) (1997). Method of monitoring machining load using variable alarm threshold. US 5,631,851 A. [https://patents.google.com/patent/US5631851A/en](https://patents.google.com/patent/US5631851A/en)

■ **참조/제조사·기술 자료**
- FANUC. Series 30i/31i/32i-MODEL A Tool Life Management Specifications (A-79409E).
- Siemens. SINUMERIK 840D 기능 설명서 — 공구 감시(공구 수명·대체 공구).
- Caron Engineering. TMAC (Tool Monitoring Adaptive Control). [https://caroneng.com/products/tmac](https://caroneng.com/products/tmac)
- Haas Automation. 밀링 설정 84 (Tool Overload Action).
- 공구 카탈로그·기술 자료: Harvey Tool / Harvey Performance, OSG, Kennametal, Mitsubishi Materials, Sandvik Coromant (절삭 조건·비절삭저항 표).
- 코팅 자료: Oerlikon Balzers, Platit.
- 측정·공구 홀더 자료: Renishaw NC4, Blum LaserControl, Haimer, Rego-Fix, BIG KAISER.
