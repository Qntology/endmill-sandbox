//! 공구 수명 말기(정지·교체·만료·파손)와 공구 × 피삭재 N:N 행렬 시험.
//!
//! 예전 경로 시뮬레이션은 VB 를 한계의 2배에서 자르기만 하고 경로 끝까지 깎았다 (인코넬 포켓: 한계를 넘긴 뒤 134 s · 합력 8 kN,
//! 고경도 볼 포켓: 한계를 넘긴 뒤 26 분). 여기서는 그런 일이 다시 생기지 않는지와, 공구를 다른 소재에 댔을 때의 판정이
//! 리서치 정리(문헌·제조사 자료)와 같은 방향인지를 본다. 행렬 결과는 `[MATRIX]` 줄과 CSV/JSON 으로 남긴다
//! (`ENDMILL_MATRIX_DIR` 가 있으면 그 폴더, 없으면 임시 폴더).
use endmill_model::appfit;
use endmill_model::pairmatrix::{self, MatrixCell};
use endmill_model::physics::{self, CutContext};
use endmill_model::profile::{MachiningPreset, MachiningProfile, ToolLifePolicy};
use endmill_model::store::Store;
use endmill_model::toollife;
use endmill_model::WorkpieceMaterial;

fn preset(name: &str) -> MachiningProfile {
    let p = MachiningPreset::builtin().into_iter().find(|p| p.name.contains(name)).expect("preset");
    MachiningProfile::from_preset(&p, &p.name)
}

fn simulate(p: &MachiningProfile, pattern: &str) -> endmill_model::loadsim::LoadSimReport {
    let ctx = CutContext::from_profile(p);
    endmill_model::viewport::build_report_prog(p, pattern, &ctx, &mut |_| true).unwrap().1
}

fn with_policy(name: &str, policy: ToolLifePolicy) -> MachiningProfile {
    let mut p = preset(name);
    p.machine.tool_life_policy = policy;
    p
}

fn print_events(tag: &str, r: &endmill_model::loadsim::LoadSimReport) {
    let tl = &r.tool_life;
    println!(
        "[EOL] {} · 상태 {} · 공구 {} · 완료 {:.0}% · VB {:.3} mm · 응력 ×{:.2} · 피로 {:.3} · 치핑 {:.0}% · 부하 ×{:.2}",
        tag,
        tl.state,
        tl.tools_used,
        tl.completed_fraction() * 100.0,
        r.vb_end_mm,
        tl.peak_stress_ratio,
        tl.fatigue_damage,
        tl.chip_probability * 100.0,
        tl.load_ratio_max
    );
    for e in tl.events.iter() {
        println!("[EOL]    {} #{} t {:.1} s seg {} — {}", e.kind, e.tool_no, e.t_s, e.seg, e.message);
    }
}

#[test]
fn tertiary_wear_factor_starts_at_the_onset_and_reaches_the_multiplier_at_collapse() {
    for g in [
        endmill_model::pairref::WpGroup::Aluminum,
        endmill_model::pairref::WpGroup::CarbonSteel,
        endmill_model::pairref::WpGroup::Hardened,
        endmill_model::pairref::WpGroup::Nickel,
    ] {
        let p = toollife::eol_params(g);
        assert!((toollife::accel_factor(&p, 0.0) - 1.0).abs() < 1e-12);
        assert!((toollife::accel_factor(&p, p.vb_accel_mm) - 1.0).abs() < 1e-12);
        assert!((toollife::accel_factor(&p, p.vb_collapse_mm) - p.accel_mult).abs() < 1e-9, "{:?}", g);
        let mid = toollife::accel_factor(&p, 0.5 * (p.vb_accel_mm + p.vb_collapse_mm));
        assert!(mid > 1.0 && mid < p.accel_mult);
        assert!(p.vb_collapse_mm > p.vb_accel_mm && p.static_frac > p.fatigue_frac);
    }
}

#[test]
fn rake_corrected_radial_recession_matches_the_classic_formula() {
    let f = physics::radial_per_vb(12.0, 18.0);
    let ta = 12f64.to_radians().tan();
    let tg = 18f64.to_radians().tan();
    assert!((f - ta / (1.0 - ta * tg)).abs() < 1e-12);
    assert!(f > 1.06 * ta && f < 1.09 * ta, "positive rake recesses more than VB·tanα: {}", f / ta);
    assert!(physics::radial_per_vb(12.0, -5.0) < ta, "negative rake recesses less");
}

