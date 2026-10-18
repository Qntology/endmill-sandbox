# 엔드밀 샌드박스 개발 배경과 문제 해결 과정 정리해봤습니다

엔드밀 샌드박스는 CNC 기계에서 금속을 깎는 회전 공구 '엔드밀'이 어떤 조건에서 얼마나 힘을 받고, 뜨거워지고, 닳는지를 컴퓨터 안에서 미리 돌려 보는 가상 가공 시뮬레이터입니다.

- 공구·소재: 공구 모양·코팅과 절삭 소재를 골라 조합 계산
- 절삭 조건: 회전수와 이송 속도를 물리 공식으로 계산
- 1차 예측: 가공 경로를 따라 힘·열·마모를 한 걸음씩 계산
- 2차 보정: 공구 교체 시점과, 치수를 맞추려고 한 번 절삭 여부를 제안

■ 프로젝트 내용 · □ 개인 생각

---

## 시작하면서

■ 현장에서는 '언제 공구를 바꿀지, 한 번 더 절삭하여 치수를 맞출지' 수작업으로 판단 합니다. 너무 일찍 바꾸면 공구값이 낭비되고, 늦으면 제품이 망가집니다. 실제로 시험하려면 공구·소재·기계 시간·측정 장비가 필요하고, 공구·소재·냉각·조건등 조합마다 결과가 다릅니다.

■ 정밀 가공에서 가장 무서운 건 공구가 조금씩 닳는 것입니다. 날이 닳으면 같은 프로그램으로 절삭하면 벽 치수가 수 µm씩 틀어지고, 표면이 거칠어지고, 열과 떨림이 커집니다. 금형이나 정밀 부품은 이 몇 µm 차이로 불량이 되고, 심하면 엔드밀이 부러져 소재까지 망칩니다.

■ 엔드밀 샌드박스는 절삭 전에 가상으로 돌려 마모가 언제 한계에 닿는지, 치수가 얼마나 틀어지는지 미리 확인해보는 도구입니다.

□ 시작은 사실적인 실제 민간 항공사에서 사용하는 항공 시뮬레이터 같이 현장의 절삭 시뮬레이션의 가능성으로 시작하였으며, 현장의 거리감을 보완하기 위해, 공구·소재 간의 N:N 매칭 기반의 테스트 자료 및 논문에 대입하여 현실적인 안정적인 범위 값 도출 시도하였습니다.

---

## 시행착오

■ 공구 8종과 소재 8종을 서로 짝지은 64가지 조합(N:N 매칭)을 만들고, 조합마다 나온 숫자를 시험 기록과 논문·측정 자료에 하나씩 대어 봤습니다. 대어 볼 때마다 현장과 어긋나는 곳이 나왔고, 그때마다 원인을 찾고, 자료를 더 찾고, 다시 틀어지지 않도록 시험을 하나씩 늘리는 식으로 메웠습니다.

■ 아래에서 ▲는 시험 자료·논문과 대조했을 때 잘 안된 부분입니다. (1 µm 는 1 mm 의 1,000분의 1 이고, 머리카락 굵기가 70 µm 정도입니다.)

### 1. 소재가 달라도 같은 숫자를 쓰던 문제

▲ 처음에는 칼날(공구)과 소재 사이의 마찰, 처음 닳는 양 같은 값이 소재가 바뀌어도 거의 같았습니다. 그래서 단단한 금형강을 깎을 때 날끝 온도가 745 °C 로 나왔는데, 실제로 재 본 자료는 350~600 °C 였습니다. 알루미늄은 반대로 공구 수명이 실측보다 몇 배 길게 나왔습니다.

■ 현장에서는 소재마다 공구와 속도를 다르게 고릅니다. 무른 알루미늄은 칼날에 들러붙고, 단단한 금형강은 날끝을 빨리 상하게 합니다. 그래서 소재별로 실제 측정 자료를 모아 기준값을 따로 만들고, 코팅 같은 공구 차이는 작은 보정으로만 얹었습니다. 금형강은 542 °C 로 측정 범위 안에 들어왔고, 알루미늄 공구 수명도 실측 범위로 맞췄습니다.

### 2. 공구 흔들림을 두 배로 읽던 문제

▲ 공구를 기계에 물리면 아무리 잘 물려도 수 µm 쯤 흔들립니다. 측정기에 나오는 흔들림 값은 실제로 중심이 어긋난 양의 두 배인데, 이 값을 그대로 쓰는 바람에 순간 힘이 평균의 4배 넘게 튀었습니다.

