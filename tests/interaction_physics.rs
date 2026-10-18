use endmill_model::physics::{self, cutting_coeffs, mechanistic_forces, CutContext, Engagement, ThermalBody};
use endmill_model::profile::{MachiningPreset, MachiningProfile};
use endmill_model::workpiece_setup::ClampingMethod;
use endmill_model::WorkpieceMaterial;

fn steel() -> MachiningProfile {
    MachiningProfile::from_preset(&MachiningPreset::default_steel_general(), "steel")
}

#[test]
fn flank_wear_raises_radial_force_and_edge_heat() {
    let p = steel();
    let mut ctx = CutContext::from_profile(&p);
    let co = cutting_coeffs(&ctx);
    let d = ctx.tool.diameter_mm;
    let eng = Engagement::side(10.0, 3.0, d, true);
    let rpm = p.conditions.spindle_rpm as f64;
    let fresh = mechanistic_forces(&ctx.tool, &co, 0.05, rpm, &eng, 72, 8);
    ctx.tool.flank_wear_mm = 0.2;
    let worn = mechanistic_forces(&ctx.tool, &co, 0.05, rpm, &eng, 72, 8);
    assert!(worn.radial_load_n_per_mm > 1.5 * fresh.radial_load_n_per_mm, "{} vs {}", worn.radial_load_n_per_mm, fresh.radial_load_n_per_mm);
    assert!(fresh.flank_share == 0.0 && worn.flank_share > 0.3);
    let body = ThermalBody::of_setup(&p.workpiece_setup, &ctx.wp);
    let tf = physics::thermal(&ctx, &co, &fresh, 180.0, rpm, &eng, body.mass_kg, body.area_m2);
    let tw = physics::thermal(&ctx, &co, &worn, 180.0, rpm, &eng, body.mass_kg, body.area_m2);
    assert!(tw.interface_c > tf.interface_c);
    assert!(tw.heat_to_workpiece_share > 0.02 && tw.heat_to_workpiece_share < 0.7);
}

#[test]
fn three_phase_trajectory_is_consistent() {
    let p = steel();
    let ctx = CutContext::from_profile(&p);
    let body = ThermalBody::of_setup(&p.workpiece_setup, &ctx.wp);
    let a = physics::analyze_cut(&ctx, &p.conditions, true, body);
    let tr = &a.trajectory;
    assert!(tr.life_min.is_finite() && tr.life_min > 0.0);
    assert!(tr.life_min < tr.linear_life_min, "{} {}", tr.life_min, tr.linear_life_min);
    assert!(tr.acceleration > 1.2, "acceleration {}", tr.acceleration);
    assert!(a.wear.chatter_factor >= 1.0);
    assert!((a.wear.tool_life_min * a.wear.chatter_factor - tr.life_min).abs() < 1e-6 * tr.life_min);
    let early = tr.vb_at(0.02 * tr.life_min) / (0.02 * tr.life_min);
    let mid = (tr.vb_at(0.5 * tr.life_min) - tr.vb_at(0.4 * tr.life_min)) / (0.1 * tr.life_min);
    assert!(early > mid, "break-in {} vs steady {}", early, mid);
    let v = tr.vb_at(0.6 * tr.life_min);
    assert!((tr.time_at(v) - 0.6 * tr.life_min).abs() < 0.02 * tr.life_min);
    assert!((tr.vb_at(tr.life_min) - tr.vb_limit_mm).abs() < 0.02 * tr.vb_limit_mm);
    assert!(tr.growth_from(0.05, 5.0) > 0.0);
}

