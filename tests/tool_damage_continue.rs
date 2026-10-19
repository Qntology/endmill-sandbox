//! 공구 손상으로 1차가 멈춘 뒤 작업자가 고른 정책으로 같은 부품을 이어 깎는 흐름과, 손상 공구로 깎은 최종 벽(품질 저하) 표시 시험.
//!
//! 예전에는 공구가 수명 판정·치핑 감지로 멈추면 그 결과를 보고 끝이었다. 이제는 선택 카드(예비 공구로 이어서 · 손상 공구로 끝까지 ·
//! 여기서 정지)를 띄우고, 고른 정책으로 같은 부품을 다시 계산한다 — 시뮬레이션은 결정적이라 멈춘 지점까지는 같다. 직전 작업의 1차
//! 기록은 공구 개체에서 되돌린 뒤(대체됨) 새로 기록한다. 손상 공구로 깎은 최종 벽은 벽 구간(벽 방향 · 위치 · 치수 · 조도)으로 남긴다.
use endmill_model::loadsim::{LoadSimReport, QUALITY_CHIP_P};
use endmill_model::physics::CutContext;
use endmill_model::profile::{MachiningPreset, MachiningProfile, ToolLifePolicy};
use endmill_model::store::jobs::{material_key, record_job, JobInput};
use endmill_model::store::Store;
use endmill_model::toollife;

fn with_policy(name: &str, policy: ToolLifePolicy) -> MachiningProfile {
    let pre = MachiningPreset::builtin().into_iter().find(|p| p.name.contains(name)).expect("preset");
    let mut p = MachiningProfile::from_preset(&pre, &pre.name);
    p.machine.tool_life_policy = policy;
    p
}

fn simulate(p: &MachiningProfile, pattern: &str) -> LoadSimReport {
    let ctx = CutContext::from_profile(p);
    endmill_model::viewport::build_report_prog(p, pattern, &ctx, &mut |_| true).unwrap().1
}

fn print_quality(tag: &str, r: &LoadSimReport) {
    let q = &r.quality_loss;
    println!(
        "[QUAL] {} · 정책 {} · 완료 {:.0}% · 절삭 {:.2}분 · 최종 벽 {} 점 · 손상 공구로 깎음 {} 점 ({:.0}%) · 벽 {}곳 · 한계 넘어 덜 깎임 최대 +{:.1} µm · Ra 최대 {:.2} µm · 정상 공구 벽 {} 점 (Ra {:.2} · 위치 오차 P90 {:.1})",
        tag,
        r.tool_life.policy.key(),
        r.tool_life.completed_fraction() * 100.0,
        r.cut_time_min,
        q.final_points,
        q.degraded_points,
        q.degraded_fraction * 100.0,
        q.zones.len(),
        q.worst_extra_wear_um,
        q.worst_ra_um,
        q.good_points,
        q.good_ra_mean_um,
        q.good_wall_p90_um
    );
    for z in q.zones.iter().take(16) {
        println!(
            "[QUAL]    {} · {} 쪽 벽 · 공구 #{} · seg {}–{} · t {:.1}–{:.1} s · X {:.1}~{:.1} Y {:.1}~{:.1} Z {:.2}~{:.2} · 벽 {:.1} mm · {} 점 · 수명 {:.0}% · 치핑 {:.0}% · 한계 넘어 +{:.1} µm (마모 {:.1}) · Ra {:.2}/{:.2} (새 날 {:.2} · ×{:.2}) · 위치 오차 {:.1} µm",
            z.cause,
            z.wall_dir,
            z.tool_no,
            z.seg_from,
            z.seg_to,
            z.t_from_s,
            z.t_to_s,
            z.x_min,
            z.x_max,
            z.y_min,
            z.y_max,
            z.z_min,
            z.z_max,
            z.len_mm,
            z.points,
            z.life_used_max * 100.0,
            z.chip_p_max * 100.0,
            z.extra_wear_um,
            z.wear_um_max,
            z.ra_mean_um,
            z.ra_max_um,
            z.ra_fresh_mean_um,
            z.ra_ratio,
            z.wall_err_max_um
        );
    }
    // 벽 위치 오차(전체)가 큰 최종 벽 한 점의 분해 — 손상 공구 몫(마모)과 나머지(휨·벽 휨·열·런아웃)를 나눠 본다
    if let Some(w) = r.wall_errors.iter().filter(|w| w.final_wall).max_by(|a, b| a.total_um.abs().partial_cmp(&b.total_um.abs()).unwrap_or(std::cmp::Ordering::Equal)) {
        println!(
            "[QUAL]    최대 위치 오차 점 seg {} t {:.1} s ({:.1}, {:.1}, z {:.2}) · 합 {:.1} µm = 공구 휨 {:.1} + 벽 휨 {:.1} + 마모 {:.1} + 공구 열 {:.1} + 소재 열 {:.1} + 런아웃 {:.1} · 벽 두께 {:.2} mm · ap {:.2}",
            w.seg,
            w.t_s,
            w.x,
            w.y,
            w.z_mid,
            w.total_um,
            w.deflection_um,
            w.workpiece_um,
            w.wear_um,
            w.thermal_tool_um,
            w.thermal_wp_um,
            w.runout_um,
            w.wall_mm,
            w.ap_mm
        );
    }
}