■ 현장에서는 공구를 물린 뒤 흔들림을 재고, 크면 다시 물립니다. 측정값과 실제 어긋남의 관계를 바로잡자 날 여러 개 중 하나가 일을 더 하는 현상까지 계산됐고, 실제 마무리 가공면이 이론보다 19~38배 거칠다는 측정 결과도 같은 원리로 설명됐습니다. 같은 제품, 같은 조건이라도 공구 한 자루마다 수명이 12~19 % 다르다는 실측도 넣어, '공구를 바꾸기 전까지 몇 개 더 깎을 수 있나'를 넉넉하게 잡게 했습니다.

### 3. 다 닳은 공구로 끝까지 깎던 문제

▲ 시험 기록을 보면, 내열 합금(인코넬)을 거칠게 깎는 작업에서 공구가 수명 한계에 닿은 뒤에도 2분 넘게 더 깎았고, 칼날에 걸리는 힘은 처음의 32배(252 N → 8,064 N)까지 올랐습니다. 금형강을 깎는 가는 볼 공구는 한계를 넘기고도 26분을 더 깎았습니다.

■ 현장이라면 그 전에 멈춥니다. 기계가 부하가 커진 것을 보고 세우거나(부하 감시), 정해진 시간을 다 쓴 공구는 예비 공구로 바꿉니다(공구 수명 관리). 그런데 시뮬레이터에는 '멈춤'도 '부러짐'도 없었습니다. 기계 회사 매뉴얼·특허와 국제 표준(ISO 8688-2)의 마모 기준으로 멈추고 바꾸는 방식 4가지를 만들고, 다 닳아 갈수록 더 빨리 닳는 구간과 힘이 공구 강도를 넘으면 부러지는 판단을 넣었습니다.

■ 이제 같은 인코넬 작업은 날이 상할 무렵 멈추거나 새 공구로 바꿔 끝까지 깎고, 가는 볼 공구로 무리하게 파 들어가면 4.4초 만에 부러지는 것으로 나옵니다.

### 4. 어떤 공구든 어떤 소재에나 잘 맞는다고 나온 문제

▲ 64가지 조합을 처음 돌렸을 때, 알루미늄용 공구를 다른 소재에 대도 꽤 잘 맞는다고 나왔습니다. 칼날 모양(날 각도, 날끝 둥글기)을 입력하지 않은 공구는 깎는 소재에 맞는 모양을 자동으로 받았기 때문입니다. 소재를 바꿀 때마다 그 소재 전용 공구로 바꿔 끼운 셈이었습니다.

■ 현장에서는 알루미늄용, 금형강용 공구를 따로 씁니다. 그래서 공구는 원래 만든 용도의 모양으로 고정하고, 공구 회사 카탈로그와 논문에서 소재마다 맞는 날 각도·날 수 범위를 정리해 범위를 벗어나면 빨리 닳거나 이가 빠지게 했습니다. 판정이 너무 엄하거나 순위가 이상하게 나오는 것도 논문 사례에 맞춰 몇 번 고쳤고, 지금은 각 소재용으로 만든 공구가 그 소재에서 1~2위로 나옵니다.

### 5. 감시하는 쪽이 같은 공구로 더 깎던 문제

▲ 날 이빠짐은 확률로 계산합니다. 그런데 같은 공구로 같은 작업을 할 때, 감시 없이 깎으면 작업의 36 % 를 마친 지점에서 이가 빠져 멈추는데, 감시 장치를 켠 설정에서는 같은 공구로 52 % 까지 더 깎는 모순이 나왔습니다.

■ 감시 장치가 있다고 공구가 튼튼해지지는 않습니다. 감시는 문제를 빨리 알아챌 뿐입니다. 그래서 공구가 겪는 위험은 똑같이 두고, 감시가 있으면 이가 빠질 무렵 그 자리에서 멈추거나 공구를 바꾸게 했습니다. '감시하는 쪽이 감시 없는 쪽보다 더 멀리 깎으면 안 된다'는 조건은 시험으로 남겼습니다.

### 6. 새 공구인데 경보가 울리던 문제

▲ 현장의 부하 감시는 새 공구일 때보다 부하가 1.2배쯤 커지면 경보를 울립니다. 이것을 흉내 냈더니, 공구가 소재에 들어가고 나오는 짧은 순간마다 값이 튀어 거의 새 공구인데도 경보가 울렸습니다.