#[test]
fn minimum_chip_thickness_and_work_hardening() {
    let mut p = steel();
    p.workpiece_setup.material = WorkpieceMaterial::StainlessSteel;
    let mut ctx = CutContext::from_profile(&p);
    ctx.tool.runout_um = 0.0;
    let co = cutting_coeffs(&ctx);
    let d = ctx.tool.diameter_mm;
    let z = ctx.tool.flutes as f64;
    let rpm = 3000.0;
    let eng = Engagement::side(5.0, 0.5, d, true);
    let tiny = mechanistic_forces(&ctx.tool, &co, 0.004, rpm, &eng, 72, 8);
    let normal = mechanistic_forces(&ctx.tool, &co, 0.06, rpm, &eng, 72, 8);
    assert!(tiny.ploughing_share > normal.ploughing_share);
    let e_tiny = tiny.cutting_power_kw / (5.0 * 0.5 * 0.004 * z * rpm);
    let e_norm = normal.cutting_power_kw / (5.0 * 0.5 * 0.06 * z * rpm);
    assert!(e_tiny > 1.5 * e_norm, "specific energy {} vs {}", e_tiny, e_norm);
    let mut co0 = co.clone();
    co0.work_hardening = 0.0;
    let f0 = mechanistic_forces(&ctx.tool, &co0, 0.02, rpm, &eng, 72, 8);
    let f1 = mechanistic_forces(&ctx.tool, &co, 0.02, rpm, &eng, 72, 8);
    assert!(f1.f_res_mean > f0.f_res_mean);
}

#[test]
fn thermal_softening_and_heat_partition_trends() {
    let p = steel();
    let ctx = CutContext::from_profile(&p);
    let base = cutting_coeffs(&ctx);
    let slow = physics::softening_factor(&ctx, &base, 80.0, 0.03);
    let fast = physics::softening_factor(&ctx, &base, 400.0, 0.03);
    assert!(fast < slow, "{} {}", fast, slow);
    assert!(physics::boothroyd_partition(0.1) > physics::boothroyd_partition(50.0));
    let co = physics::cutting_coeffs_at(&ctx, 400.0, 0.03);
    assert!((co.softening - fast).abs() < 1e-12);
}

#[test]
fn stickout_and_clamping_change_dynamics_and_budget() {
    let mut p = steel();
    p.endmill_setting.stickout_mm = Some(30.0);
    let ctx = CutContext::from_profile(&p);
    let body = ThermalBody::of_setup(&p.workpiece_setup, &ctx.wp);
    let a1 = physics::analyze_cut(&ctx, &p.conditions, true, body);
    p.endmill_setting.stickout_mm = Some(60.0);
    let ctx2 = CutContext::from_profile(&p);
    let a2 = physics::analyze_cut(&ctx2, &p.conditions, true, body);
    assert!(a2.stability.fn_hz < a1.stability.fn_hz);
    assert!(a2.stability.ap_crit_mm < a1.stability.ap_crit_mm);
    p.endmill_setting.stickout_mm = Some(30.0);
    p.workpiece_setup.clamping = ClampingMethod::VacuumChuck;
    let ctx3 = CutContext::from_profile(&p);
    let a3 = physics::analyze_cut(&ctx3, &p.conditions, true, body);
    assert!(a3.fixture_deflection_um > a1.fixture_deflection_um);
    assert!(a3.error_budget_um.total_um > a1.error_budget_um.total_um);
}

#[test]
fn recommendation_respects_chatter_limit() {
    let mut p = steel();
    p.endmill_setting.stickout_mm = Some(70.0);
    let ctx = CutContext::from_profile(&p);
    let body = ThermalBody::of_setup(&p.workpiece_setup, &ctx.wp);
    let rec = physics::recommend(&ctx, true, body, physics::Purpose::Roughing);
    let a = physics::analyze_cut(&ctx, &rec.conditions, true, body);
    assert!(
        a.stability.margin >= 0.9 || rec.notes.iter().any(|n| n.contains("채터")),
        "margin {} notes {:?}",
        a.stability.margin,
        rec.notes
    );
}

