//! 같은 프리셋에서 코팅만 바꿔 마찰·온도·마모 기구를 나란히 찍는다 (조합 기준값 점검용).
//! cargo run --features cuda --example pair_probe -- <프리셋 이름 일부> <코팅,코팅,...>
use endmill_model::physics::{self, CutContext, ThermalBody};
use endmill_model::profile::{MachiningPreset, MachiningProfile};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let name = args.get(1).cloned().unwrap_or_else(|| "티타늄".into());
    let coats: Vec<String> = args.get(2).map(|s| s.split(',').map(|c| c.trim().to_string()).collect()).unwrap_or_else(|| vec!["".into(), "TiN".into(), "AlTiN".into(), "nACo".into()]);
    let preset = MachiningPreset::builtin().into_iter().find(|p| p.name.contains(&name)).expect("preset");
    let base = MachiningProfile::from_preset(&preset, &preset.name);
    println!("{} | Vc {:.0} fz {:.4} ap {:.1} ae {:.2} | {:?}", base.name, base.conditions.cutting_speed_m_min, base.conditions.feed_per_tooth_mm, base.conditions.axial_doc_mm, base.conditions.radial_doc_mm, base.coolant_config.method);
    for c in coats.iter() {
        let mut p = base.clone();
        p.endmill_setting.coating_name = if c.is_empty() { None } else { Some(c.clone()) };
        let ctx = CutContext::from_profile(&p);
        let body = ThermalBody::of_setup(&p.workpiece_setup, &ctx.wp);
        let a = physics::analyze_cut(&ctx, &p.conditions, true, body);
        let th = &a.thermal;
        let w = &a.wear;
        let mech: Vec<String> = w.mechanisms.iter().map(|m| format!("{} {:.0}% (×{:.2})", m.key, m.share * 100.0, m.ratio)).collect();
        println!(
            "{:<8} μ {:.3} kc11 {:.0} | 계면 건식 {:.0} 냉각 {:.0} °C 장벽 {:.3} | 수명 {:.1} min (rate {:.3} µm/min) 코팅 {:.2} 온도 {:.2} 응력 {:.2} 기구합 {:.3} 열균열 {:.2} | {}",
            if c.is_empty() { "uncoated" } else { c.as_str() },
            a.coeffs.mu,
            a.coeffs.kc11,
            th.interface_dry_c,
            th.interface_c,
            th.barrier.interface,
            w.tool_life_min,
            w.vb_rate_mm_per_min * 1000.0,
            w.coating_factor,
            w.temperature_factor,
            w.stress_factor,
            w.mechanism_factor,
            th.thermal_crack_risk,
            mech.join(", ")
        );
    }
}
