//! 벽면 오차 진단 — 벽면이 만들어지는 순간(업밀링은 진입, 다운밀링은 이탈)에 어느 슬라이스가 물려 있고 법선 방향 힘이
//! 얼마인지, 그 힘이 공구 끝을 얼마나 휘게 하는지를 슬라이스별로 나열한다. 런아웃 해석(TIR 전체 / TIR의 절반)에 따른 차이도 같이 본다.
//!
//!   cargo run --features cuda --example wall_probe -- <프리셋 번호> [ae mm]
use endmill_model::loadsim;
use endmill_model::physics::{self, CutContext, Engagement};
use endmill_model::profile::{MachiningPreset, MachiningProfile};

fn main() {
    let presets = MachiningPreset::builtin();
    let idx: usize = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    let pr = presets[idx].clone();
    let p = MachiningProfile::from_preset(&pr, &pr.name);
    let ctx = CutContext::from_profile(&p);
    let c = &p.conditions;
    let d = ctx.tool.diameter_mm;
    let rpm = c.spindle_rpm as f64;
    let z = ctx.tool.flutes.max(1) as f64;
    let fz = c.feed_rate_mm_min / (rpm * z);
    let ap = c.axial_doc_mm.min(ctx.tool.loc_mm);
    let ae: f64 = std::env::args().nth(2).and_then(|s| s.parse().ok()).unwrap_or_else(|| p.workpiece_setup.stock_allowance_mm.min(d / 2.0));
    let vce = physics::effective_vc(&ctx.tool, rpm, ap);
    let co = physics::cutting_coeffs_at(&ctx, vce, (fz * 0.64).max(1e-4));
    println!("{} | D{} Z{} helix {}° stickout {:.1} LOC {} | fz {:.4} ap {:.2} ae {:.3} rpm {} | runout 입력 {} µm", p.name, d, ctx.tool.flutes, ctx.tool.helix_deg, ctx.tool.stickout_mm, ctx.tool.loc_mm, fz, ap, ae, rpm, ctx.tool.runout_um);
    let comp = physics::tool_compliance(&ctx.tool, ap, ctx.calib.deflection);
    println!("공구 끝 강성 {:.3} N/µm (하중점 {:.1} mm), 홀더 {} N/µm", comp.stiffness_n_per_um, comp.load_point_from_holder_mm, ctx.tool.holder_stiffness_n_per_um);
    let tan_h = ctx.tool.helix_deg.to_radians().tan();
    let r = d / 2.0;
    for (label, down) in [("업밀링(스톡이 진행 방향 왼쪽)", false), ("다운밀링", true)] {
        for (rname, runout) in [("런아웃 그대로", ctx.tool.runout_um), ("런아웃 0", 0.0), ("런아웃 절반", 0.5 * ctx.tool.runout_um)] {
            let mut t = ctx.tool.clone();
            t.runout_um = runout;
            let eng = Engagement::side(ap, ae, d, down);
            let f = physics::mechanistic_forces(&t, &co, fz, rpm, &eng, 48, 10);
            let (defl, force) = loadsim::wall_deflection(&t, ctx.calib.deflection, &f, &eng).unwrap_or((0.0, 0.0));
            println!(
                "== {} / {} : 평균 힘 {:.1} N 피크 {:.1} N (x{:.2}) | 벽 순간 평균 법선 힘 {:.2} N → 휨 {:.2} µm | 결합 {:.1}°..{:.1}°",
                label, rname, f.f_res_mean, f.f_res_peak, f.f_res_peak / f.f_res_mean.max(1e-9), force, defl, eng.phi_st.to_degrees(), eng.phi_ex.to_degrees()
            );
            if rname == "런아웃 그대로" {
                let steps = f.fy_series.len();
                let pitch = f.pitch_rad;
                let wall = eng.wall_angle().unwrap_or(0.0);
                for k in 0..4 {
                    let zk = (k as f64 + 0.5) * ap / 4.0;
                    let lag = zk * tan_h / r;
                    let theta = (wall + lag).rem_euclid(pitch);
                    let i = ((theta / pitch) * steps as f64).floor() as usize % steps;
                    let fy = f.fy_series[i];
                    let fx = f.fx_series[i];
                    println!("   슬라이스 z={:.2} mm 지연 {:.1}° → 회전 {:.1}° (인덱스 {}/{}): Fx {:.2} Fy {:.2} N", zk, lag.to_degrees(), theta.to_degrees(), i, steps, fx, fy);
                }
                let line: Vec<String> = (0..steps).step_by((steps / 12).max(1)).map(|i| format!("{:.0}°:{:.1}", (i as f64 / steps as f64 * pitch).to_degrees(), f.fy_series[i])).collect();
                println!("   Fy(회전각) {}", line.join("  "));
            }
        }
    }
}