#[test]
fn measured_frf_drives_recommendation_into_stable_zone() {
    let mut p = steel();
    p.endmill_setting.stickout_mm = Some(45.0);
    p.machine.tip_fn_hz = Some(2200.0);
    p.machine.tip_stiffness_n_per_um = Some(6.0);
    p.machine.tip_frf_stickout_mm = Some(45.0);
    p.machine.damping_ratio = 0.035;
    let ctx = CutContext::from_profile(&p);
    let body = ThermalBody::of_setup(&p.workpiece_setup, &ctx.wp);
    let rec = physics::recommend(&ctx, true, body, physics::Purpose::Roughing);
    let a = physics::analyze_cut(&ctx, &rec.conditions, true, body);
    assert!(a.stability.measured && !a.stability.stickout_scaled);
    assert!((a.stability.fn_hz - 2200.0).abs() < 1e-6);
    assert!(a.stability.margin >= 0.95, "margin {} notes {:?}", a.stability.margin, rec.notes);
    assert!(rec.notes.iter().any(|n| n.contains("실측 FRF")), "{:?}", rec.notes);
}

fn case(m: WorkpieceMaterial, coating: &str, coolant: endmill_model::profile::CoolantMethod) -> (CutContext, physics::CutAnalysis) {
    let mut p = steel();
    p.workpiece_setup.material = m.clone();
    if let WorkpieceMaterial::AlloySteel { hardness_hrc } = m {
        p.workpiece_setup.hardness_hrc = Some(hardness_hrc);
    }
    p.endmill_setting.coating_name = if coating.is_empty() { None } else { Some(coating.to_string()) };
    p.coolant_config = endmill_model::profile::CoolantConfig::from_method(&coolant);
    p.conditions = endmill_model::CuttingCalculator::recommend_conditions(&m, 10.0, 4, true).unwrap();
    let ctx = CutContext::from_profile(&p);
    let body = ThermalBody::of_setup(&p.workpiece_setup, &ctx.wp);
    let a = physics::analyze_cut(&ctx, &p.conditions, true, body);
    (ctx, a)
}

