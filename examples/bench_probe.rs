//! 문헌 벤치마크 칸 하나의 속을 들여다본다 — 공칭 해석의 온도·힘·마모 기구 몫·기준 상태 대비 배수를 찍는다 (진단용).
//!
//!   cargo run --example bench_probe -- <공구 key> <조건 key> [Vc 배수]
//!
//! 공구·조건 key 는 `src/benchmark.rs` 의 `paper_tools` · `sandbox_tools` · `paper_cuts`
use endmill_model::benchmark::{self, Param};
use endmill_model::physics::{self, CutContext, ThermalBody};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut tools = benchmark::paper_tools();
    tools.extend(benchmark::sandbox_tools());
    let cuts = benchmark::paper_cuts();
    if args.len() < 2 {
        println!("공구: {}", tools.iter().map(|t| t.key).collect::<Vec<_>>().join(" "));
        println!("조건: {}", cuts.iter().map(|c| c.key).collect::<Vec<_>>().join(" "));
        return;
    }
    let t = tools.iter().find(|t| t.key == args[0]).expect("공구 key");
    let mut cut = cuts.iter().find(|c| c.key == args[1]).expect("조건 key").clone();
    // "cool:dry|flood|mist|air|tt" 는 그 조건의 냉각만 바꾼다
    for a in args[2..].iter().filter_map(|s| s.strip_prefix("cool:")) {
        if let Some(m) = endmill_model::profile::CoolantMethod::from_key(a) {
            cut.coolant = m;
        }
    }
    let c = &cut;
    // 배수: "1.5" (Vc) 또는 "fz:2.2" · "ae:0.5" · "ap:2" · "vc:1.5"
    let parse = |s: &String| -> Option<(Param, f64)> {
        let (p, v) = match s.split_once(':') {
            Some((k, v)) => (
                match k {
                    "fz" => Param::Fz,
                    "ae" => Param::Ae,
                    "ap" => Param::Ap,
                    _ => Param::Vc,
                },
                v,
            ),
            None => (Param::Vc, s.as_str()),
        };
        v.parse().ok().map(|x| (p, x))
    };
    let mut scales: Vec<(Param, f64)> = args[2..].iter().filter(|s| !s.starts_with("cool:")).filter_map(parse).collect();
    if scales.is_empty() {
        scales.push((Param::Vc, 1.0));
    }
    for (param, k) in scales {
        let p = benchmark::cell_profile(t, c, &[(param, k)]);
        let mut ctx = CutContext::from_profile(&p);
        ctx.wp.vb_criterion_mm = c.vb_criterion_mm;
        let body = ThermalBody::of_setup(&p.workpiece_setup, &ctx.wp);
        let a = physics::analyze_cut(&ctx, &p.conditions, c.down, body);
        let r = physics::reference_state(&ctx);
        let pr = benchmark::predict(t, c, &[(param, k)]);
        println!("==== {} × {} · {} ×{:.2} ({})", t.key, c.key, param.key(), k, ctx.wp.label);
        println!(
            "  공구 Ø{:.1} z{} 경사 {:.1}° 여유 {:.1}° 날끝 {:.1} µm 코팅 {} 모재 {} · 적합도 마모 ×{:.2}",
            ctx.tool.diameter_mm, ctx.tool.flutes, ctx.tool.rake_deg, ctx.tool.clearance_deg, ctx.tool.edge_radius_um, ctx.tool.coating.family, ctx.tool.substrate, ctx.tool.fit_wear
        );
        println!(
            "  Vc {:.1} (유효 {:.1}) rpm {:.0} fz {:.4} ap {:.2} ae {:.2} · h_mean {:.4} h_max {:.4} · 물림 {:.0}° · 냉각 {:?}",
            a.vc_m_min, a.vc_effective_m_min, a.rpm, a.fz_mm, a.ap_mm, a.ae_mm, a.forces.h_mean_mm, a.forces.h_max_mm, a.engage_angle_deg, ctx.coolant.method
        );
        println!(
            "  kc11 {:.0} (연화 {:.3}) μ {:.3} φ {:.1}° kr {:.3} kte {:.1} · 합력 평균 {:.1} N · 동력 {:.3} kW · 비에너지 {:.2} J/mm³",
            a.coeffs.kc11,
            a.coeffs.softening,
            a.coeffs.mu,
            a.coeffs.shear_angle_deg,
            a.coeffs.kr,
            a.coeffs.kte,
            a.forces.f_res_mean,
            a.forces.cutting_power_kw,
            a.forces.cutting_power_kw * 60.0 / a.mrr_cm3_min.max(1e-9)
        );
        let (rise_s, beta) = physics::shear_zone(&ctx, &a.coeffs, a.vc_effective_m_min, a.forces.h_mean_mm);
        println!(
            "  계면 건식 {:.0} → {:.0} °C (냉각 효과 {:.2}, 과도 {:.2}) · 전단면 상승 {:.0} K (β {:.2}) · 기준 상태 계면 {:.0} °C (Vc {:.0})",
            a.thermal.interface_dry_c,
            a.thermal.interface_c,
            a.thermal.cooling.effectiveness,
            a.thermal.transient_factor,
            rise_s,
            beta,
            r.t_c,
            r.vc
        );
        println!(
            "  마모: 응력 ×{:.2} 미끄럼 ×{:.2} 온도 ×{:.2} 기구 합 ×{:.3} 런아웃 ×{:.2} 배출 ×{:.2} · VB 속도 {:.2} µm/min · 수명 {:.2} min (채터 제외 {:.2}) · 지배 {}",
            a.wear.stress_factor,
            a.wear.speed_factor,
            a.wear.temperature_factor,
            a.wear.mechanism_factor,
            a.wear.runout_factor,
            a.wear.evacuation_factor,
            pr.vb_rate_um_min,
            pr.life_min,
            pr.life_no_chatter_min,
            a.wear.dominant
        );
        for m in a.wear.mechanisms.iter() {
            println!("    {:<10} 몫 {:.3} · 기준 대비 ×{:.3}", m.key, m.share, m.ratio);
        }
        let (rt, rfz, reng, rrpm, rvc) = physics::reference_thermal(&ctx, None);
        let (rd, ..) = physics::reference_thermal(&ctx, Some(&endmill_model::profile::CoolantConfig::dry()));
        println!(
            "  기준 상태: Ø10 4날 AlTiN · Vc {:.0} rpm {:.0} fz {:.4} ap {:.1} ae {:.1} · 냉각 {:?} 계면 {:.0} °C (건식 {:.0}, 냉각 효과 {:.2}) · 건식으로 바꾸면 {:.0} °C",
            rvc,
            rrpm,
            rfz,
            reng.ap_mm,
            reng.ae_mm,
            ctx.wp.reference_coolant().method,
            rt.interface_c,
            rt.interface_dry_c,
            rt.cooling.effectiveness,
            rd.interface_c
        );
        println!(
            "  궤적: 선형 수명 {:.2} · 길들이기 {:.0} µm / {:.2} min · κ {:.3} · 가속 ×{:.2} · 채터 여유 {:.2} (×{:.2})",
            a.trajectory.linear_life_min,
            a.trajectory.break_in_vb_mm * 1000.0,
            a.trajectory.break_in_tau_min,
            a.trajectory.kappa,
            a.trajectory.acceleration,
            a.stability.margin,
            a.wear.chatter_factor
        );
        let tr = &pr.trajectory;
        let life = tr.time_at(c.vb_criterion_mm).max(1e-6);
        let pts: Vec<String> = [0.05, 0.1, 0.25, 0.5, 0.75, 1.0, 1.25]
            .iter()
            .map(|f| format!("{:.2}·T {:.0} µm", f, tr.vb_at(f * life) * 1000.0))
            .collect();
        println!("  VB 궤적 (T = {:.2} min): {}", life, pts.join(" · "));
        // 궤적 표본점의 VB 별 계면 온도·합력 (마모 랜드 → 발열 되먹임)
        let temps: Vec<String> = tr
            .vb_mm
            .iter()
            .zip(tr.temp_c.iter())
            .zip(tr.force_n.iter())
            .step_by((tr.vb_mm.len() / 8).max(1))
            .map(|((v, t), f)| format!("{:.0} µm {:.0} °C {:.0} N", v * 1000.0, t, f))
            .collect();
        println!("  VB 별 계면·합력: {}", temps.join(" · "));
    }
}