■ 실제 장치처럼 0.5초 동안의 평균으로 보게 했습니다. 반대로 아주 얇게 깎는 마무리 가공은 공구가 닳아도 부하가 별로 늘지 않아 감시 장치가 못 잡는 경우가 있어, 그런 조건은 따로 표시합니다.

### 7. 닳은 양을 치수로 바꿀 때 날 각도 하나를 빠뜨린 문제

▲ 날끝이 닳은 양을 '벽이 얼마나 덜 깎이나'로 바꾸는 계산에서 날 각도 하나를 빠뜨려, 실제보다 3~7 % 작게 읽었습니다. µm 단위로 치수를 맞추는 정밀 가공에서는 불량이 갈리는 차이입니다.

■ 현장에서는 공구 지름을 재서 닳은 만큼 보정값을 넣습니다. 이 보정이 맞으려면 계산식부터 맞아야 해서, 교과서에 나오는 식(Kronenberg)으로 바로잡고 벽 오차·보정 가공·공구 기록을 모두 같은 식으로 맞췄습니다.

### 8. 시험이 시험대로 돌지 않던 문제

▲ 시험에도 시행착오가 있었습니다. 메모리 7 GB 컴퓨터에서 시험 프로그램을 한꺼번에 만들다가 메모리가 모자라 빌드가 깨졌고, 결과 요약은 64가지 조합 중 63가지만 세고 있었습니다.

■ 한 번에 만드는 개수를 줄이고 디버그 정보를 빼는 옵션을 넣었고, 요약이 빠뜨린 줄도 고쳤습니다. 지금 test_cuda.bat 은 13단계, 시험 161개를 돌리고, 결과마다 어디가 의심스러운지까지 요약에 남깁니다.

### 아직 잘 안된 부분

▲ 자료끼리 말이 다른 곳: 티타늄을 티타늄 계열 코팅 공구로 깎으면 서로 들러붙어 나쁘다고 보고 경고를 넣었는데, 같은 계열 코팅(TiAlN) 공구가 오히려 코팅 없는 공구보다 수명이 1.44배 길었다는 자료도 있습니다. 알루미늄용 공구의 TiAlN 계열 코팅도 마찬가지로, 한 연구 안에서도 코팅과 알루미늄 종류에 따라 DLC 코팅보다 두 배 오래가기도 하고 절반도 못 가기도 했습니다. 그래서 지금은 '주의'로만 표시해 두었습니다.

▲ 직접 재 봐야 아는 값: 공구 끝을 작은 망치로 톡 쳐서 떨림 특성을 재는 시험(탭 테스트) 없이 추정값을 쓰다 보니 떨림 위험이 크게 잡혀, 금형강 마무리 가공은 정상 작업에서도 이빠짐 확률이 84 % 로 나옵니다. 이 밖에도 측정 없이는 정할 수 없는 값 14가지를 작업마다 '어떤 값을 썼고, 어느 범위일 수 있고, 무엇을 재면 되는지'로 기록합니다. 같은 조건에서 공구 10자루 이상을 실제로 써 본 기록이 가장 먼저 필요합니다.

□ 논문 값 하나를 그대로 넣으면 조합이 바뀔 때마다 어긋났습니다. 값마다 범위와 확신도를 같이 적고, 틀린 곳이 나올 때마다 다시 틀어지지 않게 시험을 하나씩 늘려 간 것이 현장과의 거리를 줄이는 가장 확실한 방법이었습니다.

---

## 마무리하면서


---

참조

본문과 시뮬레이터 계산에서 대조한 논문·교과서·표준·특허·제조사 자료입니다. 분야별로 나누고, 분야 안에서는 연도순으로 적었습니다.

**프로젝트 시험 자료**

- test_cuda.bat 실행 기록 — 2026-10-18 13:08 (보완 전), 15:09 (보완 후): 단계별 로그, summary.log, highlights.log
- 마모 로그 — wear_logs 의 작업별 CSV·JSON (공구 수명 감시, 감사 기록, [UNKNOWN] 항목)
- 공구 × 소재 행렬 결과 — matrix/pair_matrix_rect_profile.csv·json (64칸)
- 정리 문서 — docs/tool-life-and-pair-matrix.md (모델·근거 표·확인 못 한 항목)

**공구 마모·수명·파손**