#[test]
fn coating_workpiece_chemistry_ranks_like_practice() {
    use endmill_model::profile::CoolantMethod::Mist;
    let life = |m: WorkpieceMaterial, c: &str| case(m, c, Mist).1.wear.tool_life_min;
    let steel_altin = life(WorkpieceMaterial::CarbonSteel, "AlTiN");
    let steel_bare = life(WorkpieceMaterial::CarbonSteel, "");
    println!("[COAT] C45 수명 AlTiN {:.1} · 무코팅 {:.1} · 다이아몬드 {:.1} · DLC {:.1} min", steel_altin, steel_bare, life(WorkpieceMaterial::CarbonSteel, "Diamond"), life(WorkpieceMaterial::CarbonSteel, "DLC"));
    assert!(steel_altin > 3.0 * life(WorkpieceMaterial::CarbonSteel, "Diamond"));
    assert!(steel_altin > 2.0 * life(WorkpieceMaterial::CarbonSteel, "DLC"));
    // 강에서 PVD 코팅이 바꾸는 절삭력은 0.4~4 % 뿐이라 수명 이득은 마모 화학(확산·산화·경도)에서 나와야 한다 — 문헌의 코팅/무코팅 수명비 2~5배.
    // (예전 3배 기준은 무코팅에 핀온디스크 마찰 0.6 을 써서 온도를 올린 몫이 섞인 값이었다)
    assert!(steel_altin > 2.0 * steel_bare, "AlTiN {} vs 무코팅 {}", steel_altin, steel_bare);
    let al_altin = life(WorkpieceMaterial::Aluminum, "AlTiN");
    let al_dlc = life(WorkpieceMaterial::Aluminum, "DLC");
    let (al_ctx, al_a) = case(WorkpieceMaterial::Aluminum, "DLC", Mist);
    println!("[COAT] Al 수명 AlTiN {:.1} · DLC {:.1} · 다이아몬드 {:.1} · 무코팅 {:.1} min | DLC 계면 {:.0} °C (산화 개시 {:.0}) μ {:.2}", al_altin, al_dlc, life(WorkpieceMaterial::Aluminum, "Diamond"), life(WorkpieceMaterial::Aluminum, ""), al_a.thermal.interface_c, al_ctx.tool.coating.oxidation_c, al_a.coeffs.mu);
    // DLC/TiAlN 수명비는 문헌(제조사 자료) 1.5~3배. 예전 2배 기준은 DLC 에 핀온디스크 마찰 0.1 을 써서 온도·힘을 깎은 몫이 섞인 값
    assert!(al_dlc > 1.5 * al_altin && life(WorkpieceMaterial::Aluminum, "Diamond") > al_dlc, "DLC {} vs AlTiN {}", al_dlc, al_altin);
    assert!(life(WorkpieceMaterial::Aluminum, "") > al_altin);
    assert!(life(WorkpieceMaterial::Titanium, "AlCrN") > life(WorkpieceMaterial::Titanium, "TiN"));
    let hard = WorkpieceMaterial::AlloySteel { hardness_hrc: 60 };
    let (h_naco, h_altin, h_tin) = (life(hard.clone(), "nACo"), life(hard.clone(), "AlTiN"), life(hard.clone(), "TiN"));
    println!("[COAT] 60 HRC 수명 nACo {:.2} · AlTiN {:.2} · TiN {:.2} min", h_naco, h_altin, h_tin);
    // TiN 의 약점은 약 550 °C 에서 시작되는 산화라, 고경도 계면 온도를 측정 대역(350~600 °C)으로 바로잡자 격차가 줄었다
    // (예전 3배 기준은 745 °C 로 과대평가한 계면 온도에서 나온 값)
    assert!(h_naco > h_altin && h_altin > 2.0 * h_tin, "nACo {} AlTiN {} TiN {}", h_naco, h_altin, h_tin);
    // 다이아몬드 × 강: 촉매 흑연화가 빨라지는 약 700 °C 아래에서도 새 면 반응·산화로 화학 마모가 크다 (Thornton·Wilks 1978 — 연강에서
    // 황동보다 약 10⁴배, Shimada 외 2004 — 900 K 아래는 산화 지배). 확산 항이 무코팅 초경의 몇 배이고 판정은 온도와 관계없이 "쓰지 않음"(3)
    let (_, d) = case(WorkpieceMaterial::CarbonSteel, "Diamond", Mist);
    let (_, bare) = case(WorkpieceMaterial::CarbonSteel, "", Mist);
    println!(
        "[COAT] C45 다이아몬드 계면 {:.0} °C · 흑연화 몫 {:.2} · 확산 항 {:.1} (무코팅 {:.1}) · μ {:.2} (무코팅 {:.2})",
        d.thermal.interface_c, d.tribology.dissolution, d.tribology.diffusion, bare.tribology.diffusion, d.coeffs.mu, bare.coeffs.mu
    );
    assert!(d.tribology.compat.severity == 3 && d.tribology.diffusion > 3.0 * bare.tribology.diffusion, "{:?}", d.tribology.compat);
    // 무너진 탄소 코팅의 겉보기 마찰은 무코팅 수준이다 — 예전에는 최대 4배로 키워 계면 온도가 되먹임으로 올랐다
    assert!(d.coeffs.mu < 1.15 * bare.coeffs.mu, "diamond μ {:.2} vs bare {:.2}", d.coeffs.mu, bare.coeffs.mu);
    let (_, a) = case(WorkpieceMaterial::Aluminum, "AlTiN", Mist);
    assert!(a.tribology.compat.messages.iter().any(|m| m.contains("응착")));
    assert_eq!(d.regime.len(), endmill_model::tribology::REGIME_DIM);
}

