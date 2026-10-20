//! 문헌 실측 벤치마크 시험 — 논문 공구·소재·조건을 샌드박스 데이터로 옮긴 N:N (`endmill_model::benchmark`) 이 물리 점검을 통과하고,
//! 물리를 고친 비교(1018 Taylor 기울기·절대 수명 · Ti 속도 기울기 · 계면 온도의 속도 방향 · 선형 계수 힘 · 반경력 비)가 실측 대역 안에 머무는지 본다.
//! 비교 줄 `[BENCH]` · 요약 `[BENCH-SUM]` · 수명 격자 `[BENCH-GRID]` 를 찍는다. 아직 맞지 않는 비교(Ti 절대 수명 · 알루미늄 DLC 이득 ·
//! 316L 건식/습식 · IN718 고속 · 힘의 속도 기울기 등)는 실패로 두지 않고 줄로 남긴다 — 해설은 `docs/literature-benchmark.md` 1장·8장
use endmill_model::benchmark::{self, Comparison, Param, Quantity};

fn find<'a>(rep: &'a benchmark::Report, id: &str) -> &'a Comparison {
    rep.comparisons.iter().find(|c| c.id == id).unwrap_or_else(|| panic!("비교 {} 없음", id))
}

#[test]
fn literature_benchmark_runs_the_nn_grid_and_keeps_the_fixed_physics_in_band() {
    let rep = benchmark::run();
    for l in benchmark::lines(&rep) {
        println!("{}", l);
    }
    let tools = benchmark::paper_tools().len() + benchmark::sandbox_tools().len();
    let cuts = benchmark::paper_cuts().len();
    assert_eq!(rep.cells.len(), tools * cuts, "N:N 칸 수");
    assert!(rep.issues.is_empty(), "물리 점검: {:?}", rep.issues);
    // 1018 무코팅 초경의 Taylor 기울기 4건 (예전 −1.80 · −2.46 · −1.92 — 응착을 온도 활성화하기 전), Vc 89.8→448.8 의 5배 구간 포함
    for id in ["kar_taylor_lo", "kar_taylor_hi", "kar21_taylor", "kar_taylor_wide"] {
        let c = find(&rep, id);
        assert!(c.in_band, "{}: 기울기 {:.2} 가 대역 {:.2}~{:.2} 밖", id, c.predicted, c.lo, c.hi);
    }
    // Ti-6Al-4V 수명의 속도 기울기 3건 — 두 영역 활성화(550 °C 아래 2,500 K · 위 16,000 K, ETH 2025 플랭크 C₂) 뒤 대역 안
    // (예전 6,500 K 한 영역: Tatar −2.43 · Liu −1.65 · Matuszak −2.45)
    for id in ["tatar_vc", "liu_vc", "mat_vc"] {
        let c = find(&rep, id);
        assert!(c.in_band, "{}: 기울기 {:.2} 가 대역 {:.2}~{:.2} 밖", id, c.predicted, c.lo, c.hi);
    }
    // H13 52 HRC 볼: 계면 온도가 속도와 함께 오른다 (예전 기울기 0.11 — 전단면 온도를 크기효과 비절삭에너지로 잡아 고속에서 kc 가 빠르게 줄었다)
    let t = find(&rep, "dewes_t_vc");
    assert!(t.predicted > 0.2 && t.in_band, "계면 온도 상승의 속도 기울기 {:.2}", t.predicted);
    // 절삭력: Ti 선형 계수로 되돌린 평균 힘 ±25 % · 반경력 비
    for id in ["li_slot05_f", "li_slot10_f", "ti_kr"] {
        let c = find(&rep, id);
        assert!(c.in_band, "{}: {:.3} vs 실측 {:.3}", id, c.predicted, c.measured);
    }
    // 1018 절대 수명 7건 (Vc 89.8~448.8): 여유각 15° 마모 랜드 기하 · 저탄소강 소재(C45 마모 상수에 묶음) · 궤적 적분 뒤 예측/실측 0.52~1.10
    // (1차 0.30~0.61, 그 전 0.07~0.18). 대역 ×/÷2 에 여유를 둔 0.4~2.5
    for id in ["kar1018_090", "kar1018_114", "kar1018_224", "kar1018_374", "kar1018_449", "kar2021_150", "kar2021_299"] {
        let c = find(&rep, id);
        assert!(c.ratio > 0.4 && c.ratio < 2.5, "{}: 예측/실측 {:.2}", id, c.ratio);
    }
    let inb = rep.comparisons.iter().filter(|c| c.in_band).count();
    println!("[BENCH] 대역 안 {} / {}", inb, rep.comparisons.len());
    assert!(inb >= 24, "대역 안 비교가 줄었다: {} / {}", inb, rep.comparisons.len());
}

