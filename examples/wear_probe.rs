//! 포켓 경로의 마모가 어디에 쌓이는지(깊이 띠 vs 깊이를 가리지 않은 스칼라 VB)와 '다음 부품당 VB 증가' 예측이 경로 구성(헬리컬 진입 ·
//! 황삭 줄 · 정삭 윤곽)을 따라가는지 찍는다 — 체제별 마모 몫, 시간 10분위 VB 속도, 예측기 가중, 다음 부품을 실제로 깎은 성장 (docs/pocket-zigzag-path.md 3절).
//!
//!   cargo run --features cuda --example wear_probe -- [ti|steel|al ...]
use endmill_model::physics::CutContext;
use endmill_model::profile::{MachiningPreset, MachiningProfile};
use endmill_model::sds::SdsStore;
use endmill_model::wearcomp;

fn preset(name: &str) -> MachiningProfile {
    let p = MachiningPreset::builtin().into_iter().find(|p| p.name.contains(name)).expect("preset");
    MachiningProfile::from_preset(&p, &p.name)
}

fn main() {
    let which: Vec<String> = std::env::args().skip(1).collect();
    let which = if which.is_empty() { vec!["ti".to_string(), "steel".to_string(), "al".to_string()] } else { which };
    for w in which.iter() {
        let p = match w.as_str() {
            "steel" => preset("탄소강"),
            "al" => preset("알루미늄 황삭"),
            _ => preset("티타늄"),
        };
        let dir = std::env::temp_dir().join(format!("endmill-wear-probe-{}-{}", w, std::process::id()));
        let sds = SdsStore::open(&dir.join("sds"));
        let (ctx0, _) = endmill_model::pipeline::calibrated_context(&p, Some(&sds));
        let ap = p.conditions.axial_doc_mm;
        let (_v, r0, _segs) = endmill_model::viewport::build_report_prog(&p, "pocket_zigzag", &ctx0, &mut |_| true).unwrap();
        let grow = r0.vb_end_mm - r0.vb_start_mm;
        let band = r0.wear_field.band_vb(ap);
        let sum_dvb: f64 = r0.regimes.iter().map(|g| g.dvb_mm).sum();
        let expect_band: f64 = r0.regimes.iter().map(|g| g.dvb_mm * (g.ap_mm / ap).min(1.0)).sum();
        println!(
            "[WEAR] {} · 절삭 {:.2} min · VB {:.4} → {:.4} (성장 {:.4}, 정상 {:.4} + 길들이기 {:.4}) · 띠({:.1}) {:.4} · Σ체제 dvb {:.4} · Σ dvb·ap/ap0 {:.4} · max {:.4}",
            p.name, r0.cut_time_min, r0.vb_start_mm, r0.vb_end_mm, grow, r0.growth_calib_mm, r0.growth_fixed_mm, ap, band, sum_dvb, expect_band, r0.wear_field.max_vb()
        );
        let mut regs = r0.regimes.clone();
        regs.sort_by(|a, b| b.dvb_mm.partial_cmp(&a.dvb_mm).unwrap());
        for g in regs.iter().take(10) {
            println!(
                "[REG]   {:6} ap {:5.2} ae {:5.2} φ {:5.1}–{:5.1} t {:6.1} s dvb {:.5} ({:4.1}%) rate {:6.2} µm/min h {:5.1} µm",
                g.mode, g.ap_mm, g.ae_mm, g.phi_st_deg, g.phi_ex_deg, g.t_cut_s, g.dvb_mm, 100.0 * g.dvb_mm / sum_dvb.max(1e-12), g.vb_rate_um_min, g.h_mean_um
            );
        }
        // 시간 10분위별 VB 성장 · 평균 ap · ae
        let bins = &r0.wear_log;
        let total = bins.last().map(|b| b.t_cut_s).unwrap_or(0.0);
        let mut prev_vb = r0.vb_start_mm;
        for k in 0..10 {
            let (a, b) = (total * k as f64 / 10.0, total * (k + 1) as f64 / 10.0);
            let inb: Vec<_> = bins.iter().filter(|x| x.t_cut_s > a && x.t_cut_s <= b).collect();
            let dt: f64 = inb.iter().map(|x| x.dt_s).sum::<f64>().max(1e-9);
            let vb_end = inb.last().map(|x| x.vb_mm).unwrap_or(prev_vb);
            let apm = inb.iter().map(|x| x.ap_mm * x.dt_s).sum::<f64>() / dt;
            let aem = inb.iter().map(|x| x.ae_mm * x.dt_s).sum::<f64>() / dt;
            let bi = inb.iter().map(|x| x.breakin_um_min * x.dt_s).sum::<f64>() / dt;
            println!(
                "[DEC]   {:2} t {:6.1}–{:6.1} s dVB {:.5} ap {:5.2} ae {:5.2} 속도 {:6.2} µm/min (길들이기 {:5.2}) seg {}–{}",
                k,
                a,
                b,
                vb_end - prev_vb,
                apm,
                aem,
                (vb_end - prev_vb) * 1000.0 / ((b - a) / 60.0).max(1e-9),
                bi,
                inb.first().map(|x| x.seg0).unwrap_or(0),
                inb.last().map(|x| x.seg1).unwrap_or(0)
            );
            prev_vb = vb_end;
        }
        let log0 = wearcomp::analyze_job(&p, &ctx0, &r0, None, &sds);
        for b in log0.backtests.iter() {
            println!("[BT]    {} rmse {:.3} µm bias {:+.3} weight {:.3}", b.method, b.rmse_um, b.bias_um, b.weight);
        }
        let body = endmill_model::physics::ThermalBody::of_setup(&p.workpiece_setup, &ctx0.wp);
        let nominal = endmill_model::physics::analyze_cut(&ctx0, &p.conditions, true, body);
        let (k, traj) = wearcomp::path_wear(&nominal.trajectory, &r0);
        let phys_part = traj.growth_from(r0.vb_end_mm, r0.cut_time_min);
        // 예전 부품당 예측: 작업 끝 구간의 융합 예측 기울기 × 작업 시간
        let total_s = log0.t_cut_s.last().cloned().unwrap_or(0.0);
        let tail_extrapolated = log0.forecast.as_ref().and_then(|f| {
            let (t1, v1) = (f.t_future.last()?, f.point.last()?);
            let (t0, v0) = (total_s, log0.vb_mm.last().cloned()?);
            Some(((v1 - v0) / (t1 - t0).max(1e-9) * total_s).max(0.0))
        });
        let mut ctx1: CutContext = ctx0.clone();
        ctx1.tool.flank_wear_mm = r0.vb_end_mm;
        let (_v1, r1, _s1) = endmill_model::viewport::build_report_prog(&p, "pocket_zigzag", &ctx1, &mut |_| true).unwrap();
        let next = r1.vb_end_mm - r1.vb_start_mm;
        let per_part = log0.per_part_vb_p50_mm.unwrap_or(0.0);
        println!(
            "[PART]  {} · 다음 부품 실제 {:.5} mm · 부품당 예측 p50 {:.5} (×{:.2}) p90 {:.5} · 경로 물리 궤적 {:.5} (경로 계수 {:?}) · 작업 끝 기울기 외삽(예전) {} ",
            p.name,
            next,
            per_part,
            per_part / next.max(1e-12),
            log0.per_part_vb_p90_mm.unwrap_or(0.0),
            phys_part,
            k,
            tail_extrapolated.map(|v| format!("{:.5} (×{:.2})", v, v / next.max(1e-12))).unwrap_or_else(|| "—".into())
        );
        println!(
            "[PART]  다음 부품 성장 {:.5} = 정상 {:.5} + 길들이기 {:.5} · 띠({:.1}) 성장 {:.5}",
            next,
            r1.growth_calib_mm,
            r1.growth_fixed_mm,
            ap,
            r1.wear_field.band_vb(ap)
        );
    }
}