#[test]
fn coolant_effect_depends_on_workpiece_and_temperature() {
    use endmill_model::profile::CoolantMethod::{AirBlast, Dry, Flood, Mist, ThroughTool};
    let (_, ti_dry) = case(WorkpieceMaterial::Titanium, "AlTiN", Dry);
    let (_, ti_hpc) = case(WorkpieceMaterial::Titanium, "AlTiN", ThroughTool);
    assert!(ti_hpc.wear.tool_life_min > 2.5 * ti_dry.wear.tool_life_min);
    assert!(ti_hpc.thermal.interface_c < ti_dry.thermal.interface_c - 100.0);
    assert!(ti_hpc.coeffs.chip_lift > 0.1);
    let hard = WorkpieceMaterial::AlloySteel { hardness_hrc: 55 };
    let (_, h_air) = case(hard.clone(), "AlTiN", AirBlast);
    let (_, h_flood) = case(hard, "AlTiN", Flood);
    assert!(h_flood.thermal.thermal_crack_risk > h_air.thermal.thermal_crack_risk + 0.2);
    assert!(h_air.wear.tool_life_min > h_flood.wear.tool_life_min);
    let (_, s_dry) = case(WorkpieceMaterial::CarbonSteel, "AlTiN", Dry);
    let (_, s_mist) = case(WorkpieceMaterial::CarbonSteel, "AlTiN", Mist);
    assert!(s_mist.coeffs.mu < s_dry.coeffs.mu && s_mist.coeffs.lubricant_access > 0.05);
    let (_, in_flood) = case(WorkpieceMaterial::Inconel, "AlTiN", Flood);
    let (_, in_hpc) = case(WorkpieceMaterial::Inconel, "AlTiN", ThroughTool);
    assert!(in_hpc.thermal.cooling.effectiveness > in_flood.thermal.cooling.effectiveness);
    assert!(in_hpc.thermal.cooling.boiling_factor > in_flood.thermal.cooling.boiling_factor);
    let (_, cfrp) = case(WorkpieceMaterial::CFRP, "Diamond", Flood);
    assert!(cfrp.tribology.compat.severity >= 2);
}

#[test]
fn lubricant_penetration_and_wear_fall_with_speed() {
    let p = steel();
    let ctx = CutContext::from_profile(&p);
    let slow = endmill_model::tribology::contact_friction(&ctx, 60.0, 500.0);
    let fast = endmill_model::tribology::contact_friction(&ctx, 300.0, 500.0);
    assert!(slow.lubricant_access > fast.lubricant_access && slow.mu_eff < fast.mu_eff);
    let body = ThermalBody::of_setup(&p.workpiece_setup, &ctx.wp);
    let mut lives = Vec::new();
    for f in [0.5, 1.0, 2.0] {
        let mut c = p.conditions.clone();
        c.spindle_rpm = (c.spindle_rpm as f64 * f).round() as u32;
        c.feed_rate_mm_min *= f;
        let a = physics::analyze_cut(&ctx, &c, true, body);
        lives.push((a.thermal.interface_c, a.wear.tool_life_min));
    }
    assert!(lives[0].0 < lives[1].0 && lives[1].0 < lives[2].0, "{:?}", lives);
    assert!(lives[0].1 > 2.0 * lives[1].1 && lives[1].1 > 2.0 * lives[2].1, "{:?}", lives);
}

#[test]
fn material_properties_follow_temperature_and_rate() {
    let ti = physics::WorkpieceProps::of(&WorkpieceMaterial::Titanium);
    let st = physics::WorkpieceProps::of(&WorkpieceMaterial::CarbonSteel);
    assert!(ti.k_at(500.0) > 1.5 * ti.k_at(20.0));
    assert!(st.k_at(500.0) < 0.8 * st.k_at(20.0));
    assert!(st.flow_stress(0.5, 1.0e4, 600.0) < st.flow_stress(0.5, 1.0e4, 200.0));
    assert!(st.flow_stress(0.5, 1.0e5, 400.0) > st.flow_stress(0.5, 1.0, 400.0));
    let shares = endmill_model::tribology::normalized_shares(&ti.mech_shares);
    assert!((shares.iter().sum::<f64>() - 1.0).abs() < 1e-9);
    let p = steel();
    let ctx = CutContext::from_profile(&p);
    let r = physics::reference_state(&ctx);
    assert!(r.t_c > 100.0 && r.raw.iter().all(|v| *v > 0.0));
}

// ---- 공구–피삭재 조합 기준값 (src/pairref.rs, WorkpieceProps 의 문헌 기준값) ----

fn builtin(name: &str) -> MachiningProfile {
    let p = MachiningPreset::builtin().into_iter().find(|p| p.name.contains(name)).expect("preset");
    MachiningProfile::from_preset(&p, &p.name)
}