/// 한계 가까이 쓴 공구로 시작 (VB = 한계 × `frac`) — 새 포켓 경로는 마지막 정삭 윤곽이 벽을 정하므로, 손상 공구가 그 윤곽을 깎는 경우를 만든다
fn simulate_worn(p: &MachiningProfile, pattern: &str, frac: f64) -> (CutContext, LoadSimReport) {
    let mut ctx = CutContext::from_profile(p);
    ctx.tool.flank_wear_mm = frac * ctx.vb_limit_mm();
    let r = endmill_model::viewport::build_report_prog(p, pattern, &ctx, &mut |_| true).unwrap().1;
    (ctx, r)
}

/// `ENDMILL_QUALITY_DUMP` 가 있으면 뷰포트·보고서 미리보기용으로 경로·뷰포트 결과와 공구 손상 선택 카드를 JSON 으로 남긴다
fn dump_for_preview(p: &MachiningProfile, ctx: &CutContext, pattern: &str, card: &endmill_model::choice::ChoicePrompt) {
    let Ok(path) = std::env::var("ENDMILL_QUALITY_DUMP") else {
        return;
    };
    use endmill_model::gcode::ToolPathSegment as S;
    let (view, _, segs) = endmill_model::viewport::build_report_prog(p, pattern, ctx, &mut |_| true).unwrap();
    let rpm = p.conditions.spindle_rpm;
    let segments: Vec<serde_json::Value> = segs
        .iter()
        .map(|s| {
            let (kind, feed) = (s.kind(), s.feed());
            match s {
                S::Linear { x, y, z, .. } | S::Rapid { x, y, z } => serde_json::json!({ "kind": kind, "x": x, "y": y, "z": z, "feed": feed, "z_start": null, "i": null, "j": null, "rpm": rpm }),
                S::ArcCW { x, y, z, i, j, .. } | S::ArcCCW { x, y, z, i, j, .. } => serde_json::json!({ "kind": kind, "x": x, "y": y, "z": z, "feed": feed, "z_start": null, "i": i, "j": j, "rpm": rpm }),
                S::Plunge { x, y, z_start, z_end, .. } => serde_json::json!({ "kind": kind, "x": x, "y": y, "z": z_end, "feed": feed, "z_start": z_start, "i": null, "j": null, "rpm": rpm }),
            }
        })
        .collect();
    let out = serde_json::json!({ "view": view, "segments": segments, "damage_choice": card });
    std::fs::write(&path, serde_json::to_string(&out).unwrap()).unwrap();
    println!("[QUAL] 미리보기 자료 저장: {}", path);
}