- Usui, E., Shirakashi, T., Kitagawa, T. (1984). Analytical prediction of cutting tool wear. *Wear*, 100(1–3), 129–151. https://doi.org/10.1016/0043-1648(84)90010-3
- Hovsepian, P.Eh., Luo, Q., Robinson, G., et al. (2006). TiAlN/VN superlattice structured PVD coatings: A new alternative in machining of aluminium alloys for aerospace and automotive components. *Surface and Coatings Technology*, 201(1–2), 265–272. https://doi.org/10.1016/j.surfcoat.2005.11.106
- Karandikar, J.M., Abbas, A.E., Schmitz, T.L. (2013). Tool life prediction using random walk Bayesian updating. *Machining Science and Technology*, 17(3), 410–442. https://doi.org/10.1080/10910344.2013.806103
- Martin, B., Morrow, J., Heaney, P., et al. (2013). Observation of Tool Life of Micro End Mills. *8th International Conference on MicroManufacturing (ICOMM 2013)*, Victoria, BC, Paper No. 116. https://minds.wisconsin.edu/handle/1793/65296
- Wei, C., Song, X., Fu, J., et al. (2013). Simultaneously high fracture toughness and transverse rupture strength in ultrafine cemented carbide. *CrystEngComm*, 15(17), 3305. https://doi.org/10.1039/C3CE40211H
- Kuczmaszewski, J., Zaleski, K., Matuszak, J., et al. (2017). Wpływ średnicy frezu na jego zużycie podczas obróbki stopu tytanu Ti6Al4V (The influence of tool diameter on wear during milling of titanium alloy Ti6Al4V). *Mechanik*, 3/2017, 198–200. https://doi.org/10.17814/mechanik.2017.3.40
- Sánchez-Hechavarría, Y., Mariño-Cala, M., Brito-Sauvanell, L.Á., et al. (2017). Supervisión del desgaste y deterioro de flancos en fresado extremo de alta velocidad en superficies inclinadas de acero endurecido AISI D6. *Minería y Geología*, 33(1), 58–75. https://www.redalyc.org/articulo.oa?id=223549947005
- Uddin, M.S., Pham, B., Sarhan, A., et al. (2017). Comparative study between wear of uncoated and TiAlN-coated carbide tools in milling of Ti6Al4V. *Advances in Manufacturing*, 5(1), 83–91. https://doi.org/10.1007/s40436-016-0166-1
- Liu, D., Zhang, Y., Luo, M., et al. (2019). Investigation of Tool Wear and Chip Morphology in Dry Trochoidal Milling of Titanium Alloy Ti–6Al–4V. *Materials*, 12(12), 1937. https://doi.org/10.3390/ma12121937
- Grigoriev, S.N., Volosova, M.A., Fedorov, S.V., et al. (2020). Influence of DLC Coatings Deposited by PECVD Technology on the Wear Resistance of Carbide End Mills and Surface Roughness of AlCuMg2 and 41Cr4 Workpieces. *Coatings*, 10(11), 1038. https://doi.org/10.3390/coatings10111038
- Tatar, K., Sjöberg, S., Andersson, N. (2020). Investigation of cutting conditions on tool life in shoulder milling of Ti6Al4V using PVD coated micro-grain carbide insert based on design of experiments. *Heliyon*, 6(6), e04217. https://doi.org/10.1016/j.heliyon.2020.e04217
- Karandikar, J., Schmitz, T., Smith, S. (2021). Logistic classification for tool life modeling in machining. *Procedia CIRP*, 101, 106–109. https://doi.org/10.1016/j.procir.2021.03.125
- Matuszak, J., Zaleski, K., Zyśko, A. (2023). Investigation of the Impact of High-Speed Machining in the Milling Process of Titanium Alloy on Tool Wear, Surface Layer Properties, and Fatigue Life of the Machined Object. *Materials*, 16(15), 5361. https://doi.org/10.3390/ma16155361

**공구 교체·감시 전략**

- Kelly, K., O'Donnell, G.E. (2007). Decision Making under Uncertainty in Tool Condition Monitoring. *24th International Manufacturing Conference*, Waterford, Ireland, 965–976. https://www.tara.tcd.ie/bitstreams/2195963a-5d41-418e-8ff4-3717a1cba118/download
- Colantonio, L., Moroncini, G., Sénéchal, O., et al. (2025). Cost and Environmental Assessment of Tool Replacement Strategies Under Imperfect Wear Monitoring in Ti6Al4V Milling. *International Journal of Prognostics and Health Management*, 16(4). https://doi.org/10.36001/IJPHM.2025.v16i4.4269

**표면 무결성·표면 거칠기**