/// 내장 프리셋의 공구–칩 계면 온도(건식)가 측정 문헌으로 만든 대역 안에 있어야 한다 (R4 조사: 측정법 편향을 보정한 추정 대역).
/// 예전 값: 58 HRC 볼 745 °C (대역 350~600), C45 606 °C (420~580), Al Vc 300 DLC 202 °C (220~380, Vc 165 보다 낮았음)
#[test]
fn preset_interface_temperatures_sit_in_published_bands() {
    let bands = [
        ("알루미늄 황삭", 180.0, 350.0),
        ("알루미늄 정삭", 220.0, 380.0),
        ("티타늄", 500.0, 750.0),
        ("탄소강", 420.0, 580.0),
        ("스테인리스", 480.0, 650.0),
        ("인코넬", 550.0, 800.0),
        ("금형강", 350.0, 600.0),
        ("CFRP", 150.0, 300.0),
    ];
    let mut al = Vec::new();
    for (name, lo, hi) in bands {
        let p = builtin(name);
        let ctx = CutContext::from_profile(&p);
        let body = ThermalBody::of_setup(&p.workpiece_setup, &ctx.wp);
        let a = physics::analyze_cut(&ctx, &p.conditions, true, body);
        let t = a.thermal.interface_dry_c;
        println!("[TEMP] {:<10} 계면 건식 {:.0} °C → 냉각 {:.0} °C (대역 {:.0}~{:.0})", name, t, a.thermal.interface_c, lo, hi);
        assert!(t > lo && t < hi, "{}: 계면 {:.0} °C 가 대역 {:.0}~{:.0} 밖", name, t, lo, hi);
        if name.starts_with("알루미늄") {
            al.push(t);
        }
    }
    assert!(al[1] > al[0], "같은 최대 칩 두께에서 Vc 300 이 Vc 165 보다 차가우면 안 된다: {:?}", al);
}

/// 거시 절삭에서 코팅이 바꾸는 절삭력은 0~6 % (C45 AlTiN −0.4~−4 %, Al-Si DLC −4~−6 %). 예전에는 핀온디스크 마찰을 그대로 써서
/// DLC 알루미늄 정삭 μ 0.10 · kc −40 %, 다이아몬드 CFRP μ 0.05 였다
#[test]
fn coatings_change_cutting_friction_only_a_little() {
    for (name, coat) in [("알루미늄 정삭", "DLC"), ("CFRP", "Diamond"), ("탄소강", "AlTiN"), ("티타늄", "AlCrN")] {
        let p = builtin(name);
        let mut bare = p.clone();
        bare.endmill_setting.coating_name = None;
        let mut coated = p.clone();
        coated.endmill_setting.coating_name = Some(coat.into());
        let cb = cutting_coeffs(&CutContext::from_profile(&bare));
        let cc = cutting_coeffs(&CutContext::from_profile(&coated));
        let r = cc.kc11 / cb.kc11;
        assert!(r > 0.9 && r < 1.02, "{} {}: 코팅 kc 비 {:.3}", name, coat, r);
        assert!(cc.mu > 0.2, "{} {}: 절삭 마찰 {:.3} 는 핀온디스크 값이 아니어야 한다", name, coat, cc.mu);
    }
}

/// 날끝 계수와 반경력 비는 피삭재별 측정값을 따른다: 알루미늄 엔드밀 Kre/Kte 0.83~1.0, Ti-6Al-4V 1.8~1.9 (Kte 24~41 N/mm),
/// Krc/Ktc 하한은 C45 0.33 (실측 0.34~0.38)·Ti 0.37 — 예전에는 모든 소재 Kre = 1.4·Kte, Kr 하한 0.15
#[test]
fn edge_and_radial_coefficients_follow_the_workpiece() {
    let al = CutContext::from_profile(&builtin("알루미늄 황삭"));
    let ti = CutContext::from_profile(&builtin("티타늄"));
    let st = CutContext::from_profile(&builtin("탄소강"));
    let (ca, ct, cs) = (cutting_coeffs(&al), cutting_coeffs(&ti), cutting_coeffs(&st));
    assert!((ca.kre / ca.kte - 0.9).abs() < 1e-9 && (ct.kre / ct.kte - 1.9).abs() < 1e-9 && (cs.kre / cs.kte - 1.5).abs() < 1e-9);
    assert!(ct.kte > 24.0 && ct.kte < 45.0, "Ti Kte {}", ct.kte);
    assert!(ca.kr >= 0.40 && ct.kr >= 0.37 && cs.kr >= 0.33, "{} {} {}", ca.kr, ct.kr, cs.kr);
}

