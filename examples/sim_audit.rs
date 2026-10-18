//! 시뮬레이션 감사 (점검용) — 프리셋 하나를 지정한 경로 패턴으로 wearcomp::run_pipeline 끝까지 돌린 뒤
//! 입력 조건 → 경로 → 공칭 해석 → 경로 시뮬레이션 → 마모 로그 → 보정 판정의 값을 나란히 놓고,
//! 서로 맞아야 하는 값(체적·시간·이송·결합각·마모 속도·교체 개수)을 교차 확인한다.
//!
//!   cargo run --features cuda --example sim_audit -- <프리셋 번호 또는 이름 일부> [패턴 키]
//!
//! 프리셋 번호는 인자를 주지 않으면 목록으로 출력된다. 패턴 기본값은 rect_profile.
use endmill_model::gcode::{GCodeGenerator, ToolPathPattern};
use endmill_model::physics::{self, ThermalBody};
use endmill_model::profile::{MachiningPreset, MachiningProfile};
use endmill_model::sds::SdsStore;
use endmill_model::wearcomp::{self, CompOptions};

fn main() {
    let presets = MachiningPreset::builtin();
    let Some(sel) = std::env::args().nth(1) else {
        for (i, p) in presets.iter().enumerate() {
            println!("{:>2}  {}", i, p.name);
        }
        return;
    };
    let pattern_key = std::env::args().nth(2).unwrap_or_else(|| "rect_profile".to_string());
    let preset = match sel.parse::<usize>() {
        Ok(i) => presets.get(i).cloned().expect("프리셋 번호 범위 밖"),
        Err(_) => presets.iter().find(|p| p.name.contains(sel.as_str())).cloned().expect("프리셋 이름 불일치"),
    };
    let p = MachiningProfile::from_preset(&preset, &preset.name);
    let dir = std::env::temp_dir().join(format!("endmill-sim-audit-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut sds = SdsStore::open(&dir.join("sds"));

    // ---- 1. 입력 ----
    let c = &p.conditions;
    let w = &p.workpiece_setup;
    let e = &p.endmill_setting;
    println!("== 1. 입력 ==");
    println!(
        "프리셋 {} | 소재 {:?} 판재 {}x{}x{} mm 여유 {} mm 공차 {:?} 고정 {:?}",
        p.name, w.material, w.width_mm, w.height_mm, w.thickness_mm, w.stock_allowance_mm, w.tolerance_mm, w.clamping
    );
    println!(
        "공구 D{} Z{} 헬릭스 {}° LOC {} OAL {} 샹크 {} 코팅 {:?} 고급 {} 가변피치 {}",
        e.diameter_mm, e.flute_count, e.helix_angle_deg, e.loc_mm, e.oal_mm, e.shank_diameter_mm, e.coating_name, e.is_high_end, e.variable_pitch
    );
    println!(
        "조건 Vc {:.1} m/min rpm {} F {:.1} mm/min fz {:.4} ap {:.2} ae {:.2} 냉각 {:?}",
        c.cutting_speed_m_min, c.spindle_rpm, c.feed_rate_mm_min, c.feed_per_tooth_mm, c.axial_doc_mm, c.radial_doc_mm, c.coolant
    );
    println!(
        "프로그램 fz = F/(rpm*Z) = {:.4} mm (프리셋 fz {:.4})",
        c.feed_rate_mm_min / (c.spindle_rpm as f64 * e.flute_count as f64),
        c.feed_per_tooth_mm
    );
    let m = &p.machine;
    println!(
        "기계 홀더강성 {} N/um 감쇠비 {} 홀더질량 {} kg 최대동력 {} kW 기준rpm {} 효율 {} 실측FRF {:?}/{:?}",
        m.holder_stiffness_n_per_um, m.damping_ratio, m.holder_mass_kg, m.max_power_kw, m.base_rpm, m.efficiency, m.tip_fn_hz, m.tip_stiffness_n_per_um
    );

    // ---- 2. 경로 ----
    let pat = ToolPathPattern::from_key(&pattern_key, &p).expect("패턴");
    let segs = GCodeGenerator::generate_synthetic_segments_with_pattern(&p, &pat);
    println!("== 2. 경로 ({}, 세그먼트 {}개) ==", pattern_key, segs.len());
    let mut prev = (0.0, 0.0, e.loc_mm + 10.0);
    let (mut feed_len, mut rapid_len) = (0.0, 0.0);
    for (i, s) in segs.iter().enumerate() {
        let l = s.length_from(prev);
        let end = s.end_point();
        if s.feed().is_some() {
            feed_len += l;
        } else {
            rapid_len += l;
        }
        if i < 24 || i + 3 >= segs.len() {
            println!("  [{:>3}] {:<7} -> ({:8.2},{:8.2},{:8.2}) 길이 {:8.2} mm 이송 {:?}", i, s.kind(), end.0, end.1, end.2, l, s.feed());
        }
        prev = end;
    }
    println!(
        "  이송 구간 합 {:.1} mm, 급이송 합 {:.1} mm → 프리셋 이송 {:.1} mm/min 이면 이송 {:.2} s",
        feed_len,
        rapid_len,
        c.feed_rate_mm_min,
        feed_len / c.feed_rate_mm_min * 60.0
    );

    // ---- 3. 공칭 해석 (프리셋 ae 그대로) ----
    let (ctx, _) = endmill_model::pipeline::calibrated_context(&p, Some(&sds));
    let body = ThermalBody::of_setup(&p.workpiece_setup, &ctx.wp);
    let a = physics::analyze_cut(&ctx, &p.conditions, true, body);
    println!("== 3. 공칭 해석 analyze_cut (ae={:.2}) ==", p.conditions.radial_doc_mm);
    println!(
        "공구 돌출 {:.1} mm 코어비 {:.2} E {:.0} GPa 날끝반경 {:.2} um 경사각 {:.1}° 여유각 {:.1}° 런아웃 {:.1} um 코팅 {} {:.1} um",
        ctx.tool.stickout_mm, ctx.tool.core_ratio, ctx.tool.elastic_gpa, ctx.tool.edge_radius_um, ctx.tool.rake_deg, ctx.tool.clearance_deg, ctx.tool.runout_um, ctx.tool.coating.name, ctx.tool.coating.thickness_um
    );
    println!("피삭재 kc11 {:.0} mc {:.2} 보정 {:?}", ctx.wp.kc11, ctx.wp.mc, ctx.calib);
    println!(
        "결합 {:?} {:.1}° 칩얇아짐 x{:.2} | Vc_eff {:.1} | MRR {:.3} cm3/min",
        a.mode, a.engage_angle_deg, a.chip_thinning_factor, a.vc_effective_m_min, a.mrr_cm3_min
    );
    let k = &a.coeffs;
    println!(
        "계수 mu {:.3} beta {:.1}° phi {:.1}° kc11 {:.0} mc {:.2} kr {:.3} kte {:.2} kre {:.2} 연화 {:.3} 소성변형률속도 {:.0}/s 속도계수 {:.3} h_min {:.2} um",
        k.mu, k.beta_deg, k.shear_angle_deg, k.kc11, k.mc, k.kr, k.kte, k.kre, k.softening, k.strain_rate, k.rate_factor, k.h_min_mm * 1000.0
    );
    let f = &a.forces;
    println!(
        "힘 평균 {:.1} N 최대 {:.1} N (피크/평균 {:.2}) Fx {:.1} Fy {:.1} 토크 {:.4}/{:.4} Nm 절삭동력 {:.4} kW 스핀들 {:.4} kW 칩 평균 {:.2} 최대 {:.2} um 이빨수 {:.2}",
        f.f_res_mean, f.f_res_peak, f.f_res_peak / f.f_res_mean.max(1e-9), f.fx_mean, f.fy_mean, f.torque_mean_nm, f.torque_peak_nm, f.cutting_power_kw, a.spindle_power_kw, f.h_mean_mm * 1000.0, f.h_max_mm * 1000.0, f.teeth_in_cut
    );
    println!(
        "비에너지 = 절삭동력/MRR = {:.3} J/mm3 | 밀림 {:.2} 플랭크 점유(미반영) | 강성 {:.3} N/um 휨(피크) {:.2} um 벽(top/bot/mean) {:.2}/{:.2}/{:.2} um",
        f.cutting_power_kw * 1000.0 / (a.mrr_cm3_min * 1000.0 / 60.0).max(1e-9),
        a.ploughing_share,
        a.stiffness_n_per_um,
        a.deflection_peak_um,
        a.wall.deflection_top_um,
        a.wall.deflection_bottom_um,
        a.wall.deflection_um
    );
    let t = &a.thermal;
    println!(
        "열 계면 건식 {:.0} 습식 {:.0} °C (냉각저감 {:.0}%) 공구몸체 {:.0} °C 반경성장 {:.2} um 접촉 {:.3} ms 과도계수 {:.3} 공작물열점유 {:.3} 공작물정상상승 {:.1} K",
        t.interface_dry_c, t.interface_c, t.coolant_reduction_pct, t.tool_body_c, t.tool_radial_growth_um, t.engagement_time_ms, t.transient_factor, t.heat_to_workpiece_share, t.workpiece_steady_rise_c
    );
    println!("  cooling {}", serde_json::to_string(&t.cooling).unwrap_or_default());
    println!("  barrier {}", serde_json::to_string(&t.barrier).unwrap_or_default());
    let wr = &a.wear;
    println!(
        "마모 rate {:.3} um/min 수명 {:.1} min (선형 {:.1}) 한계 {:.2} mm | 인자: 온도 {:.2} 응력 {:.2} 속도 {:.2} 코팅 {:.2} 런아웃 {:.2} 배출 {:.3} 채터 {:.3} 기구합 {:.3} 지배 {}",
        wr.vb_rate_mm_per_min * 1000.0, wr.tool_life_min, wr.linear_life_min, wr.vb_limit_mm, wr.temperature_factor, wr.stress_factor, wr.speed_factor, wr.coating_factor, wr.runout_factor, wr.evacuation_factor, wr.chatter_factor, wr.mechanism_factor, wr.dominant
    );
    for mech in wr.mechanisms.iter() {
        println!("    기구 {:<10} 점유 {:.1}% 기준대비 x{:.3}", mech.label, mech.share * 100.0, mech.ratio);
    }
    let tr = &a.trajectory;
    println!(
        "궤적 수명 {:.1} min 선형 {:.1} min kappa {:.3} 길들이기 VB {:.4} mm tau {:.3} min 3단계 {:?} 가속 x{:.2} 점수 {}",
        tr.life_min, tr.linear_life_min, tr.kappa, tr.break_in_vb_mm, tr.break_in_tau_min, tr.phase3_min, tr.acceleration, tr.t_min.len()
    );
    let st = &a.stability;
    println!(
        "안정성 fn {:.0} Hz k {:.3} N/um zeta {:.3} kt {:.0} N/mm2 kr {:.3} 한계 ap {:.2} mm (임계 {:.2}) 여유 {:.3} 채터 {:.0} Hz 공정감쇠 x{:.2} 피치 x{:.2} 실측 {}",
        st.fn_hz, st.k_n_per_um, st.zeta, st.kt_n_mm2, st.kr, st.ap_lim_mm, st.ap_crit_mm, st.margin, st.chatter_hz, st.process_damping_gain, st.pitch_gain, st.measured
    );
    let fv = &a.forced;
    println!(
        "강제진동 날통과 {:.0} Hz F1 {:.1} N 진폭 {:.3} um 배율 {:.3} SLE {:.3} um 공진근처 {}",
        fv.tooth_hz, fv.f1_n, fv.amplitude_um, fv.magnification, fv.sle_um, fv.near_resonance
    );
    let eb = &a.error_budget_um;
    println!(
        "오차예산 휨 {:.2} 마모10분 {:.2} 열(공구) {:.2} 열(소재) {:.2} 고정 {:.2} 동적 {:.2} 환경 {:.2} 드리프트 {:.2} 합 {:.2} / 공차 {:.0} um",
        eb.deflection_um, eb.wear_10min_um, eb.thermal_tool_um, eb.thermal_workpiece_um, eb.fixture_um, eb.dynamic_um, eb.environment_um, eb.drift_um, eb.total_um, eb.tolerance_um
    );
    let sf = &a.surface;
    println!(
        "표면 Ra {:.3} um 이송흔 {:.3} 최소칩 {:.3} 채터 x{:.2} 스캘럽 {:.3} | Rz {:.2} 안정 Ra {:.3} 포락선 Rt {:.3} 형성 날 {}/{} 채터 파상도 {:.2} um | 런아웃 과삭 {:.2} um",
        sf.wall_ra_um, sf.feed_mark_um, sf.min_chip_um, 1.0 + sf.chatter_penalty, sf.scallop_um, sf.wall_rz_um, sf.wall_ra_stable_um, sf.envelope_rt_um, sf.forming_teeth, ctx.tool.flutes, sf.chatter_wpv_um, eb.runout_um
    );
    for n in a.notes.iter() {
        println!("  노트: {}", n);
    }

    // ---- 4. 파이프라인 ----
    let t0 = std::time::Instant::now();
    let r = wearcomp::run_pipeline(&p, &pattern_key, &mut sds, None, None, &CompOptions::default(), None, None, &mut |_, _| true).expect("파이프라인");
    let rep = &r.report;
    println!("== 4. 경로 시뮬레이션 (소요 {:.1}s) ==", t0.elapsed().as_secs_f64());
    println!(
        "총 {:.3} min = 절삭 {:.3} + 급이송 {:.3} + 기타 | 제거 {:.4} cm3 | 격자 {:.4} mm | 플런지 {} | 샘플 {} | 빈 {}",
        rep.total_time_min,
        rep.cut_time_min,
        rep.rapid_time_min,
        rep.removed_volume_cm3,
        rep.cell_mm,
        rep.plunge_count,
        rep.samples.len(),
        rep.wear_log.len()
    );
    let feed_mm_s = c.feed_rate_mm_min / 60.0;
    let mrr_prog = rep.removed_volume_cm3 * 1000.0 / (rep.cut_time_min * 60.0).max(1e-9);
    // 판재보다 깊은 절입은 판 두께로 제한된다 (관통 절삭)
    let ap_cross = c.axial_doc_mm.min(w.thickness_mm);
    println!(
        "교차확인1: 제거체적/절삭시간 = {:.2} mm3/s | 이송 {:.3} mm/s 에서 ap {:.2} 이면 평균 ae = {:.3} mm (프리셋 ae {:.2}, 여유 {:.2})",
        mrr_prog,
        feed_mm_s,
        ap_cross,
        mrr_prog / (ap_cross * feed_mm_s).max(1e-9),
        c.radial_doc_mm,
        w.stock_allowance_mm
    );
    println!(
        "교차확인2: 절삭시간 × 이송 = {:.1} mm 의 경로 (이송 구간 합 {:.1} mm, 접선 길이 비 {:.3})",
        rep.cut_time_min * 60.0 * feed_mm_s,
        feed_len,
        rep.cut_time_min * 60.0 * feed_mm_s / feed_len.max(1e-9)
    );
    let au = &rep.engage_audit;
    println!("격자 감사: {}", serde_json::to_string(au).unwrap_or_default());
    if pattern_key == "rect_profile" {
        let ae_geo = w.stock_allowance_mm.max(0.0).min(e.diameter_mm / 2.0);
        let perim = 2.0 * (w.width_mm + w.height_mm);
        println!(
            "rect_profile 기하 기대: ae {:.3} mm · 절삭 길이 약 {:.0} mm · 절삭 시간 약 {:.2} s · 제거 약 {:.0} mm3 | 시뮬: ae(모델) {:.3} ae(체적) {:.3} · 길이 {:.1} · 시간 {:.2} s · 제거 {:.0} mm3 → ae 비 {:.3} 시간 비 {:.3}",
            ae_geo,
            perim,
            perim / feed_mm_s,
            ae_geo * c.axial_doc_mm.min(w.thickness_mm) * perim,
            au.ae_model_mm,
            au.ae_volume_mm,
            au.cut_len_mm,
            au.cut_time_s,
            au.removed_mm3,
            au.ae_model_mm / ae_geo.max(1e-9),
            au.cut_time_s / (perim / feed_mm_s).max(1e-9)
        );
    }
    let cutting: Vec<_> = rep.samples.iter().filter(|s| s.force_n > 0.0).collect();
    if !cutting.is_empty() {
        let n = cutting.len() as f64;
        println!(
            "절삭 샘플(다운샘플 {}개) 평균 ae {:.3} mm ap {:.3} 결합 {:.2}° 힘 {:.1} N (피크 {:.1}) 온도 {:.1} 채터여유 {:.3} 벽휨 {:.3} um",
            cutting.len(),
            cutting.iter().map(|s| s.ae_mm).sum::<f64>() / n,
            cutting.iter().map(|s| s.ap_mm).sum::<f64>() / n,
            cutting.iter().map(|s| s.engage_deg).sum::<f64>() / n,
            cutting.iter().map(|s| s.force_n).sum::<f64>() / n,
            cutting.iter().map(|s| s.force_peak_n).sum::<f64>() / n,
            cutting.iter().map(|s| s.temp_c).sum::<f64>() / n,
            cutting.iter().map(|s| s.chatter_margin).sum::<f64>() / n,
            cutting.iter().map(|s| s.wall_defl_um.abs()).sum::<f64>() / n
        );
    }
    for (i, b) in rep.wear_log.iter().enumerate() {
        if i < 3 || i + 2 >= rep.wear_log.len() {
            println!(
                "  빈{:>3} t_cut {:7.3} dt {:.4} 제거 {:.3} mm3 F {:.1} T {:.1} h {:.2} um eng {:.2}° chat {:.3} VB {:.7} mm",
                i, b.t_cut_s, b.dt_s, b.removed_mm3, b.force_n, b.temp_c, b.h_um, b.engage_deg, b.chatter_min, b.vb_mm
            );
        }
    }
    for wmsg in rep.warnings.iter() {
        println!("  경고: {}", wmsg);
    }

    // ---- 5. 마모 ----
    let d = &r.outcome.draft;
    let vb_rate_job = if rep.cut_time_min > 1e-9 { (rep.vb_end_mm - rep.vb_start_mm) / rep.cut_time_min } else { 0.0 };
    println!("== 5. 마모 ==");
    println!(
        "경로 VB {:.5} -> {:.5} mm / {:.3} min = {:.3} um/min | 공칭 {:.3} um/min (kappa x채터 {:.3} 반영 전) | 비 x{:.3}",
        rep.vb_start_mm,
        rep.vb_end_mm,
        rep.cut_time_min,
        vb_rate_job * 1000.0,
        a.wear.vb_rate_mm_per_min * 1000.0,
        a.wear.chatter_factor,
        vb_rate_job / a.wear.vb_rate_mm_per_min.max(1e-12)
    );
    println!(
        "경로 채터 마모 평균 x{:.3} (새 공구 x{:.3}) 확률 {:.0}% | 성장시간 {:.3} min | 성장(보정대상) {:.6} 고정(길들이기) {:.6} mm",
        rep.chatter_wear_factor,
        rep.chatter_wear_factor_fresh,
        rep.chatter_probability * 100.0,
        rep.growth_cut_min,
        rep.growth_calib_mm,
        rep.growth_fixed_mm
    );
    let pt = &d.path_trajectory;
    println!(
        "경로 계수 path_factor k = {:.4} | 경로 궤적 수명 {:.1} min (선형 {:.1}) 길들이기 VB {:.4} tau {:.3} min | 공칭 궤적 수명 {:.1} min",
        d.path_factor, pt.life_min, pt.linear_life_min, pt.break_in_vb_mm, pt.break_in_tau_min, a.trajectory.life_min
    );
    let path_part_min = rep.cut_time_min;
    let rem = pt.minutes_between(r.outcome.vb_after_mm, d.vb_limit_mm);
    println!(
        "교체 개수 검산: 남은 {:.1} min / 부품당 {:.4} min = {:.0}개 | 로그 마모율 기준 {:.1} min → {:.0}개",
        rem,
        path_part_min,
        rem / path_part_min.max(1e-9),
        (d.vb_limit_mm - r.outcome.vb_after_mm) / vb_rate_job.max(1e-12),
        (d.vb_limit_mm - r.outcome.vb_after_mm) / vb_rate_job.max(1e-12) / path_part_min.max(1e-9)
    );
    println!(
        "결과: parts_left p50={:?} p90={:?} | 출처: {}",
        r.outcome.parts_left_p50, r.outcome.parts_left_p90, r.outcome.parts_source
    );
    if let Some(life) = r.log.life.as_ref() {
        println!(
            "life: {} 이력 {}점 t_now {:.3} min vb_now {:.5} 지평 {:.3} min cross {:?}/{:?}",
            life.method, life.history_n, life.t_now_min, life.vb_now_mm, life.horizon_min, life.cross_p50_min, life.cross_p90_min
        );
    }
    println!("per_part_vb p50={:?} p90={:?}", r.log.per_part_vb_p50_mm, r.log.per_part_vb_p90_mm);

    // ---- 6. 판정 ----
    println!("== 6. 보정 판정 ==");
    println!("{}", r.outcome.report_line);
    println!(
        "잔여 P10/P50/P90 {:.2}/{:.2}/{:.2} um 최대 {:.2} 최소 {:.2} (공차 {:.0}) n={} 영역 {}",
        d.residual.p10_um, d.residual.p50_um, d.residual.p90_um, d.residual.max_um, d.residual.min_um, d.residual.tolerance_um, d.residual.n, d.residual.zones
    );
    for z in d.zones.iter() {
        println!(
            "  영역 seg{} 길이 {:.1} ap {:.2} {} 벽 {:.2} mm 잔여 P50 {:.2} (휨 {:.2} 벽 {:.2} 마모 {:.2} 열 {:.2}) 채터(실행/신품) {:.3}/{:.3}",
            z.seg, z.len_mm, z.ap_mm, if z.down { "다운" } else { "업" }, z.wall_mm, z.residual_p50_um, z.cause_defl_um, z.cause_wall_um, z.cause_wear_um, z.cause_thermal_um, z.execute.chatter, z.fresh.chatter
        );
    }
    for g in d.gates.iter() {
        println!("  게이트 {:<16} 등급 {} 값 {:.3} / 한계 {:.3} {} — {}", g.id, g.severity, g.value, g.limit, g.unit, g.message);
    }
    for n in d.notes.iter() {
        println!("  초안 노트: {}", n);
    }
    for n in r.outcome.notes.iter() {
        println!("  결과 노트: {}", n);
    }
    for n in r.log.notes.iter() {
        println!("  로그 노트: {}", n);
    }
    println!("== 7. 감사 요약 (simaudit — wear_logs JSON 의 audit 키와 같은 내용) ==");
    for l in endmill_model::simaudit::lines(&r.audit) {
        println!("{}", l);
    }
    let _ = std::fs::remove_dir_all(&dir);
}
