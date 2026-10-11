//! 물리(MPM) 교차검증 보정이 파이프라인의 보정 경로(`calibrated_context`)에 들어가는 방식을 확인한다.
//!
//! 실제 MPM 해석은 돌리지 않고 가짜 기록을 넣는다 (해석기 자체는 `crates/endmill_mpm` 의 테스트가 맡는다).
//! 기록 저장소가 프로세스 전역이라 시나리오를 한 테스트 함수 안에서 차례로 본다.
use endmill_model::physcal::{self, PhysAnalytic, PhysCalRecord, PhysMpm, PhysReliability, PhysRunOptions, RECORD_VERSION};
use endmill_model::physics::Calibration;
use endmill_model::pipeline::{calibrated_context, process_scope};
use endmill_model::profile::{MachiningPreset, MachiningProfile};
use endmill_model::sds::SdsStore;
use endmill_model::WorkpieceMaterial;
use std::path::PathBuf;

fn tmpdir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("endmill_physcal_{}_{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn stainless() -> MachiningProfile {
    let mut p = MachiningProfile::from_preset(&MachiningPreset::default_steel_general(), "physcal-flow");
    p.workpiece_setup.material = WorkpieceMaterial::StainlessSteel;
    p
}

/// 계산을 돌리지 않고 만든 기록. 조건은 `p` 에서 그대로 뽑는다.
fn fake_record(p: &MachiningProfile, id: &str, created_ms: i64, power: f64, temp: f64) -> PhysCalRecord {
    let plan = physcal::build_plan(p, &PhysRunOptions::default()).expect("plan");
    PhysCalRecord {
        id: id.into(),
        created_ms,
        version: RECORD_VERSION,
        cond: plan.cond.clone(),
        mpm: PhysMpm {
            fc_n_per_mm: 100.0,
            fc_sd: 1.0,
            ft_n_per_mm: 20.0,
            interface_t_c: Some(700.0),
            interface_t_p95_c: Some(800.0),
            interface_rise_c: Some(680.0),
            tmax_c: 900.0,
            chip_ratio: Some(0.6),
            shear_angle_deg: Some(33.0),
            mu_apparent: Some(0.4),
            tau_shear_mpa: Some(900.0),
            wp_over_wtool: 0.92,
            ke_ratio_max: 0.01,
            steady: true,
            steps: 1000,
            particles: 1000,
            wall_s: 1.0,
            history: vec![],
            notes: vec![],
        },
        analytic: PhysAnalytic {
            fc_n_per_mm: 100.0 / power,
            ft_n_per_mm: 20.0,
            rise_c: 680.0 / temp,
            interface_cooled_c: 650.0,
            mu: 0.4,
            kc11_eff: 2000.0,
            mc: 0.25,
            softening: 1.0,
        },
        raw_power: power,
        raw_temperature: temp,
        ratio_power: power,
        ratio_temperature: temp,
        reliability: PhysReliability { level: "medium".into(), reasons: vec![] },
        validated: false,
        apply: false,
        apply_temperature: false,
        weight: 0.5,
    }
}

fn axis<'a>(applied: &'a [endmill_model::pipeline::CalibrationApplied], name: &str) -> Option<&'a endmill_model::pipeline::CalibrationApplied> {
    applied.iter().find(|a| a.axis == name)
}