#[test]
fn body_breaking_force_has_the_published_scale() {
    // 리서치 추산: 2.5D 팔에서 2날 약 26·D² N, 4날 약 43·D² N (항절력 4 GPa, 코어 단면) — 이 모델은 등가 단면·정적 유효 강도라 같은 자릿수면 된다
    let p = preset("알루미늄 황삭");
    let mut tool = CutContext::from_profile(&p).tool;
    tool.stickout_mm = 15.0 + tool.loc_mm.min(15.0);
    let eol = toollife::eol_params(endmill_model::pairref::WpGroup::Aluminum);
    let s = toollife::strength_of(&tool, &eol);
    let ap = 2.0;
    let unit = toollife::body_stress(&tool, 1.0, 0.0, ap).sigma_mpa;
    let f_break = s.static_mpa() / unit;
    println!("[EOL] Ø{} {}날 돌출 {:.0} mm 정적 파손 하중 {:.0} N (정적 유효 {:.0} MPa)", tool.diameter_mm, tool.flutes, tool.stickout_mm, f_break, s.static_mpa());
    assert!(f_break > 400.0 && f_break < 3000.0, "Ø6 2-flute breaks near 1 kN, got {:.0} N", f_break);
    assert!(s.fatigue_mpa() < s.static_mpa());
    assert!(s.cycles_to_failure(0.9 * s.fatigue_mpa()).is_infinite());
    assert!(s.cycles_to_failure(1.2 * s.fatigue_mpa()) < 1.0e7);
}

#[test]
fn inconel_pocket_stops_changes_expires_or_breaks_by_policy_instead_of_cutting_on() {
    let limit = CutContext::from_profile(&preset("인코넬")).vb_limit_mm();
    // 감시 없음: 파손으로 끝난다 (붕괴·피로·과부하·치핑 중 하나), 날끝 붕괴 VB 를 넘겨 깎지 않는다
    let run = simulate(&with_policy("인코넬", ToolLifePolicy::RunToFailure), "pocket_zigzag");
    print_events("인코넬 포켓 · 감시 없음", &run);
    let tr = &run.tool_life;
    assert!(tr.failed && !tr.failure_cause.is_empty(), "an unmonitored worn tool ends in failure");
    assert!(run.vb_end_mm <= tr.params.vb_collapse_mm * toollife::VB_GUARD_FACTOR + 1e-9);
    // 정지: 수명 판정(지금 절삭을 마치고) 또는 치핑 감지(즉시)에서 멈춘다 — 한계를 크게 넘겨 깎지 않는다
    let stop = simulate(&with_policy("인코넬", ToolLifePolicy::Stop), "pocket_zigzag");
    print_events("인코넬 포켓 · 정지", &stop);
    let tl = &stop.tool_life;
    assert!(tl.events.iter().any(|e| e.kind == "life_end" || e.kind == "chip_detected"), "the pocket wears or chips the tool");
    assert!(tl.unfinished && !tl.failed && tl.completed_fraction() < 0.99, "stop leaves the rest of the path uncut");
    assert!(stop.vb_end_mm <= toollife::HARD_LIMIT_FACTOR * limit + 0.01, "no deep overrun past the limit: {:.3} vs {:.2}", stop.vb_end_mm, limit);
    assert!(stop.warnings.iter().any(|w| w.contains("[공구 수명]")));
    // 같은 공구는 감시 여부와 관계없이 같은 위험을 겪는다: 감시하는 정책이 감시 없는 공구가 부서지는 지점을 넘어 깎지 않는다
    assert!(
        stop.cut_time_min <= run.cut_time_min + 1e-9,
        "the monitored tool never cuts past the point where the same unmonitored tool fails: {:.3} vs {:.3} min",
        stop.cut_time_min,
        run.cut_time_min
    );
    assert!(tl.chip_probability < toollife::CHIP_FAILURE_P + 0.05, "a monitored tool is not left cutting with a likely chipped edge: {:.2}", tl.chip_probability);
    // 교체 후 계속: 공구 여러 개로 끝까지, 어느 공구도 판정을 크게 넘기지 않고 치핑 확률이 파손 수준에 닿으면 바로 바뀐다
    let change = simulate(&with_policy("인코넬", ToolLifePolicy::ChangeTool), "pocket_zigzag");
    print_events("인코넬 포켓 · 교체", &change);
    let tc = &change.tool_life;
    if !tc.failed {
        assert!(tc.tool_changes >= 1 && tc.tools_used == tc.tool_changes + 1);
        assert!(!tc.unfinished && tc.completed_fraction() > 0.999);
        for r in tc.retired.iter() {
            assert!(r.vb_band_mm <= toollife::HARD_LIMIT_FACTOR * limit + 0.01, "retired tool #{} overran: {:.3}", r.tool_no, r.vb_band_mm);
            assert!(r.chip_p < toollife::CHIP_FAILURE_P + 0.05, "retired tool #{} kept cutting chipped: {:.2}", r.tool_no, r.chip_p);
            assert!(r.cut_min > 0.0 && !r.field.is_empty());
            assert!(r.cause == "life_end" || r.cause == "chipping", "retired tool #{} cause {:?}", r.tool_no, r.cause);
        }
        assert_eq!(tc.retired.iter().filter(|r| r.cause == "chipping").count() as u32, tc.chip_detected);
        assert!(change.vb_end_mm < limit + 0.01);
        assert!(change.cut_time_min >= stop.cut_time_min);
        assert!(change.wear_log.iter().any(|b| b.tool_no == tc.tools_used), "bins carry the tool number");
    }
    // 공구 수명 관리(프로그램 끝까지): 만료 표시가 남고, 끝까지 가거나 그 사이 파손으로 멈춘다
    let fin = simulate(&with_policy("인코넬", ToolLifePolicy::FinishPart), "pocket_zigzag");
    print_events("인코넬 포켓 · 프로그램 끝까지", &fin);
    assert!(fin.tool_life.expired || fin.tool_life.failed);
}