- Wojciechowski, S. (2011). Machined surface roughness including cutter displacements in milling of hardened steel. *Metrology and Measurement Systems*, 18(3), 429–440. https://doi.org/10.2478/v10178-011-0009-7
- Wojciechowski, S., Twardowski, P., Pelic, M., et al. (2016). Precision surface characterization for finish cylindrical milling with dynamic tool displacements model. *Precision Engineering*, 46, 158–165. https://doi.org/10.1016/j.precisioneng.2016.04.010
- Dai, X., Zhuang, K., Pu, D., et al. (2020). An Investigation of the Work Hardening Behavior in Interrupted Cutting Inconel 718 under Cryogenic Conditions. *Materials*, 13(9), 2202. https://doi.org/10.3390/ma13092202
- Abbas, A.T., Al Bahkali, E.A., Alqahtani, S.M., et al. (2021). Fundamental Investigation into Tool Wear and Surface Quality in High-Speed Machining of Ti6Al4V Alloy. *Materials*, 14(23), 7128. https://doi.org/10.3390/ma14237128
- Tan, L., Yao, C., Zhang, D., et al. (2021). Effects of tool wear on machined surface integrity during milling of Inconel 718. *The International Journal of Advanced Manufacturing Technology*, 116(7–8), 2497–2509. https://doi.org/10.1007/s00170-021-07626-x
- Mičietová, A., Čilliková, M., Čep, R., et al. (2023). Study of Residual Stresses and Austenite Gradients in the Surface after Hard Turning as a Function of Flank Wear and Cutting Speed. *Materials*, 16(4), 1709. https://doi.org/10.3390/ma16041709
- Nowakowski, L., Blasiak, S., Skrzyniarz, M. (2023). Influence of the relative displacements and the minimum chip thickness on the surface texture in shoulder milling. *Materials*, 16(24), 7661. https://doi.org/10.3390/ma16247661
- Wojciechowski, S., Krajewska-Śpiewak, J., Maruda, R.W., et al. (2023). Study on ploughing phenomena in tool flank face – workpiece interface including tool wear effect during ball-end milling. *Tribology International*, 181, 108313. https://doi.org/10.1016/j.triboint.2023.108313

**절삭력 계수·칩·마찰**

- Merchant, M.E. (1945). Mechanics of the Metal Cutting Process. I. Orthogonal Cutting and a Type 2 Chip. *Journal of Applied Physics*, 16(5), 267–275. https://doi.org/10.1063/1.1707586
- Kienzle, O. (1952). Die Bestimmung von Kräften und Leistungen an spanenden Werkzeugen und Werkzeugmaschinen. *VDI-Z*, 94(11/12), 299–305.
- Budak, E., Altintas, Y., Armarego, E.J.A. (1996). Prediction of milling force coefficients from orthogonal cutting data. *Journal of Manufacturing Science and Engineering*, 118(2), 216–224. https://doi.org/10.1115/1.2831014
- Gradišek, J., Kalveram, M., Weinert, K. (2004). Mechanistic identification of specific force coefficients for a general end mill. *International Journal of Machine Tools and Manufacture*, 44(4), 401–414. https://doi.org/10.1016/j.ijmachtools.2003.10.001
- Marcicano, J.P.P., Almeida, L.O., Machado, I.F., Hassui, A. (2005). Determination of the Mechanical Behavior of AISI 304 austenitic stainless steel in Machining Processes. *18th International Congress of Mechanical Engineering (COBEM 2005)*, Ouro Preto, MG, COBEM2005-1123. https://abcm.org.br/app/webroot/anais/cobem/2005/PDF/COBEM2005-1123.pdf
- Li, X., Zhao, W., Li, L., et al. (2014). Modeling, identification and application of process damping in milling of titanium alloy. *Journal of Vibroengineering*, 16(6), 2657–2675. https://doi.org/10.21595/jve.2014.15081
- Rubeo, M.A., Schmitz, T.L. (2016). Mechanistic force model coefficients: A comparison of linear regression and nonlinear optimization. *Precision Engineering*, 45, 311–321. https://doi.org/10.1016/j.precisioneng.2016.03.008
- Zhan, G., He, L., Jiang, H., et al. (2017). Research on Stress and Strain of 304 Stainless Steel in Turning Process Based on Oxley-Welsh Theory. *Proceedings of ICMEIM 2017*, Advances in Engineering Research 100, 444–449. https://doi.org/10.2991/icmeim-17.2017.75
- Gao, S., Duan, X., Zhu, K., et al. (2022). Generic Cutting Force Modeling with Comprehensively Considering Tool Edge Radius, Tool Flank Wear and Tool Runout in Micro-End Milling. *Micromachines*, 13(11), 1805. https://doi.org/10.3390/mi13111805
- Klippel, H., Süssmaier, S., Kuffa, M., et al. (2022). Dry Cutting Experiments Database Ti6Al4V and Ck45. arXiv:2209.04197. https://doi.org/10.48550/arXiv.2209.04197 (학술지판: *The International Journal of Advanced Manufacturing Technology*, 135(5–6), 2871–2908, 2024. https://doi.org/10.1007/s00170-024-14597-2)
- Kovalčík, J., Mašek, P., Malý, J., et al. (2022). The Effect of Coatings on Cutting Force in Turning of C45 Steel. *Materials*, 15(2), 590. https://doi.org/10.3390/ma15020590
- Schmitz, T., Nazario, J., No, T., et al. (2022). Time-domain simulation and testing for dynamic force and stability prediction in milling: Collaboration with feed rate scheduling software. *IFAC-PapersOnLine*, 55(36), 103–108. https://doi.org/10.1016/j.ifacol.2022.11.341
- Sun, H., Ding, H., Deng, C., et al. (2023). Efficient Prediction of Stability Boundaries in Milling Considering the Variation of Tool Features and Workpiece Materials. *Sensors*, 23(21), 8954. https://doi.org/10.3390/s23218954
- Andersson Lassila, A., Svensson, D., Wang, W., et al. (2024). Numerical evaluation of cutting strategies for thin-walled parts. *Scientific Reports*, 14, 1459. https://doi.org/10.1038/s41598-024-51883-1
- Tymczyszyn, J., Szajna, A., Mrówka-Nowotnik, G. (2025). Influence of Nanocomposite PVD Coating on Cutting Tool Wear During Milling of 316L Stainless Steel Under Air Cooling Conditions. *Materials*, 18(9), 1959. https://doi.org/10.3390/ma18091959
- Tian, W., Zhou, J., Ren, J., et al. (2026). Modeling and experimental study of cutting forces of a variable pitch ball-end cutter in five-axis milling. *Scientific Reports*, 16, 6541. https://doi.org/10.1038/s41598-026-36982-5
- Zou, Z., Xin, Y., Ma, C. (2026). Influence of Cutting-Edge Micro-Geometry on Material Separation and Minimum Cutting Thickness in the Turning of 304 Stainless Steel. *Materials*, 19(3), 591. https://doi.org/10.3390/ma19030591