/// 공구·소재 판정 규칙이 논문 칸에서도 지켜지는가: 탄소 코팅 × 철계·Ti 와 HSS × 52 HRC 는 '쓰지 않음', 논문 공구는 자기 조건에서 쓸 수 있음
#[test]
fn nn_grid_flags_chemistry_and_substrate_like_practice() {
    let rep = benchmark::run();
    let cell = |t: &str, c: &str| rep.cells.iter().find(|p| p.tool == t && p.cut == c).unwrap_or_else(|| panic!("{} × {}", t, c));
    for (t, c) in [("grig_al_dlc", "kar1018_224"), ("grig_st_dlc", "tatar_ti"), ("hov_hss_dlc", "dewes_h13")] {
        let p = cell(t, c);
        assert!(p.compat_severity >= 2 || p.fit_verdict >= 2, "{} × {}: 궁합 {} 적합도 {}", t, c, p.compat_severity, p.fit_verdict);
    }
    for t in ["hov_hss_bare", "hov_hss_tialn"] {
        let p = cell(t, "dewes_h13");
        assert!(p.fit_verdict >= 2 && p.life_min < 0.05, "HSS × 52 HRC: 판정 {} 수명 {:.3}", p.fit_verdict, p.life_min);
    }
    for (t, c) in [
        ("kar_1018_insert", "kar1018_090"),
        ("tatar_r390", "tatar_ti"),
        ("grig_al_bare", "grig_al"),
        ("dewes_ball", "dewes_h13"),
        ("li_ti_slot", "li_ti_slot10"),
        ("phm_ball", "phm2010"),
        ("tym_naco", "tym_316l"),
        ("grg_harvi", "grg_in718_flood"),
        ("hisam_r217", "hisam_in718"),
        ("luo_ti_apmt", "luo_ti_60"),
    ] {
        let p = cell(t, c);
        assert!(p.compat_severity < 3 && p.fit_verdict < 2, "논문 공구 {} 가 자기 조건 {} 에서 부적합: 궁합 {} 적합도 {}", t, c, p.compat_severity, p.fit_verdict);
    }
}

/// 물리 방향: 같은 공구·조건에서 속도를 올리면 계면 온도는 오르고 수명은 준다 (Taylor) — 논문 조건 전부에서
#[test]
fn speed_raises_temperature_and_shortens_life_in_every_paper_condition() {
    let tools = benchmark::paper_tools();
    for c in benchmark::paper_cuts() {
        let own = tools.iter().find(|t| t.key == c.tool).unwrap_or_else(|| panic!("{} 의 논문 공구 {}", c.key, c.tool));
        let lo = benchmark::predict(own, &c, &[(Param::Vc, 0.85)]);
        let hi = benchmark::predict(own, &c, &[(Param::Vc, 1.15)]);
        println!(
            "[BENCH] 속도 ±15 % · {} × {}: 계면 {:.0} → {:.0} °C · 수명 {:.3} → {:.3} min",
            own.key, c.key, lo.interface_c, hi.interface_c, lo.value(Quantity::LifeMin), hi.value(Quantity::LifeMin)
        );
        assert!(hi.interface_c > lo.interface_c, "{} × {}: 속도를 올렸는데 계면 온도가 내려감", own.key, c.key);
        assert!(hi.value(Quantity::LifeMin) < lo.value(Quantity::LifeMin), "{} × {}: 속도를 올렸는데 수명이 늘어남", own.key, c.key);
    }
}
