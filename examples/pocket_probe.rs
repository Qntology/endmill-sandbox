//! 포켓 경로 점검: 내장 프리셋마다 한 경로(기본 pocket_zigzag)를 수명 정책별로 돌려 최종 벽 오차 · 절삭 시간 · 공구 수명 결과를 한 줄씩 찍는다.
//! 경로를 바꾼 전후를 같은 표로 견주는 용도 (`[POCKET]` 줄은 `|` 로 나뉜 표, `[POCKET-W]` 는 최악 벽 점의 항목별 분해).
//!
//!   cargo run --features cuda --example pocket_probe -- [패턴] [정책,정책,...] [프리셋 이름 일부]
//!
//! 정책 키: stop · change_tool · finish_part · run_to_failure (기본 stop,finish_part)
use endmill_model::gcode::{GCodeGenerator, ToolPathPattern};
use endmill_model::physics::CutContext;
use endmill_model::profile::{MachiningPreset, MachiningProfile, ToolLifePolicy};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let pattern = args.get(1).cloned().unwrap_or_else(|| "pocket_zigzag".into());
    let policies: Vec<ToolLifePolicy> = args
        .get(2)
        .map(|s| s.split(',').filter_map(ToolLifePolicy::from_key).collect())
        .unwrap_or_else(|| vec![ToolLifePolicy::Stop, ToolLifePolicy::FinishPart]);
    let filter = args.get(3).cloned();
    println!("[POCKET] 프리셋 | 정책 | 상태 | 공구 | 완료% | 절삭 min | 전체 min | 제거 cm3 | 최종 벽 점 | 최대 |오차| µm | p90 µm | 평균 µm | 최대 공구 휨 µm | 얇은 벽 휨 최대 µm | 수명 최대% | 응력비 | 치핑% | 원인");
    for preset in MachiningPreset::builtin() {
        if let Some(f) = filter.as_ref() {
            if !preset.name.contains(f.as_str()) {
                continue;
            }
        }
        for pol in policies.iter() {
            let mut p = MachiningProfile::from_preset(&preset, &preset.name);
            p.machine.tool_life_policy = *pol;
            let ctx = CutContext::from_profile(&p);
            if *pol == policies[0] {
                let pat = ToolPathPattern::from_key(&pattern, &p).expect("pattern");
                let meta = GCodeGenerator::generate_metadata_with_pattern(&p, &pat);
                let c = &p.conditions;
                println!(
                    "[POCKET-P] {} | D{} {}날 | ap {:.2} ae {:.2} fz {:.4} F {:.0} | 스톡 {}x{}x{} 여유 {} | G-code 예상 {:.2} min (절삭 {:.2}) · 이송 거리 {:.0} mm · 세그먼트 {}",
                    p.name,
                    p.endmill_setting.diameter_mm,
                    p.endmill_setting.flute_count,
                    c.axial_doc_mm,
                    c.radial_doc_mm,
                    c.feed_per_tooth_mm,
                    c.feed_rate_mm_min,
                    p.workpiece_setup.width_mm,
                    p.workpiece_setup.height_mm,
                    p.workpiece_setup.thickness_mm,
                    p.workpiece_setup.stock_allowance_mm,
                    meta.estimated_time_min,
                    meta.cutting_time_min,
                    meta.cutting_distance_mm,
                    GCodeGenerator::generate_synthetic_segments_with_pattern(&p, &pat).len()
                );
            }
            let (_, r, segs) = match endmill_model::viewport::build_report_prog(&p, &pattern, &ctx, &mut |_| true) {
                Ok(v) => v,
                Err(e) => {
                    println!("[POCKET] {} | {} | ERR {}", p.name, pol.key(), e);
                    continue;
                }
            };
            let tl = &r.tool_life;
            let finals: Vec<&endmill_model::loadsim::WallErrorSample> = r.wall_errors.iter().filter(|w| w.final_wall).collect();
            let mut abs: Vec<f64> = finals.iter().map(|w| w.total_um.abs()).collect();
            abs.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let p90 = if abs.is_empty() { 0.0 } else { abs[((abs.len() - 1) as f64 * 0.9).round() as usize] };
            let max_defl = finals.iter().map(|w| w.deflection_um.abs()).fold(0.0, f64::max);
            let max_thin = finals.iter().map(|w| w.workpiece_um.abs()).fold(0.0, f64::max);
            println!(
                "[POCKET] {} | {} | {} | {} | {:.0} | {:.2} | {:.2} | {:.1} | {} | {:.1} | {:.1} | {:.1} | {:.1} | {:.1} | {:.0} | {:.2} | {:.0} | {}",
                p.name,
                pol.key(),
                tl.state,
                tl.tools_used,
                tl.completed_fraction() * 100.0,
                r.cut_time_min,
                r.total_time_min,
                r.removed_volume_cm3,
                r.final_wall_count,
                r.max_wall_error_um,
                p90,
                r.mean_wall_error_um,
                max_defl,
                max_thin,
                tl.life_used_max * 100.0,
                tl.peak_stress_ratio,
                tl.chip_probability * 100.0,
                if tl.failure_cause.is_empty() { tl.events.iter().rev().find(|e| !e.cause.is_empty()).map(|e| e.cause.clone()).unwrap_or_default() } else { tl.failure_cause.clone() }
            );
            // 최종 벽을 만든 패스: 마지막 플런지(포켓 정삭 하강) 뒤 = 정삭 윤곽
            if let Some(fin) = segs.iter().rposition(|s| matches!(s, endmill_model::gcode::ToolPathSegment::Plunge { .. })) {
                let by_finish = finals.iter().filter(|w| w.seg > fin).count();
                let mut fin_abs: Vec<f64> = finals.iter().filter(|w| w.seg > fin).map(|w| w.total_um.abs()).collect();
                fin_abs.sort_by(|a, b| a.partial_cmp(b).unwrap());
                println!(
                    "[POCKET-F] {} | {} | 최종 벽 {}점 중 정삭 윤곽 {}점 · 그 밖 {}점 · 정삭 벽 최대 {:.1} µm · p50 {:.1} µm · 미절삭 경계 {}점",
                    p.name,
                    pol.key(),
                    finals.len(),
                    by_finish,
                    finals.len() - by_finish,
                    fin_abs.last().cloned().unwrap_or(0.0),
                    if fin_abs.is_empty() { 0.0 } else { fin_abs[fin_abs.len() / 2] },
                    r.uncut_boundary_count
                );
            }
            if let Some(w) = finals.iter().max_by(|a, b| a.total_um.abs().partial_cmp(&b.total_um.abs()).unwrap()) {
                println!(
                    "[POCKET-W] {} | {} | 최악 seg {} ({:.1}, {:.1}, z {:.1}) t {:.1} s ap {:.2} {} | 합 {:.1} = 공구 휨 {:.1} + 얇은 벽 {:.1} (두께 {:.2} mm) + 마모 {:.1} + 공구 열 {:.1} + 소재 열 {:.1} + 런아웃 {:.1}",
                    p.name,
                    pol.key(),
                    w.seg,
                    w.x,
                    w.y,
                    w.z_mid,
                    w.t_s,
                    w.ap_mm,
                    if w.down { "다운" } else { "업" },
                    w.total_um,
                    w.deflection_um,
                    w.workpiece_um,
                    w.wall_mm,
                    w.wear_um,
                    w.thermal_tool_um,
                    w.thermal_wp_um,
                    w.runout_um
                );
            }
            for w in r.warnings.iter().filter(|w| w.contains("슬롯형") || w.contains("얇은 벽") || w.contains("플런지") || w.contains("파손") || w.contains("램프") || w.contains("헬리컬")) {
                println!("[POCKET-N] {} | {} | {}", p.name, pol.key(), w);
            }
        }
    }
}