#[test]
fn a_finishing_ball_cannot_rough_a_hardened_pocket() {
    // 고경도 정삭 프리셋(Ø6 롱넥 볼)의 절입으로 포켓을 지그재그로 파면 슬롯에 가까운 결합 — 몸체 응력이 강도를 넘어 바로 부러진다
    for pol in ToolLifePolicy::ALL {
        let r = simulate(&with_policy("금형강", pol), "pocket_zigzag");
        let tl = &r.tool_life;
        print_events(&format!("고경도 포켓 · {}", pol.key()), &r);
        assert!(tl.failed && tl.completed_fraction() < 0.2, "{:?}: the ball nose breaks early, not after 30 minutes", pol);
    }
}

#[test]
fn intended_finishing_passes_do_not_break_their_tools() {
    for name in ["알루미늄 황삭", "티타늄", "탄소강", "스테인리스", "인코넬", "CFRP", "알루미늄 정삭"] {
        let r = simulate(&preset(name), "rect_profile");
        let tl = &r.tool_life;
        println!("[EOL] {} rect_profile · 상태 {} · 응력 ×{:.2} · 치핑 {:.0}% · 부하 ×{:.2}", name, tl.state, tl.peak_stress_ratio, tl.chip_probability * 100.0, tl.load_ratio_max);
        assert!(!tl.failed, "{}: {:?}", name, tl.stop_reason());
        assert!(!tl.unfinished && tl.completed_fraction() > 0.999, "{}", name);
        assert!(tl.peak_stress_ratio < 0.8, "{}: {:.2}", name, tl.peak_stress_ratio);
    }
}

#[test]
fn a_tool_that_starts_worn_out_is_not_used_silently() {
    let mut p = with_policy("탄소강", ToolLifePolicy::Stop);
    let mut ctx = CutContext::from_profile(&p);
    ctx.tool.flank_wear_mm = 1.05 * ctx.vb_limit_mm();
    let r = endmill_model::viewport::build_report_prog(&p, "rect_profile", &ctx, &mut |_| true).unwrap().1;
    assert!(r.tool_life.events.iter().any(|e| e.kind == "start_worn"));
    assert!(r.tool_life.unfinished && r.cut_time_min == 0.0 && r.removed_volume_cm3 == 0.0, "the worn tool does not start cutting");
    p.machine.tool_life_policy = ToolLifePolicy::ChangeTool;
    ctx.machine.tool_life_policy = ToolLifePolicy::ChangeTool;
    let r2 = endmill_model::viewport::build_report_prog(&p, "rect_profile", &ctx, &mut |_| true).unwrap().1;
    assert_eq!(r2.tool_life.tool_changes, 1);
    assert!(!r2.tool_life.unfinished && r2.vb_start_mm == 0.0 && r2.cut_time_min > 0.0, "a sister tool takes over before the first cut");
}

