//! 기본 프리셋을 포켓 지그재그 경로로 1차 시뮬레이션해 마모·온도·예열·채터·벽 응력을 한 줄씩 요약한다 (점검용).
use endmill_model::gcode::ToolPathPattern;
use endmill_model::loadsim;
use endmill_model::physics::{self, CutContext, ThermalBody};
use endmill_model::profile::{MachiningPreset, MachiningProfile};

fn main() {
    let only: Option<String> = std::env::args().nth(1);
    for pr in MachiningPreset::builtin() {
        if let Some(o) = &only {
            if !pr.name.contains(o.as_str()) {
                continue;
            }
        }
        // 앱이 라이브러리에 저장하는 것과 같은 조건 (calc::recommend_preset — 용도별 조건 추천)
        let tuned = endmill_model::calc::recommend_preset(&pr, &endmill_model::profile::ShopEnvironment::standard());
        let p = MachiningProfile::from_preset(&tuned, &pr.name);
        let ctx = CutContext::from_profile(&p);
        let body = ThermalBody::of_setup(&p.workpiece_setup, &ctx.wp);
        let a = physics::analyze_cut(&ctx, &p.conditions, true, body);
        let c = &p.conditions;
        println!("  [{}] Vc {:.0} rpm {} fz {:.4} ap {:.2} ae {:.2}", pr.name, c.cutting_speed_m_min, c.spindle_rpm, c.feed_per_tooth_mm, c.axial_doc_mm, c.radial_doc_mm);
        let t0 = std::time::Instant::now();
        let r = loadsim::simulate(&p, &ToolPathPattern::PocketZigZag, &ctx);
        let vb_rate = if r.cut_time_min > 1e-9 { (r.vb_end_mm - r.vb_start_mm) / r.cut_time_min } else { 0.0 };
        let sigma_y = ctx.wp.flow_stress(0.0, 1.0, 20.0).max(1.0);
        let yr = r.seg_walls.iter().map(|s| s.wall_stress_mpa / sigma_y).fold(0.0, f64::max);
        println!(
            "{:<16} cut {:5.1} min VB {:.3} (rate {:.4} vs nominal {:.4} = x{:.1}, life nominal {:.0} min) Tmax {:4.0} C preheat {:4.0} K wp_end {:4.0} C | chatter frac {:.2} min margin {:.2} wear x{:.2} | wall {:?} mm yield x{:.1} | {:.1}s",
            pr.name,
            r.cut_time_min,
            r.vb_end_mm,
            vb_rate,
            a.wear.vb_rate_mm_per_min,
            vb_rate / a.wear.vb_rate_mm_per_min.max(1e-9),
            a.wear.tool_life_min,
            r.max_temp_c,
            r.max_preheat_c,
            r.wp_temp_end_c,
            r.chatter_fraction,
            r.min_chatter_margin,
            r.chatter_wear_factor,
            r.thin_wall_mm.map(|v| (v * 100.0).round() / 100.0),
            yr,
            t0.elapsed().as_secs_f64()
        );
    }
}
