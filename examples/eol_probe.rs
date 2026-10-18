//! 공구 수명 말기 점검: 프리셋 × 경로 × 수명 정책별로 수명 판정·교체·파손·정지가 어디서 일어나는지 찍는다.
//!
//!   cargo run --features cuda --example eol_probe -- [패턴,패턴,...] [정책,정책,...] [프리셋 이름 일부]
//!
//! 정책 키: stop · change_tool · run_to_failure (기본 셋 다). 패턴 기본값: rect_profile,pocket_zigzag,slot
use endmill_model::physics::CutContext;
use endmill_model::profile::{MachiningPreset, MachiningProfile, ToolLifePolicy};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let patterns: Vec<String> = args.get(1).map(|s| s.split(',').map(|x| x.to_string()).collect()).unwrap_or_else(|| vec!["rect_profile".into(), "pocket_zigzag".into(), "slot".into()]);
    let policies: Vec<ToolLifePolicy> = args
        .get(2)
        .map(|s| s.split(',').filter_map(ToolLifePolicy::from_key).collect())
        .unwrap_or_else(|| vec![ToolLifePolicy::Stop, ToolLifePolicy::ChangeTool, ToolLifePolicy::RunToFailure]);
    let filter = args.get(3).cloned();
    for preset in MachiningPreset::builtin() {
        if let Some(f) = filter.as_ref() {
            if !preset.name.contains(f.as_str()) {
                continue;
            }
        }
        for pk in patterns.iter() {
            for pol in policies.iter() {
                let mut p = MachiningProfile::from_preset(&preset, &preset.name);
                p.machine.tool_life_policy = *pol;
                let ctx = CutContext::from_profile(&p);
                let limit = ctx.vb_limit_mm();
                let r = match endmill_model::viewport::build_report_prog(&p, pk, &ctx, &mut |_| true) {
                    Ok(v) => v.1,
                    Err(e) => {
                        println!("[EOL] {} | {} | {} | ERR {}", p.name, pk, pol.key(), e);
                        continue;
                    }
                };
                let tl = &r.tool_life;
                println!(
                    "[EOL] {} | {} | {} | 상태 {} · 공구 {}개 (교체 {}) · 완료 {:.0}% · 절삭 {:.2} min 제거 {:.1} cm3 · VB {:.3}->{:.3} (한계 {:.2}) 수명 최대 {:.0}% · 응력 최대 {:.0} MPa (×{:.2}, {}) · 피로 {:.3} · 치핑 P {:.0}% (평균 배수 채터 {:.1} 응력 {:.1} 경도 {:.1} 열 {:.1} 초과 {:.1} 적합 {:.1}) · 3차 최대 ×{:.1}",
                    p.name,
                    pk,
                    pol.key(),
                    tl.state,
                    tl.tools_used,
                    tl.tool_changes,
                    tl.completed_fraction() * 100.0,
                    r.cut_time_min,
                    r.removed_volume_cm3,
                    r.vb_start_mm,
                    r.vb_end_mm,
                    limit,
                    tl.life_used_max * 100.0,
                    tl.peak_stress_mpa,
                    tl.peak_stress_ratio,
                    tl.critical_section,
                    tl.fatigue_damage,
                    tl.chip_probability * 100.0,
                    tl.chip_factors_mean.chatter,
                    tl.chip_factors_mean.stress,
                    tl.chip_factors_mean.hardness,
                    tl.chip_factors_mean.thermal,
                    tl.chip_factors_mean.worn,
                    tl.chip_factors_mean.fit,
                    tl.accel_factor_max
                );
                for e in tl.events.iter() {
                    println!("[EOL]    · {:<11} #{} t {:7.1} s (절삭 {:6.1} s) seg {:3} | {}", e.kind, e.tool_no, e.t_s, e.t_cut_s, e.seg, e.message);
                }
            }
        }
    }
}
