//! 물리(MPM) 교차검증을 앱 없이 돌려 본다.
//!
//!   cargo run --release --example physcal_probe -- stainless quick
//!   cargo run --release --example physcal_probe -- aluminum standard 60
//!
//! 인자: [재료] [quick|standard] [칩 두께 µm] [--plan] [--threads N]
//!   --plan: 해석은 하지 않고 규모·예상 시간만 출력
//! 재료: aluminum | carbon | stainless | titanium | inconel | alloy40 | alloy55 (기본 stainless)
//! 같은 조건에서 앱의 해석식과 MPM 의 절삭력·접촉면 온도를 맞대고, 보정 계수와 신뢰도를 출력한다.
//! 이 결과는 실험으로 검증되지 않았다.
use endmill_model::physcal::{self, PhysRunOptions};
use endmill_model::profile::{MachiningPreset, MachiningProfile};
use endmill_model::WorkpieceMaterial;
use std::io::Write;

fn main() {
    let all: Vec<String> = std::env::args().skip(1).collect();
    let plan_only = all.iter().any(|a| a == "--plan");
    let mut threads: Option<usize> = None;
    let mut args: Vec<String> = Vec::new();
    let mut it = all.into_iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--plan" => {}
            "--threads" => threads = it.next().and_then(|v| v.parse::<usize>().ok()),
            _ => args.push(a),
        }
    }
    let mat = args.first().map(|s| s.as_str()).unwrap_or("stainless");
    let preset = args.get(1).cloned().unwrap_or_else(|| "quick".into());
    let h_um = args.get(2).and_then(|s| s.parse::<f64>().ok());
    let material = match mat {
        "aluminum" | "al" => WorkpieceMaterial::Aluminum,
        "carbon" | "s45c" => WorkpieceMaterial::CarbonSteel,
        "stainless" | "sus304" => WorkpieceMaterial::StainlessSteel,
        "titanium" | "ti" => WorkpieceMaterial::Titanium,
        "inconel" => WorkpieceMaterial::Inconel,
        "alloy40" => WorkpieceMaterial::AlloySteel { hardness_hrc: 40 },
        "alloy55" => WorkpieceMaterial::AlloySteel { hardness_hrc: 55 },
        other => {
            eprintln!("알 수 없는 재료: {}", other);
            std::process::exit(2);
        }
    };
    let mut p = MachiningProfile::from_preset(&MachiningPreset::default_steel_general(), "probe");
    if let WorkpieceMaterial::AlloySteel { hardness_hrc } = &material {
        p.workpiece_setup.hardness_hrc = Some(*hardness_hrc);
    }
    p.workpiece_setup.material = material;
    let opt = PhysRunOptions { preset: Some(preset.clone()), h_um, threads };
    let pv = match physcal::preview(&p, &opt) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("계획 실패: {}", e);
            std::process::exit(1);
        }
    };
    println!(
        "조건: {} · 공구 {} · 가공 평균 칩 두께 {:.1} µm → 해석 {:.0} µm · Vc {:.0} m/min · 경사각 {:.1}° · 날 둥글기 {:.1} µm · μ {:.2}",
        pv.cond.material_label, pv.cond.tool_signature, pv.cond.h_job_um, pv.cond.h_um, pv.cond.vc_m_min, pv.cond.rake_deg, pv.cond.edge_um, pv.cond.mu
    );
    println!(
        "규모: 입자 {} · 단계 {} · 격자 {:.1} µm · 질량 ×{:.0} · 스레드 {} · 예상 {:.0}초",
        pv.particles, pv.steps, pv.cond.dx_um, pv.cond.mscale, pv.cond.threads, pv.est_seconds
    );
    for w in &pv.warnings {
        println!("주의: {}", w);
    }
    if plan_only {
        return;
    }
    let plan = physcal::build_plan(&p, &opt).expect("plan");
    let t0 = std::time::Instant::now();
    let last = std::sync::Mutex::new(-1.0f64);
    let rec = physcal::run_plan(&plan, &|f, msg| {
        let mut l = last.lock().unwrap();
        if f - *l >= 0.1 {
            *l = f;
            eprintln!("[MPM] {:>3.0}% {} (경과 {:.0}s)", f * 100.0, msg, t0.elapsed().as_secs_f64());
        }
        true
    });
    let rec = match rec {
        Ok(r) => r,
        Err(e) => {
            eprintln!("실행 실패: {}", e);
            std::process::exit(1);
        }
    };
    let a = &rec.analytic;
    let m = &rec.mpm;
    println!();
    println!("{:<28}{:>14}{:>14}{:>10}", "항목", "앱 해석식", "MPM", "MPM/해석식");
    println!("{:<28}{:>14.1}{:>14.1}{:>10.2}", "절삭방향 힘 Fc (N/mm)", a.fc_n_per_mm, m.fc_n_per_mm, rec.raw_power);
    println!("{:<28}{:>14.1}{:>14.1}{:>10.2}", "반경(추력) 힘 Ft (N/mm)", a.ft_n_per_mm, m.ft_n_per_mm, m.ft_n_per_mm / a.ft_n_per_mm.max(1e-9));
    match m.interface_rise_c {
        Some(r) => println!("{:<28}{:>14.0}{:>14.0}{:>10.2}", "접촉면 온도 상승 (K, 냉각 전)", a.rise_c, r, rec.raw_temperature),
        None => println!("{:<28}{:>14.0}{:>14}{:>10}", "접촉면 온도 상승 (K, 냉각 전)", a.rise_c, "—", "—"),
    }
    println!(
        "MPM 칩 두께비 {} · 전단각 {} · 겉보기 마찰 {} · 소성일/공구일 {:.2} · 운동에너지 비 {:.3} · 정상상태 {} · 소요 {:.0}초",
        m.chip_ratio.map(|v| format!("{:.2}", v)).unwrap_or("—".into()),
        m.shear_angle_deg.map(|v| format!("{:.1}°", v)).unwrap_or("—".into()),
        m.mu_apparent.map(|v| format!("{:.2}", v)).unwrap_or("—".into()),
        m.wp_over_wtool,
        m.ke_ratio_max,
        m.steady,
        m.wall_s
    );
    println!();
    println!("보정 계수(가중치 적용 전): power ×{:.3} · temperature ×{:.3} (동력 보정 적용 뒤 잔차)", rec.ratio_power, rec.ratio_temperature);
    println!("기본 가중치 {:.2} → 실제 적용값 power ×{:.3} · temperature ×{:.3} (켰을 때)", rec.weight, 1.0 + rec.weight * (rec.ratio_power - 1.0), 1.0 + rec.weight * (rec.ratio_temperature - 1.0));
    println!("신뢰도: {}", rec.reliability.level);
    for r in &rec.reliability.reasons {
        println!("  - {}", r);
    }
    if let Ok(mut f) = std::fs::File::create(format!("physcal_probe_{}_{}.json", mat, preset)) {
        let _ = f.write_all(serde_json::to_string_pretty(&rec).unwrap_or_default().as_bytes());
        println!("\n기록: physcal_probe_{}_{}.json", mat, preset);
    }
}