#[test]
fn physics_calibration_flows_through_calibrated_context() {
    physcal::clear_all_for_tests();
    let p = stainless();
    let base = Calibration::default();

    // 1) 기록이 없으면 앱은 전과 똑같다
    let (ctx, applied) = calibrated_context(&p, None);
    assert!((ctx.calib.power - base.power).abs() < 1e-12 && (ctx.calib.temperature - base.temperature).abs() < 1e-12);
    assert!(applied.is_empty(), "기록이 없는데 보정이 적용됨: {:?}", applied.iter().map(|a| a.axis.clone()).collect::<Vec<_>>());

    // 2) 기록을 넣어도 사용자가 '적용'을 켜기 전에는 쓰이지 않는다 (기본 꺼짐)
    physcal::add_record(fake_record(&p, "pc1", 1_000, 0.80, 1.30)).unwrap();
    let (ctx, applied) = calibrated_context(&p, None);
    assert!(applied.is_empty());
    assert!((ctx.calib.power - base.power).abs() < 1e-12);
    assert!(physcal::records()[0].validated == false, "검증되지 않은 기록은 validated=false 여야 한다");

    // 3) 적용을 켜면 power 만 가중치(0.5)만큼 곱해진다 — 온도는 따로 켜야 한다
    physcal::set_flags("pc1", Some(true), None, None).unwrap();
    let (ctx, applied) = calibrated_context(&p, None);
    let pw = axis(&applied, "power").expect("power 보정 항목");
    assert!((pw.factor - 0.90).abs() < 1e-9, "power {}", pw.factor);
    assert!((ctx.calib.power - 0.90).abs() < 1e-9);
    assert_eq!(pw.n, 0, "물리 보정은 실측 개수 n 이 0");
    assert!(pw.scope.starts_with("물리"), "물리 보정이라고 표시해야 한다: {}", pw.scope);
    assert!(axis(&applied, "temperature").is_none());
    assert!((ctx.calib.temperature - base.temperature).abs() < 1e-12);

    // 4) 온도 보정은 별도 선택
    physcal::set_flags("pc1", None, Some(true), None).unwrap();
    let (ctx, applied) = calibrated_context(&p, None);
    let tp = axis(&applied, "temperature").expect("temperature 보정 항목");
    assert!((tp.factor - 1.15).abs() < 1e-9, "temperature {}", tp.factor);
    assert!((ctx.calib.temperature - 1.15).abs() < 1e-9);

    // 5) 실측 원장 보정이 있는 축은 실측이 우선이고, 없는 축에만 물리 보정이 남는다
    let dir = tmpdir("sds");
    let mut sds = SdsStore::open(&dir);
    let scope = process_scope(&p);
    for _ in 0..4 {
        sds.record_calibration(&scope, "power", 1.4, 1.0);
    }
    let (sds_factor, sds_n, _, _) = sds.calibration_factor(&scope, "power").expect("원장 보정");
    let (ctx, applied) = calibrated_context(&p, Some(&sds));
    let pw = axis(&applied, "power").expect("power");
    assert_eq!(pw.n, sds_n);
    assert!(!pw.scope.starts_with("물리"), "실측이 있으면 물리 보정이 덮어쓰면 안 된다: {}", pw.scope);
    assert!((pw.factor - sds_factor).abs() < 1e-12 && (ctx.calib.power - sds_factor).abs() < 1e-12);
    let tp = axis(&applied, "temperature").expect("temperature 는 물리 보정이 남아야 한다");
    assert!(tp.scope.starts_with("물리"));
    assert_eq!(applied.iter().filter(|a| a.axis == "power").count(), 1, "같은 축이 두 번 들어가면 안 된다");
    sds.close_runs();

    // 6) 조건이 다르면 쓰이지 않는다 (피삭재·공구·절삭속도) — 화면에는 사유가 나온다
    let mut other = p.clone();
    other.workpiece_setup.material = WorkpieceMaterial::Aluminum;
    let (_, applied) = calibrated_context(&other, None);
    assert!(applied.is_empty(), "다른 피삭재에 적용됨");
    let mt = physcal::matches_for(&other);
    assert_eq!(mt.len(), 1);
    assert!(!mt[0].matched && mt[0].reason.contains("피삭재"), "{}", mt[0].reason);
    let mut fast = p.clone();
    fast.conditions.cutting_speed_m_min *= 2.0;
    fast.conditions.spindle_rpm = (fast.conditions.spindle_rpm as f64 * 2.0) as u32;
    let (_, applied) = calibrated_context(&fast, None);
    assert!(applied.is_empty(), "절삭속도가 2배인 조건에 적용됨");
    assert!(physcal::matches_for(&p)[0].matched);

    // 7) 가중치 0 이면 영향이 없고, 극단적인 비도 한계 안으로 들어온다
    physcal::set_flags("pc1", None, None, Some(0.0)).unwrap();
    let (_, applied) = calibrated_context(&p, None);
    assert!(applied.is_empty());
    physcal::add_record(fake_record(&p, "pc2", 2_000, 3.0, 0.2)).unwrap();
    physcal::set_flags("pc2", Some(true), Some(true), Some(1.0)).unwrap();
    let (ctx, applied) = calibrated_context(&p, None);
    assert!((ctx.calib.power - physcal::FACTOR_MAX).abs() < 1e-12, "{}", ctx.calib.power);
    assert!((ctx.calib.temperature - physcal::FACTOR_MIN).abs() < 1e-12, "{}", ctx.calib.temperature);
    assert_eq!(applied.len(), 2);

    // 8) 적용 중인 기록이 여러 개면 조건이 맞는 가장 최근 것을 쓴다
    physcal::set_flags("pc1", Some(true), Some(false), Some(1.0)).unwrap();
    let (ctx, _) = calibrated_context(&p, None);
    assert!((ctx.calib.power - physcal::FACTOR_MAX).abs() < 1e-12, "더 최근 기록(pc2)이 쓰여야 한다: {}", ctx.calib.power);
    physcal::remove("pc2").unwrap();
    let (ctx, applied) = calibrated_context(&p, None);
    assert!((ctx.calib.power - 0.80).abs() < 1e-9, "pc2 를 지우면 pc1 (가중 1.0 → 0.80): {}", ctx.calib.power);
    assert!(axis(&applied, "temperature").is_none());

    // 9) 지운 뒤에는 원래대로
    physcal::remove("pc1").unwrap();
    let (ctx, applied) = calibrated_context(&p, None);
    assert!(applied.is_empty() && (ctx.calib.power - base.power).abs() < 1e-12);
    physcal::clear_all_for_tests();
}