**떨림(채터)·동역학·런아웃·공구 휨**

- Kline, W.A., DeVor, R.E. (1983). The effect of runout on cutting geometry and forces in end milling. *International Journal of Machine Tool Design and Research*, 23(2–3), 123–140. https://doi.org/10.1016/0020-7357(83)90012-4
- Kops, L., Vo, D.T. (1990). Determination of the equivalent diameter of an end mill based on its compliance. *CIRP Annals*, 39(1), 93–96. https://doi.org/10.1016/S0007-8506(07)61010-5
- Altintas, Y., Budak, E. (1995). Analytical prediction of stability lobes in milling. *CIRP Annals*, 44(1), 357–362. https://doi.org/10.1016/S0007-8506(07)62342-7
- Insperger, T., Stépán, G. (2002). Semi-discretization method for delayed systems. *International Journal for Numerical Methods in Engineering*, 55(5), 503–518. https://doi.org/10.1002/nme.505
- Insperger, T., Stépán, G. (2004). Updated semi-discretization method for periodic delay-differential equations with discrete delay. *International Journal for Numerical Methods in Engineering*, 61(1), 117–141. https://doi.org/10.1002/nme.1061
- Merdol, S.D., Altintas, Y. (2004). Multi frequency solution of chatter stability for low immersion milling. *Journal of Manufacturing Science and Engineering*, 126(3), 459–466. https://doi.org/10.1115/1.1765139
- Gradišek, J., Kalveram, M., Insperger, T., et al. (2005). On stability prediction for milling. *International Journal of Machine Tools and Manufacture*, 45(7–8), 769–781. https://doi.org/10.1016/j.ijmachtools.2004.11.015
- Schmitz, T.L., Duncan, G.S. (2005). Three-component receptance coupling substructure analysis for tool point dynamics prediction. *Journal of Manufacturing Science and Engineering*, 127(4), 781–790. https://doi.org/10.1115/1.2039102
- Zatarain, M., Muñoa, J., Peigné, G., Insperger, T. (2006). Analysis of the influence of mill helix angle on chatter stability. *CIRP Annals*, 55(1), 365–368. https://doi.org/10.1016/S0007-8506(07)60436-3
- Schmitz, T.L., Couey, J., Marsh, E., et al. (2007). Runout effects in milling: Surface finish, surface location error, and stability. *International Journal of Machine Tools and Manufacture*, 47(5), 841–851. https://doi.org/10.1016/j.ijmachtools.2006.06.014
- Altintas, Y., Stépán, G., Merdol, D., Dombovari, Z. (2008). Chatter stability of milling in frequency and discrete time domain. *CIRP Journal of Manufacturing Science and Technology*, 1(1), 35–44. https://doi.org/10.1016/j.cirpj.2008.06.003
- Schmitz, T.L., Ziegert, J.C., Canning, J.S., Zapata, R. (2008). Case study: A comparison of error sources in high-speed milling. *Precision Engineering*, 32(2), 126–133. https://doi.org/10.1016/j.precisioneng.2007.06.001
- Budak, E., Tunç, L.T. (2010). Identification and modeling of process damping in turning and milling using a new approach. *CIRP Annals*, 59(1), 403–408. https://doi.org/10.1016/j.cirp.2010.03.078
- Zhang, J., Schmitz, T., Zhao, W., Lu, B. (2011). Receptance coupling for tool point dynamics prediction on machine tools. *Chinese Journal of Mechanical Engineering*, 24(3), 340–345. https://doi.org/10.3901/CJME.2011.03.340
- Ozturk, E., Kumar, U., Turner, S., Schmitz, T. (2012). Investigation of spindle bearing preload on dynamics and stability limit in milling. *CIRP Annals*, 61(1), 343–346. https://doi.org/10.1016/j.cirp.2012.03.134
- Tyler, C.T., Schmitz, T.L. (2012). Process damping analytical stability analysis and validation. *Transactions of NAMRI/SME*, 40, 396–405. https://mtrc.utk.edu/wp-content/uploads/sites/45/2019/09/NAMRC40-7758.pdf
- Tyler, C.T., Schmitz, T.L. (2014). Process damping milling model database. *Transactions of NAMRI/SME*, 42, 325–334. https://mtrc.utk.edu/wp-content/uploads/sites/45/2019/09/ProcessDamping_NAMRC_2014_rev1.pdf
- Honeycutt, A., Schmitz, T.L. (2018). Milling bifurcations: A review of literature and experiment. *Journal of Manufacturing Science and Engineering*, 140(12), 120801. https://doi.org/10.1115/1.4041325