fn check_zones(r: &LoadSimReport, d_mm: f64) {
    let q = &r.quality_loss;
    assert_eq!(q.zones.iter().map(|z| z.points).sum::<usize>(), q.degraded_points, "every degraded point is in one wall zone");
    assert_eq!(q.good_points + q.degraded_points, q.final_points);
    assert!(q.points.len() <= 1500 && q.points.windows(2).all(|w| w[0].t_s <= w[1].t_s), "viewport points are capped and in cut order");
    for p in q.points.iter() {
        let z = &q.zones[p.zone];
        assert!(p.severity == z.severity && p.x >= z.x_min - 1e-9 && p.x <= z.x_max + 1e-9 && p.y >= z.y_min - 1e-9 && p.y <= z.y_max + 1e-9, "a point lies in its zone");
    }
    for z in q.zones.iter() {
        match z.cause.as_str() {
            "past_limit" => assert!(z.life_used_max >= 1.0 - 1e-9 && z.severity == 1, "past limit at {:.2}", z.life_used_max),
            "chip_risk" => assert!(z.chip_p_max >= QUALITY_CHIP_P - 1e-9 && z.severity == 2),
            "accelerated" => assert_eq!(z.severity, 3),
            other => panic!("unknown cause {}", other),
        }
        assert!(z.seg_from <= z.seg_to && z.t_from_s <= z.t_to_s + 1e-9 && z.len_mm >= 0.0);
        // 같은 벽면: 축 방향 벽이면 벽에 수직인 좌표가 반경 폭 띠 안에 있다
        let thin = match z.wall_dir.as_str() {
            "+X" | "−X" => Some(z.x_max - z.x_min),
            "+Y" | "−Y" => Some(z.y_max - z.y_min),
            _ => None,
        };
        if let Some(w) = thin {
            assert!(w <= 0.5 * d_mm.max(2.0) + 1e-6, "{} wall zone spans {:.2} mm across the wall", z.wall_dir, w);
        }
        // 새 날 Ra 는 마모 배수만 뺀 값이라 깎인 그대로보다 크지 않다
        assert!(z.ra_fresh_mean_um <= z.ra_mean_um + 1e-9 && z.ra_ratio >= 1.0 - 1e-9);
        assert!(z.extra_wear_um >= 0.0 && z.extra_wear_um <= z.wear_um_max + 1e-9);
    }
}