/// 길들이기는 수명의 비율이 아니라 날끝·소재로 정해지는 양이고 수 분 안에 끝난다 (HRC 52 볼 첫 4초에 30~60 µm, Ti AlCrN 5 m 이송에 115 µm).
/// 예전에는 τ = 선형 수명의 3 % 라서 알루미늄 DLC 정삭이 21.7분, CFRP 11.9분이었다
#[test]
fn break_in_is_a_pair_amount_with_a_minutes_scale_not_a_life_fraction() {
    let names = ["알루미늄 정삭", "CFRP", "티타늄", "탄소강", "금형강"];
    for name in names {
        let p = builtin(name);
        let ctx = CutContext::from_profile(&p);
        let body = ThermalBody::of_setup(&p.workpiece_setup, &ctx.wp);
        let a = physics::analyze_cut(&ctx, &p.conditions, true, body);
        let tr = &a.trajectory;
        assert!(tr.break_in_tau_min > 0.3 && tr.break_in_tau_min < 5.0, "{}: τ {}", name, tr.break_in_tau_min);
        assert!(tr.break_in_vb_mm > 0.0 && tr.break_in_vb_mm <= 0.4 * tr.vb_limit_mm + 1e-12, "{}: VB_b {}", name, tr.break_in_vb_mm);
        assert!((tr.vb_at(tr.life_min) - tr.vb_limit_mm).abs() < 0.03 * tr.vb_limit_mm, "{}", name);
    }
    let vb = |name: &str| {
        let p = builtin(name);
        endmill_model::pairref::breakin(&CutContext::from_profile(&p), p.conditions.cutting_speed_m_min).vb_mm
    };
    assert!(vb("티타늄") > vb("탄소강") && vb("탄소강") > vb("알루미늄 정삭"));
    let ti = CutContext::from_profile(&builtin("티타늄"));
    assert!(endmill_model::pairref::breakin(&ti, 120.0).vb_mm > endmill_model::pairref::breakin(&ti, 60.0).vb_mm, "Ti 길들이기는 속도와 함께 커진다");
}

/// 수명 판정 VB 는 부품 공차 규칙과 피삭재 판정 기준 중 작은 쪽: 고경도 정삭 0.15, Ti·CFRP 0.2, 공차가 넉넉한 탄소강 0.3
#[test]
fn tool_life_criterion_follows_the_workpiece_and_the_tolerance() {
    let lim = |name: &str| CutContext::from_profile(&builtin(name)).vb_limit_mm();
    assert!((lim("금형강") - 0.15).abs() < 1e-9);
    assert!((lim("티타늄") - 0.2).abs() < 1e-9);
    assert!((lim("CFRP") - 0.2).abs() < 1e-9, "CFRP 공차 0.1 mm 라도 박리·미절단 때문에 0.2");
    assert!((lim("탄소강") - 0.3).abs() < 1e-9);
    assert!((lim("알루미늄 황삭") - 0.2).abs() < 1e-9, "공차 20 µm 규칙");
}