**절삭 열·냉각·기계 열변위**

- Loewen, E.G., Shaw, M.C. (1954). On the Analysis of Cutting-Tool Temperatures. *Transactions of the ASME*, 76(2), 217–225. https://doi.org/10.1115/1.4014799
- Boothroyd, G. (1963). Temperatures in Orthogonal Metal Cutting. *Proceedings of the Institution of Mechanical Engineers*, 177(1), 789–810. https://doi.org/10.1243/PIME_PROC_1963_177_058_02
- Dewes, R.C., Ng, E., Chua, K.S., et al. (1999). Temperature measurement when high speed machining hardened mould/die steel. *Journal of Materials Processing Technology*, 92–93, 293–301. https://doi.org/10.1016/S0924-0136(99)00116-8
- Kops, L., Arenson, M. (1999). Determination of Convective Cooling Conditions in Turning. *CIRP Annals*, 48(1), 47–52. https://doi.org/10.1016/S0007-8506(07)63129-1
- Hosokawa, A., Oda, K., Yamada, K., et al. (2000). 断続切削における工具逃げ面温度 — 高速エンドミル加工に関する研究(第1報) (Temperature of Tool Flank in Intermittent Cutting: Studies on High Speed Endmilling, 1st Report). *精密工学会誌*, 66(11), 1786–1791. https://doi.org/10.2493/jjspe.66.1786
- Rai, J.K., Xirouchakis, P. (2009). FEM-based prediction of workpiece transient temperature distribution and deformations during milling. *The International Journal of Advanced Manufacturing Technology*, 42(5–6), 429–449. https://doi.org/10.1007/s00170-008-1610-6
- Brandão, L.C., Coelho, R.T., Malavolta, A.T. (2010). Experimental and theoretical study on workpiece temperature when tapping hardened AISI H13 using different cooling systems. *Journal of the Brazilian Society of Mechanical Sciences and Engineering*, 32(2), 154–159. https://doi.org/10.1590/S1678-58782010000200008
- Li, Y., Yu, M., Bai, Y., et al. (2021). A Review of Thermal Error Modeling Methods for Machine Tools. *Applied Sciences*, 11(11), 5216. https://doi.org/10.3390/app11115216
- Shokrani, A., Betts, J., Carnevale, M. (2021). Thermal analysis in MQL end milling operations. *Procedia CIRP*, 101, 358–361. https://doi.org/10.1016/j.procir.2020.07.011
- Uhlmann, E., Polte, M., Triebel, F., et al. (2022). Reduction of warm-up period after machine downtime by means of a thermoelectric tempered motorized milling spindle. *euspen Special Interest Group Meeting: Thermal Issues*, ETH Zurich, TI22105. https://euspen.eu/knowledge-base/TI22105.pdf
- Stegmann, J., Baumert, M., Kabelac, S., et al. (2024). Assessment of the Heat Transfer Conditions in the Cavity of a Rotating Circular Saw. *Energies*, 17(13), 3189. https://doi.org/10.3390/en17133189