#[test]
fn unfinished_first_pass_holds_the_compensation() {
    let p = with_policy("인코넬", ToolLifePolicy::Stop);
    let (ctx, _) = endmill_model::pipeline::calibrated_context(&p, None);
    let r = endmill_model::viewport::build_report_prog(&p, "pocket_zigzag", &ctx, &mut |_| true).unwrap().1;
    assert!(r.tool_life.unfinished);
    let sds = endmill_model::sds::SdsStore::open(&std::env::temp_dir().join(format!("endmill-eol-sds-{}", std::process::id())));
    let log = endmill_model::wearcomp::analyze_job(&p, &ctx, &r, None, &sds);
    let draft = endmill_model::wearcomp::evaluate(&p, &ctx, &r, &log, &endmill_model::wearcomp::CompOptions::default(), None);
    let g = draft.gates.iter().find(|g| g.id == "pass1_complete").expect("gate");
    assert_eq!(g.severity, 3);
    assert_eq!(draft.rule_action, endmill_model::wearcomp::CompAction::Hold);
    assert!(!draft.allowed.iter().any(|a| a.cuts()), "no compensation pass on an unfinished first pass: {:?}", draft.allowed);
}

#[test]
fn mid_path_tool_changes_are_recorded_as_separate_tool_instances() {
    let p = with_policy("인코넬", ToolLifePolicy::ChangeTool);
    let dir = std::env::temp_dir().join(format!("endmill-eol-store-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let mut store = Store::open(&dir.join("library")).unwrap();
    let mut sds = endmill_model::sds::SdsStore::open(&dir.join("sds"));
    let (mut ctx, _) = endmill_model::pipeline::calibrated_context(&p, Some(&sds));
    let (row, start, _) = store.prepare_tool(&p, &mut ctx).unwrap();
    let r = endmill_model::viewport::build_report_prog(&p, "pocket_zigzag", &ctx, &mut |_| true).unwrap();
    let rep = r.1;
    if rep.tool_life.tool_changes == 0 || rep.tool_life.failed {
        return;
    }
    let log = endmill_model::wearcomp::analyze_job_with(&p, &ctx, &rep, None, &sds, Some(&start));
    let opts = endmill_model::wearcomp::CompOptions::default();
    let draft = endmill_model::wearcomp::evaluate(&p, &ctx, &rep, &log, &opts, None);
    let dec = endmill_model::wearcomp::consult(&draft, None, None, false);
    let comp = endmill_model::wearcomp::finalize(&p, &ctx, &rep, &r.2, &log, &draft, &dec, &opts);
    let _ = endmill_model::wearcomp::record(&mut sds, &p, &log, &comp, None);
    let mut row = row;
    let em = row.endmill_id;
    let out = endmill_model::store::jobs::record_job(
        &store.rdb,
        Some(&mut row),
        &endmill_model::store::jobs::JobInput {
            project_id: store.project_id,
            profile: &p,
            ctx: &ctx,
            pattern: "pocket_zigzag",
            stamp: "EOL1",
            parent_job_id: None,
            endmill_id: em,
            workpiece_id: None,
            report: &rep,
            log: &log,
            comp: &comp,
            replan: false,
            summary: serde_json::json!({}),
        },
    )
    .unwrap();
    for l in out.lines.iter() {
        println!("[EOL] {}", l);
    }
    let tools = store.rdb.list_tools(true, 20).unwrap();
    // 경로 중 교체(수명 판정이든 치핑 감지든)로 빠진 공구는 모두 은퇴로 남는다
    let retired = tools.iter().filter(|t| t.retired_at.is_some() && t.retire_reason.contains("예비 공구로 교체")).count();
    assert!(retired as u32 >= rep.tool_life.tool_changes, "each worn-out tool is retired: {} of {}", retired, rep.tool_life.tool_changes);
    let active = store.rdb.active_tool(&row.slot_key).unwrap().expect("a sister tool is active");
    assert!(active.vb_band_mm < CutContext::from_profile(&p).vb_limit_mm(), "the active instance holds only the last tool's wear: {:.3}", active.vb_band_mm);
    assert!((active.cut_min - rep.tool_life.last_tool_cut_min).abs() < 1e-6);
}

// ---------------------------------------------------------------- N:N 행렬

fn wp(m: WorkpieceMaterial) -> physics::WorkpieceProps {
    physics::WorkpieceProps::of(&m)
}

#[test]
fn application_fit_reproduces_the_research_anchor_cases() {
    // (1) 38 HRC 강: 날카로운 날(4 µm) 대 호닝(25 µm) — 수명이 크게 차이 (문헌 약 8배)
    let p = preset("탄소강");
    let ctx = CutContext::from_profile(&p);
    let mut sharp = ctx.tool.clone();
    sharp.edge_radius_um = 4.0;
    let mut honed = ctx.tool.clone();
    honed.edge_radius_um = 25.0;
    let w38 = wp(WorkpieceMaterial::AlloySteel { hardness_hrc: 38 });
    let fs = appfit::evaluate(&sharp, &w38, false);
    let fh = appfit::evaluate(&honed, &w38, false);
    assert!(fs.wear_factor * fs.chip_factor.sqrt() > 2.0 * fh.wear_factor * fh.chip_factor.sqrt(), "sharp {:?} vs honed {:?}", (fs.wear_factor, fs.chip_factor), (fh.wear_factor, fh.chip_factor));
    // (2) +8° 경사·날카로운 날로 56~58 HRC: 첫 패스 파손급 치핑
    let mut pos = ctx.tool.clone();
    pos.rake_deg = 8.0;
    pos.edge_radius_um = 4.0;
    let f58 = appfit::evaluate(&pos, &wp(WorkpieceMaterial::AlloySteel { hardness_hrc: 58 }), false);
    assert!(f58.chip_factor > 100.0 && f58.verdict == 2, "{}", f58.summary());
    // (3) Ti 여유각 8° 는 10° 보다 치핑·마모가 크다
    let ti = wp(WorkpieceMaterial::Titanium);
    let mut c8 = ctx.tool.clone();
    c8.clearance_deg = 8.0;
    c8.rake_deg = 10.0;
    c8.edge_radius_um = 12.0;
    let mut c10 = c8.clone();
    c10.clearance_deg = 10.0;
    let a8 = appfit::evaluate(&c8, &ti, false);
    let a10 = appfit::evaluate(&c10, &ti, false);
    assert!(a8.chip_factor > a10.chip_factor && a8.wear_factor > a10.wear_factor);
    // (4) HSS 로 58 HRC 는 파손급
    let mut hss = ctx.tool.clone();
    hss.substrate = "hss".into();
    assert_eq!(appfit::evaluate(&hss, &wp(WorkpieceMaterial::AlloySteel { hardness_hrc: 58 }), false).verdict, 2);
    // (5) 알루미늄 슬롯에 6날은 칩 막힘
    let d6 = CutContext::from_profile(&preset("CFRP")).tool;
    let al = wp(WorkpieceMaterial::Aluminum);
    assert!(appfit::evaluate_with(&d6, &al, true, 1.0).evac_risk > appfit::evaluate(&d6, &al, false).evac_risk);
}

#[test]
fn every_built_in_preset_tool_fits_its_own_material() {
    for p in MachiningPreset::builtin() {
        let prof = MachiningProfile::from_preset(&p, &p.name);
        let ctx = CutContext::from_profile(&prof);
        let fit = appfit::evaluate(&ctx.tool, &ctx.wp, false);
        println!("[MATRIX] 프리셋 {} — {}", p.name, fit.summary());
        assert!(fit.verdict <= 1, "{}: {}", p.name, fit.summary());
        assert!(fit.wear_factor < 1.2, "{}: the intended pair must not change its calibrated wear: ×{:.3}", p.name, fit.wear_factor);
    }
}

fn save_matrix(cells: &[MatrixCell], tag: &str) {
    let dir = std::env::var("ENDMILL_MATRIX_DIR").map(std::path::PathBuf::from).unwrap_or_else(|_| std::env::temp_dir().join("endmill-matrix"));
    let _ = std::fs::create_dir_all(&dir);
    let csv = dir.join(format!("pair_matrix_{}.csv", tag));
    let json = dir.join(format!("pair_matrix_{}.json", tag));
    let _ = std::fs::write(&csv, pairmatrix::csv(cells));
    let _ = std::fs::write(&json, serde_json::to_string_pretty(cells).unwrap_or_default());
    println!("[MATRIX] 저장: {} · {}", csv.display(), json.display());
}

#[test]
fn tool_material_matrix_ranks_like_practice() {
    let cells = pairmatrix::run(Some("rect_profile"));
    for l in pairmatrix::lines(&cells) {
        println!("{}", l);
    }
    save_matrix(&cells, "rect_profile");
    assert_eq!(cells.len(), 64);
    for c in cells.iter() {
        assert!(c.life_min.is_finite() && c.life_min > 0.0, "{} × {}: life {}", c.use_case, c.tool, c.life_min);
        assert!(c.interface_c.is_finite() && c.ra_um.is_finite() && c.fit_score.is_finite());
    }
    let cell = |uc: &str, model: &str| cells.iter().find(|c| c.use_case == uc && c.tool_model == model).expect("cell");
    // 의도된 짝은 부적합이 아니고, 경로에서 파손되지 않는다
    for c in cells.iter().filter(|c| c.intended) {
        assert!(c.verdict <= 1, "{} × {}: {:?}", c.use_case, c.tool, c.reasons);
        assert!(c.path_failure.is_empty(), "{} × {} broke: {}", c.use_case, c.tool, c.path_failure);
    }
    // 다이아몬드 코팅은 철계·니켈계에서 화학 궁합이 나쁘다 (탄소 확산)
    for uc in ["carbon_steel", "alloy_steel_30", "stainless", "inconel", "hardened_58"] {
        assert!(cell(uc, "BUILTIN-DC10F6").compat_severity >= 2, "diamond on {}", uc);
    }
    // 알루미늄용 날카로운 고경사 공구(DLC Ø8, TiAlN Ø6)로 58 HRC 는 부적합 (치핑)
    for m in ["BUILTIN-DL8F3", "YG-1 AL-2F"] {
        let c = cell("hardened_58", m);
        assert_eq!(c.verdict, 2, "{} on 58 HRC: {:?}", m, c.reasons);
        assert!(c.fit_chip > 20.0);
    }
    // 고경도용 음의 경사 볼은 알루미늄에서 무뎌 구성인선 (적합 아님)
    let hb_al = cell("aluminum", "BUILTIN-HB6N18");
    assert!(hb_al.fit_bue > 1.5 && hb_al.verdict >= 1, "{:?}", hb_al.reasons);
    // CFRP 는 다이아몬드 코팅 공구가 가장 오래 간다
    let cf_best = cells.iter().filter(|c| c.use_case == "cfrp").max_by(|a, b| a.life_min.partial_cmp(&b.life_min).unwrap()).unwrap();
    assert_eq!(cf_best.tool_model, "BUILTIN-DC10F6", "best on CFRP: {}", cf_best.tool);
    // 고경도에서는 고경도용 볼이 알루미늄용 공구들보다 순위가 높다
    let hb = cell("hardened_58", "BUILTIN-HB6N18");
    assert!(hb.rank_in_material < cell("hardened_58", "BUILTIN-DL8F3").rank_in_material && hb.rank_in_material < cell("hardened_58", "YG-1 AL-2F").rank_in_material);
    // 인코넬에서는 내열합금용 5날이 DLC 알루미늄 공구보다 오래 간다
    assert!(cell("inconel", "BUILTIN-AT12F5").life_min > cell("inconel", "BUILTIN-DL8F3").life_min);
    // 의도된 짝은 그 소재에서 상위권 (8개 중 4위 안)
    for c in cells.iter().filter(|c| c.intended) {
        assert!(c.rank_in_material <= 4, "{} × {} ranks {}", c.use_case, c.tool, c.rank_in_material);
    }
}