/// 포켓 경로는 마지막 정삭 윤곽이 벽 치수를 정한다 (`docs/pocket-zigzag-path.md`) — 손상 공구 표시는 손상 공구가 그 윤곽을 깎았을 때 나온다.
/// 인코넬 포켓은 첫 단계 진입·첫 줄에서 수명 판정에 닿아 정삭 전에 멈춘다: 카드는 뜨지만 부품 벽이 아직 없어 품질 저하도 없다.
/// 한계 가까이 쓴 공구(VB 0.95 × 한계)로 탄소강 포켓을 깎으면: 정지는 황삭 중에 멈춰 카드를 띄우고, 손상 공구로 끝까지 가면 한계를 넘긴
/// 공구가 정삭 윤곽을 깎아 최종 벽이 모두 품질 저하로 남으며(수명 한계 수준보다 덜 깎임), 예비 공구로 이어 가면 새 공구가 정삭해 정상 벽이 된다
#[test]
fn a_damaged_tool_stop_offers_to_continue_and_finishing_with_it_marks_the_degraded_walls() {
    let past = |r: &LoadSimReport| r.quality_loss.zones.iter().filter(|z| z.severity != 2).map(|z| z.points).sum::<usize>();
    let card_of = |r: &LoadSimReport| {
        let card = toollife::damage_choice(&r.tool_life).expect("a stopped first pass asks the operator");
        assert_eq!(card.kind, "damage");
        assert_eq!(card.recommended, "change_tool");
        assert_eq!(card.options[0].key, "change_tool", "the recommended option comes first");
        for k in ["change_tool", "finish_part", "stop"] {
            assert!(card.options.iter().any(|o| o.key == k && o.enabled), "{} is offered", k);
        }
        card
    };
    let d_mm = CutContext::from_profile(&with_policy("인코넬", ToolLifePolicy::Stop)).tool.diameter_mm;
    let stop = simulate(&with_policy("인코넬", ToolLifePolicy::Stop), "pocket_zigzag");
    print_quality("인코넬 포켓 · 정지", &stop);
    let tl = &stop.tool_life;
    assert!(tl.unfinished && !tl.failed, "the Inconel pocket stops on the tool-life call");
    check_zones(&stop, d_mm);
    card_of(&stop);
    assert_eq!(stop.quality_loss.final_points, 0, "the stop comes before the finishing contour — no part wall yet");
    // 손상 공구로 끝까지: 인코넬 공구는 정삭 윤곽에 닿기 전에 부러진다 — 다시 묻고, 수명 한계를 넘긴 뒤의 파손이면 예비 공구로 이어 갈 수 있다
    let fin = simulate(&with_policy("인코넬", ToolLifePolicy::FinishPart), "pocket_zigzag");
    print_quality("인코넬 포켓 · 손상 공구로 끝까지", &fin);
    assert!(fin.cut_time_min >= stop.cut_time_min - 1e-9, "continuing cuts at least as far as the stop");
    check_zones(&fin, d_mm);
    if fin.tool_life.failed {
        let again = toollife::damage_choice(&fin.tool_life).expect("a broken tool asks again");
        assert!(again.options.iter().any(|o| o.key == "finish_part" && !o.enabled), "a broken tool cannot finish the part");
        if fin.tool_life.life_used_end >= 1.0 {
            assert_eq!(again.recommended, "change_tool", "the break came from wear past the limit");
        }
    }

    // 한계 가까이 쓴 공구로 탄소강 포켓
    let worn = |pol: ToolLifePolicy| simulate_worn(&with_policy("탄소강", pol), "pocket_zigzag", 0.95);
    let ds_mm = CutContext::from_profile(&with_policy("탄소강", ToolLifePolicy::Stop)).tool.diameter_mm;
    let (_, s_stop) = worn(ToolLifePolicy::Stop);
    print_quality("탄소강 포켓 · 한계 가까운 공구 · 정지", &s_stop);
    assert!(s_stop.tool_life.unfinished && !s_stop.tool_life.failed, "the worn tool reaches its limit during roughing");
    check_zones(&s_stop, ds_mm);
    let card = card_of(&s_stop);
    assert_eq!(s_stop.quality_loss.final_points, 0);
    let (ctx_fin, s_fin) = worn(ToolLifePolicy::FinishPart);
    print_quality("탄소강 포켓 · 한계 가까운 공구 · 손상 공구로 끝까지", &s_fin);
    check_zones(&s_fin, ds_mm);
    let q = &s_fin.quality_loss;
    assert!(!s_fin.tool_life.unfinished && s_fin.tool_life.expired, "the expired tool finishes the program");
    assert!(q.final_points > 0 && q.degraded_points == q.final_points, "the expired tool cuts the whole finishing contour: {} of {}", q.degraded_points, q.final_points);
    assert!(past(&s_fin) > past(&s_stop) && q.worst_extra_wear_um > 0.0, "walls past the limit are left thicker: +{:.1} µm", q.worst_extra_wear_um);
    dump_for_preview(&with_policy("탄소강", ToolLifePolicy::FinishPart), &ctx_fin, "pocket_zigzag", &card);
    // 예비 공구로 이어서: 끝까지 가고, 새 공구가 정삭 윤곽을 깎아 최종 벽은 정상
    let (_, s_change) = worn(ToolLifePolicy::ChangeTool);
    print_quality("탄소강 포켓 · 한계 가까운 공구 · 예비 공구로 이어서", &s_change);
    assert!(!s_change.tool_life.unfinished && s_change.tool_life.tool_changes >= 1);
    check_zones(&s_change, ds_mm);
    assert!(s_change.quality_loss.good_points > 0 && past(&s_change) < past(&s_fin), "the sister tool finishes the walls: {} vs {}", past(&s_change), past(&s_fin));
    assert!(toollife::damage_choice(&s_change.tool_life).is_none(), "a finished path asks nothing");
}