**재료 모델·입자 계산(MPM)**

- Taylor, G.I., Quinney, H. (1934). The latent energy remaining in a metal after cold working. *Proceedings of the Royal Society of London A*, 143(849), 307–326. https://doi.org/10.1098/rspa.1934.0004
- Johnson, G.R., Cook, W.H. (1983). A constitutive model and data for metals subjected to large strains, high strain rates and high temperatures. *Proceedings of the 7th International Symposium on Ballistics*, The Hague, 541–547.
- Jiang, C., Schroeder, C., Selle, A., et al. (2015). The affine particle-in-cell method. *ACM Transactions on Graphics*, 34(4), 51. https://doi.org/10.1145/2766996
- Hu, Y., Fang, Y., Ge, Z., et al. (2018). A moving least squares material point method with displacement discontinuity and two-way rigid body coupling. *ACM Transactions on Graphics*, 37(4), 150. https://doi.org/10.1145/3197517.3201293

**교과서·고전**

- Taylor, F.W. (1907). On the Art of Cutting Metals. *Transactions of the ASME*, 28, 31–279. https://doi.org/10.1115/1.4060388
- Kronenberg, M. (1966). *Machining Science and Application: Theory and Practice for Operation and Development of Machining Processes*. Pergamon Press.
- Trent, E.M., Wright, P.K. (2000). *Metal Cutting*, 4th ed. Butterworth-Heinemann.
- Boothroyd, G., Knight, W.A. (2006). *Fundamentals of Machining and Machine Tools*, 3rd ed. CRC Press.
- Altintas, Y. (2012). *Manufacturing Automation: Metal Cutting Mechanics, Machine Tool Vibrations, and CNC Design*, 2nd ed. Cambridge University Press. https://doi.org/10.1017/CBO9780511843723

**표준**

- ISO 8688-2:1989. Tool life testing in milling — Part 2: End milling. https://www.iso.org/standard/16092.html
- ISO 3685:1993. Tool-life testing with single-point turning tools. https://www.iso.org/standard/9151.html
- ISO 230-3:2020. Test code for machine tools — Part 3: Determination of thermal effects. https://www.iso.org/standard/73291.html

**특허**

- Norberg, R., Svensson, T. (Santrade Ltd) (1986). Programmable multi-channel tool monitor with multiple alarm limits and sensor taring. US 4,633,424 A. https://patents.google.com/patent/US4633424A/en
- Beyer, P.J., Murphy, T.F., Heckel, D.W. (AMCA International Corp.) (1988). Programmed path for automatic tool retraction and return responsive to degradation threshold. US 4,723,219 A. https://patents.google.com/patent/US4723219A/en
- Tanaka, K., Onishi, Y., Sato, K. (Fanuc Ltd) (1997). Method of monitoring machining load using variable alarm threshold. US 5,631,851 A. https://patents.google.com/patent/US5631851A/en

**제조사·기술 자료**

- FANUC. Series 30i/31i/32i-MODEL A Tool Life Management Specifications (A-79409E).
- Siemens. SINUMERIK 840D 기능 설명서 — 공구 감시(공구 수명·대체 공구).
- Caron Engineering. TMAC (Tool Monitoring Adaptive Control). https://caroneng.com/products/tmac
- Haas Automation. 밀링 설정 84 (Tool Overload Action).
- 공구 카탈로그·기술 자료: Harvey Tool / Harvey Performance, OSG, Kennametal, Mitsubishi Materials, Sandvik Coromant (절삭 조건·비절삭저항 표).
- 코팅 자료: Oerlikon Balzers, Platit.
- 측정·공구 홀더 자료: Renishaw NC4, Blum LaserControl, Haimer, Rego-Fix, BIG KAISER.
