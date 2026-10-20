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
    let c = cuts.iter().find(|c| c.key == args[1]).expect("조건 key");
    let scales: Vec<f64> = if args.len() > 2 { args[2..].iter().filter_map(|s| s.parse().ok()).collect() } else { vec![1.0] };
    for k in scales {
        let p = benchmark::cell_profile(t, c, &[(Param::Vc, k)]);
        let mut ctx = CutContext::from_profile(&p);
        ctx.wp.vb_criterion_mm = c.vb_criterion_mm;
        let body = ThermalBody::of_setup(&p.workpiece_setup, &ctx.wp);
        let a = physics::analyze_cut(&ctx, &p.conditions, c.down, body);
        let r = physics::reference_state(&ctx);
        let pr = benchmark::predict(t, c, &[(Param::Vc, k)]);
        println!("==== {} × {} · Vc ×{:.2} ({})", t.key, c.key, k, ctx.wp.label);
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
    }
}