#[test]
fn an_option_that_repeats_the_policy_already_used_is_blocked() {
    let mut tl = simulate(&with_policy("인코넬", ToolLifePolicy::Stop), "pocket_zigzag").tool_life;
    assert!(tl.unfinished);
    tl.policy = ToolLifePolicy::ChangeTool;
    let c = toollife::damage_choice(&tl).unwrap();
    let ch = c.options.iter().find(|o| o.key == "change_tool").unwrap();
    assert!(!ch.enabled && ch.blocked_by[0].contains("이미 이 정책"));
    assert_eq!(c.recommended, "stop");
    tl.policy = ToolLifePolicy::FinishPart;
    let c = toollife::damage_choice(&tl).unwrap();
    assert!(!c.options.iter().find(|o| o.key == "finish_part").unwrap().enabled);
    assert!(c.options.iter().find(|o| o.key == "change_tool").unwrap().enabled);
}

#[test]
fn superseding_a_stopped_job_returns_the_tool_to_its_state_before_the_part() {
    let p = with_policy("인코넬", ToolLifePolicy::Stop);
    let dir = std::env::temp_dir().join(format!("endmill-supersede-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let mut store = Store::open(&dir.join("library")).unwrap();
    let mut sds = endmill_model::sds::SdsStore::open(&dir.join("sds"));

    let run = |store: &mut Store, sds: &mut endmill_model::sds::SdsStore, p: &MachiningProfile, stamp: &str, parent: Option<i64>| {
        let (mut ctx, _) = endmill_model::pipeline::calibrated_context(p, Some(&*sds));
        let (mut row, start, _) = store.prepare_tool(p, &mut ctx).unwrap();
        let tool_before = row.clone();
        let (_, rep, segs) = endmill_model::viewport::build_report_prog(p, "pocket_zigzag", &ctx, &mut |_| true).unwrap();
        let log = endmill_model::wearcomp::analyze_job_with(p, &ctx, &rep, None, sds, Some(&start));
        let opts = endmill_model::wearcomp::CompOptions::default();
        let draft = endmill_model::wearcomp::evaluate(p, &ctx, &rep, &log, &opts, None);
        let dec = endmill_model::wearcomp::consult(&draft, None, None, false);
        let comp = endmill_model::wearcomp::finalize(p, &ctx, &rep, &segs, &log, &draft, &dec, &opts);
        let _ = endmill_model::wearcomp::record(sds, p, &log, &comp, None);
        let em = row.endmill_id;
        let out = record_job(
            &store.rdb,
            Some(&mut row),
            &JobInput {
                project_id: store.project_id,
                profile: p,
                ctx: &ctx,
                pattern: "pocket_zigzag",
                stamp,
                parent_job_id: parent,
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
            println!("[SUPERSEDE] {} {}", stamp, l);
        }
        (out.job_id, tool_before, rep, ctx, segs, start)
    };

    let (job, before, rep, ctx, segs, start) = run(&mut store, &mut sds, &p, "STOP1", None);
    assert!(rep.tool_life.unfinished);
    let tid = before.id;
    let charged = store.rdb.tool_by_id(tid).unwrap().unwrap();
    assert!(charged.cut_min > before.cut_min + 1e-6 && charged.jobs == before.jobs + 1, "the stopped part was charged to the tool");

    // 작업자가 2차를 다시 고른 재계획 — 앱은 직전 1차 결과를 다시 쓰고 '재계획' 작업을 남긴다 (1차는 공구에 다시 쌓지 않음).
    // 이어 가기가 그 재계획에서 시작해도 되돌릴 것은 원 작업의 1차다
    let replan_job = {
        let mut row = store.rdb.tool_by_id(tid).unwrap().unwrap();
        let log = endmill_model::wearcomp::analyze_job_with(&p, &ctx, &rep, None, &sds, Some(&start));
        let opts = endmill_model::wearcomp::CompOptions::default();
        let draft = endmill_model::wearcomp::evaluate(&p, &ctx, &rep, &log, &opts, None);
        let dec = endmill_model::wearcomp::consult(&draft, None, None, false);
        let comp = endmill_model::wearcomp::finalize(&p, &ctx, &rep, &segs, &log, &draft, &dec, &opts);
        let em = row.endmill_id;
        let out = record_job(
            &store.rdb,
            Some(&mut row),
            &JobInput {
                project_id: store.project_id,
                profile: &p,
                ctx: &ctx,
                pattern: "pocket_zigzag",
                stamp: "STOP1-R",
                parent_job_id: Some(job),
                endmill_id: em,
                workpiece_id: None,
                report: &rep,
                log: &log,
                comp: &comp,
                replan: true,
                summary: serde_json::json!({}),
            },
        )
        .unwrap();
        out.job_id
    };
    assert_ne!(replan_job, job);
    let replanned = store.rdb.tool_by_id(tid).unwrap().unwrap();
    assert!((replanned.cut_min - charged.cut_min).abs() < 1e-9, "a replan does not charge the first pass again");

    let lines = store.rdb.supersede_job(replan_job, &ctx.tool, ctx.vb_limit_mm()).unwrap();
    for l in lines.iter() {
        println!("[SUPERSEDE] {}", l);
    }
    assert!(store.rdb.job_passes(replan_job).unwrap().iter().all(|p| p.status == "superseded"), "the replan is superseded with its original");
    let back = store.rdb.tool_by_id(tid).unwrap().unwrap();
    assert!(back.retired_at.is_none(), "the tool is mounted again for the re-run");
    assert!((back.cut_min - before.cut_min).abs() < 1e-9 && back.jobs == before.jobs, "cut time and job count back: {:.4} / {}", back.cut_min, back.jobs);
    assert!((back.vb_band_mm - before.vb_band_mm).abs() < 1e-6, "VB back to the state before the part: {:.4} vs {:.4}", back.vb_band_mm, before.vb_band_mm);
    assert!(back.life_used_max < 1e-6 && back.pending_job_id.is_none());
    assert!(store.rdb.job_passes(job).unwrap().iter().all(|p| p.status == "superseded" && !p.committed));
    let recs = store.rdb.candidate_records(&material_key(&p), ctx.tool.flutes as i64, None, 1000).unwrap();
    assert!(recs.iter().all(|r| r.job_id != job), "a superseded job is no longer a reference");
    assert!(store.rdb.supersede_job(job, &ctx.tool, ctx.vb_limit_mm()).is_err(), "a job is superseded only once");

    // 고른 정책(손상 공구로 끝까지)으로 같은 부품을 다시 계산하면 같은 공구에서 시작하고, 공구에는 다시 계산한 작업만 남는다
    let pf = with_policy("인코넬", ToolLifePolicy::FinishPart);
    let (job2, start2, rep2, _, _, _) = run(&mut store, &mut sds, &pf, "FIN1", Some(replan_job));
    assert_eq!(start2.id, tid, "the re-run starts from the same tool");
    assert!((start2.cut_min - before.cut_min).abs() < 1e-9);
    let after = store.rdb.tool_by_id(tid).unwrap().unwrap();
    assert_eq!(after.jobs, before.jobs + 1, "only the re-run is on the tool");
    assert!((after.cut_min - (before.cut_min + rep2.cut_time_min)).abs() < 1e-6, "{:.4} vs {:.4}", after.cut_min, rep2.cut_time_min);
    assert!(rep2.cut_time_min >= rep.cut_time_min - 1e-9);
    let d2 = store.rdb.job_detail(job2).unwrap().unwrap();
    assert_eq!(d2.job.parent_job_id, Some(replan_job), "the re-run links the job it replaced");
    let _ = std::fs::remove_dir_all(&dir);
}