/// 홀더 종류가 런아웃 기본값을 정한다 (열박음 3, ER UP 5, ER 표준 15, 사이드락 15 µm TIR). 입력한 런아웃이 항상 우선이고,
/// 홀더 필드가 없던 예전 저장값은 미지정으로 읽혀 공구 등급 가정(고급형 3 / 보급형 10 µm)을 그대로 쓴다
#[test]
fn holder_type_sets_default_runout_unless_runout_is_entered() {
    use endmill_model::profile::{EndMillMockupSetting, HolderType};
    let mut p = builtin("알루미늄 황삭");
    p.endmill_setting.runout_um = None;
    for (h, tir) in [(HolderType::ShrinkFit, 3.0), (HolderType::ErUltraPrecision, 5.0), (HolderType::ErStandard, 15.0), (HolderType::SideLock, 15.0)] {
        p.endmill_setting.holder = h;
        assert_eq!(CutContext::from_profile(&p).tool.runout_um, tir, "{:?}", h);
    }
    p.endmill_setting.holder = HolderType::Unknown;
    let legacy = if p.endmill_setting.is_high_end { 3.0 } else { 10.0 };
    assert_eq!(CutContext::from_profile(&p).tool.runout_um, legacy);
    p.endmill_setting.holder = HolderType::ErStandard;
    p.endmill_setting.runout_um = Some(6.0);
    assert_eq!(CutContext::from_profile(&p).tool.runout_um, 6.0);
    let mut v = serde_json::to_value(&p.endmill_setting).unwrap();
    v.as_object_mut().unwrap().remove("holder");
    let back: EndMillMockupSetting = serde_json::from_value(v).unwrap();
    assert_eq!(back.holder, HolderType::Unknown);
}

/// 감쇠비 기본값은 돌출비로 정한다 (실측 L/D 3.75 → 0.05, 5.8 → 0.026, 12 → 0.009; 짧은 공구는 홀더 모드 때문에 0.045 상한).
/// 사용자가 넣은 감쇠비(탭 테스트, 또는 예전 기본 0.03 과 다른 저장값)는 그대로 쓴다
#[test]
fn default_damping_falls_with_stickout_and_user_values_are_kept() {
    use endmill_model::pairref::default_damping;
    use endmill_model::profile::HolderType;
    let mut last = f64::INFINITY;
    for ld in [2.0, 3.0, 4.0, 6.0, 9.0, 14.0] {
        let z = default_damping(ld, HolderType::Unknown);
        assert!(z <= last && (0.010..=0.045).contains(&z), "L/D {} ζ {}", ld, z);
        last = z;
    }
    assert!((default_damping(5.8, HolderType::Unknown) - 0.026).abs() < 0.003);
    assert!(default_damping(5.8, HolderType::Hydraulic) > default_damping(5.8, HolderType::ShrinkFit));
    let mut p = builtin("탄소강");
    let short = CutContext::from_profile(&p).tool.damping_ratio;
    p.endmill_setting.stickout_mm = Some(70.0);
    let long = CutContext::from_profile(&p).tool.damping_ratio;
    assert!(long < short, "{} vs {}", long, short);
    p.machine.damping_ratio = 0.035;
    assert_eq!(CutContext::from_profile(&p).tool.damping_ratio, 0.035);
    p.machine.damping_ratio = 0.03;
    p.machine.damping_user = true;
    assert_eq!(CutContext::from_profile(&p).tool.damping_ratio, 0.03);
}

/// 공구가 여유각을 정하지 않으면 그 피삭재용 엔드밀 설계값을 쓴다 (알루미늄 12°, 탄소강 8°, Ti 10° — 예전에는 모두 10°)
#[test]
fn clearance_defaults_to_the_workpiece_tool_design() {
    for (name, deg) in [("알루미늄 황삭", 12.0), ("탄소강", 8.0), ("티타늄", 10.0)] {
        let p = builtin(name);
        assert!(p.endmill_setting.clearance_deg.is_none(), "{}", name);
        assert_eq!(CutContext::from_profile(&p).tool.clearance_deg, deg, "{}", name);
    }
    let hard = physics::WorkpieceProps::of(&WorkpieceMaterial::AlloySteel { hardness_hrc: 58 });
    assert!(hard.clearance_deg < 8.0 && hard.kc11 > 4000.0, "Sandvik 58 HRC kc1.1 약 4070: {}", hard.kc11);
    assert!(endmill_model::pairref::pd_clearance_factor(7.0) > 1.0 && endmill_model::pairref::pd_clearance_factor(15.0) < 1.0);
}
