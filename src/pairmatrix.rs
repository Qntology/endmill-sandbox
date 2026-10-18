//! 공구 × 피삭재 N:N 조합 행렬 — 내장 공구 8개를 내장 소재 8개(사용 사례)에 모두 대어 본다.
//!
//! 프리셋은 공구 하나를 '그 공구가 만들어진 소재' 하나에만 짝지어 두었다. 현장에서는 공구가 다른 소재에도 쓰이고, 그때의 결과
//! (수명·치핑·파손·구성인선·화학 궁합)를 미리 보는 것이 이 행렬의 목적이다.
//!
//! 사용 사례(소재 축): 소재마다 그 소재 프리셋의 공작물 설정·냉각·목적(황삭/정삭)을 쓰고, 절삭 조건은 소재의 카탈로그 기본값
//! (`CuttingCalculator::recommend_conditions` — 공구 직경·날 수·등급만 반영)으로 둔다. 즉 "이 공구를 그 소재의 표준 조건으로 쓰면"이다.
//! 합금강 30 HRC (SCM440 계열) 는 프리셋이 없어 탄소강 블록·MQL·황삭으로 둔다.
//!
//! 공구 축: 형상(경사·여유·날끝 반경)을 입력하지 않은 내장 공구(Ø10 nACo · Ø6 TiAlN · Ø12 볼)는 지금까지 '깎는 소재의 기본값'을
//! 형상으로 받아 왔다 — 그러면 어느 소재에 대도 그 소재용 공구가 되어 버린다. 행렬에서는 공구가 설계된 소재의 기본값으로 형상을
//! 고정한다 (`freeze_geometry`). 공구 개체가 실제로 가진 형상으로 다른 소재를 깎는 상황이다.
use crate::cutting::WorkpieceMaterial;
use crate::physics::{self, CutContext, ThermalBody, WorkpieceProps};
use crate::profile::{EndMillMockupSetting, MachineLimits, MachiningPreset, MachiningProfile};
use crate::workpiece_setup::WorkpieceSetup;
use serde::{Deserialize, Serialize};

/// 내장 공구가 설계된 소재 (형상을 입력하지 않은 공구만 — 나머지는 형상이 이미 고정돼 있다)
pub fn builtin_design_material(s: &EndMillMockupSetting) -> Option<WorkpieceMaterial> {
    match s.model.as_str() {
        // OSG ADO-SUS = 스테인리스(SUS)용 고능률 4날
        "OSG ADO-SUS-4D" => Some(WorkpieceMaterial::StainlessSteel),
        "YG-1 AL-2F" => Some(WorkpieceMaterial::Aluminum),
        // 범용 AlTiN 볼 — 탄소강 기준 형상
        "Sandvik R216.34" => Some(WorkpieceMaterial::CarbonSteel),
        _ => None,
    }
}

/// 입력 안 된 경사각·여유각·날끝 반경을 `design` 소재 기준 값으로 고정한다 (`ToolGeometry::from_setting` 과 같은 규칙)
pub fn freeze_geometry(s: &mut EndMillMockupSetting, design: &WorkpieceMaterial, machine: &MachineLimits) {
    let wp = WorkpieceProps::of(design);
    let g = physics::ToolGeometry::from_setting(s, &wp, machine, Some(&s.substrate.base()));
    if s.rake_deg.is_none() {
        s.rake_deg = Some(g.rake_deg);
    }
    if s.clearance_deg.is_none() {
        s.clearance_deg = Some(g.clearance_deg);
    }
    if s.edge_radius_um.is_none() {
        s.edge_radius_um = Some(g.edge_radius_um);
    }
}

/// 소재 축의 사용 사례
#[derive(Debug, Clone)]
pub struct UseCase {
    pub key: &'static str,
    pub label: &'static str,
    pub setup: WorkpieceSetup,
    pub coolant: crate::profile::CoolantConfig,
    pub purpose: &'static str,
    /// 이 사용 사례의 내장 프리셋 공구 모델 (그 소재의 '의도된' 공구)
    pub intended_model: &'static str,
}

pub fn use_cases() -> Vec<UseCase> {
    let p = |name: &str| MachiningPreset::builtin().into_iter().find(|x| x.name.contains(name)).expect("내장 프리셋");
    let al = p("알루미늄 황삭");
    let ti = p("티타늄");
    let st = p("탄소강");
    let ss = p("스테인리스");
    let ni = p("인코넬");
    let hd = p("금형강");
    let cf = p("CFRP");
    let mut alloy_setup = WorkpieceSetup::default_steel_block();
    alloy_setup.material = WorkpieceMaterial::AlloySteel { hardness_hrc: 30 };
    vec![
        UseCase { key: "aluminum", label: "알루미늄 (황삭·플러드)", setup: al.workpiece_setup.clone(), coolant: al.coolant_config.clone(), purpose: "roughing", intended_model: "YG-1 AL-2F" },
        UseCase { key: "carbon_steel", label: "탄소강 S45C (황삭·MQL)", setup: st.workpiece_setup.clone(), coolant: st.coolant_config.clone(), purpose: "roughing", intended_model: "OSG ADO-SUS-4D" },
        UseCase { key: "alloy_steel_30", label: "합금강 30 HRC (황삭·MQL)", setup: alloy_setup, coolant: st.coolant_config.clone(), purpose: "roughing", intended_model: "OSG ADO-SUS-4D" },
        UseCase { key: "stainless", label: "스테인리스 SUS304 (정삭·플러드)", setup: ss.workpiece_setup.clone(), coolant: ss.coolant_config.clone(), purpose: "finishing", intended_model: "BUILTIN-AC8F3" },
        UseCase { key: "titanium", label: "티타늄 Ti-6Al-4V (정삭·내부 급유)", setup: ti.workpiece_setup.clone(), coolant: ti.coolant_config.clone(), purpose: "finishing", intended_model: "OSG ADO-SUS-4D" },
        UseCase { key: "inconel", label: "인코넬 718 (황삭·내부 급유)", setup: ni.workpiece_setup.clone(), coolant: ni.coolant_config.clone(), purpose: "roughing", intended_model: "BUILTIN-AT12F5" },
        UseCase { key: "hardened_58", label: "금형강 58 HRC (정삭·에어)", setup: hd.workpiece_setup.clone(), coolant: hd.coolant_config.clone(), purpose: "finishing", intended_model: "BUILTIN-HB6N18" },
        UseCase { key: "cfrp", label: "CFRP (트리밍·에어)", setup: cf.workpiece_setup.clone(), coolant: cf.coolant_config.clone(), purpose: "roughing", intended_model: "BUILTIN-DC10F6" },
    ]
}

/// 행렬의 칸 하나
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MatrixCell {
    pub tool: String,
    pub tool_model: String,
    pub coating: String,
    pub substrate: String,
    pub use_case: String,
    pub material: String,
    /// 이 소재의 내장 프리셋 공구인가
    pub intended: bool,
    pub vc_m_min: f64,
    pub fz_mm: f64,
    pub ap_mm: f64,
    pub ae_mm: f64,
    pub rpm: f64,
    /// 공칭 해석: 공구 수명 (분, VB 한계까지, 채터 배수 포함), 지배 마모 기구, 계면 온도, 채터 여유, 측벽 Ra
    pub life_min: f64,
    /// 재료 제거율 (cm³/min) 과 공구 하나가 수명 동안 깎는 체적 (cm³) — 직경·조건이 다른 공구끼리 비교하는 기준
    pub mrr_cm3_min: f64,
    pub life_volume_cm3: f64,
    pub dominant: String,
    pub interface_c: f64,
    pub chatter_margin: f64,
    pub ra_um: f64,
    pub spindle_kw: f64,
    pub power_ok: bool,
    /// 코팅–피삭재 화학 궁합 (0 문제 없음 ~ 3 심각)
    pub compat_severity: u8,
    pub compat: Vec<String>,
    /// 형상 용도 적합도
    pub fit_verdict: u8,
    pub fit_score: f64,
    pub fit_wear: f64,
    pub fit_chip: f64,
    pub fit_bue: f64,
    pub fit_evac: f64,
    pub fit_reasons: Vec<String>,
    /// 경로 시뮬레이션 (선택)
    pub path_pattern: String,
    pub path_state: String,
    pub path_completed: f64,
    pub path_failure: String,
    pub path_vb_end_mm: f64,
    pub path_life_used_max: f64,
    pub path_stress_ratio: f64,
    pub path_fatigue: f64,
    pub path_chip_p: f64,
    pub path_wall_max_um: f64,
    pub path_ra_max_um: f64,
    pub path_cut_min: f64,
    /// 종합 판정 0 적합 · 1 주의 · 2 부적합 과 그 이유
    pub verdict: u8,
    pub reasons: Vec<String>,
    /// 같은 소재에서 가장 긴 수명 대비 이 공구의 수명
    pub life_vs_best: f64,
    /// 그 소재에서의 순위 — 판정(적합→주의→부적합) 다음 수명 ÷ √치핑 배수 × (1 − 치핑 확률/2)
    pub rank_in_material: usize,
}

impl MatrixCell {
    pub fn verdict_label(&self) -> &'static str {
        ["적합", "주의", "부적합"][self.verdict.min(2) as usize]
    }
}

fn fin(v: f64) -> f64 {
    if v.is_finite() {
        v
    } else {
        0.0
    }
}

/// 공구 하나를 사용 사례 하나에 대 본 프로필 (형상 고정 포함)
pub fn cell_profile(tool: &EndMillMockupSetting, uc: &UseCase) -> MachiningProfile {
    let mut t = tool.clone();
    if let Some(design) = builtin_design_material(tool) {
        freeze_geometry(&mut t, &design, &MachineLimits::default());
    }
    let name = format!("{} × {}", tool.name, uc.label);
    let preset = MachiningPreset::from_parts(&name, "공구×소재 N:N 행렬", t, uc.setup.clone(), uc.coolant.clone(), uc.purpose);
    MachiningProfile::from_preset(&preset, &name)
}

/// 행렬의 칸 하나를 계산한다. `pattern` 이 있으면 그 경로로 시뮬레이션까지 돌린다 (공구 수명 감시 포함)
pub fn evaluate_cell(tool: &EndMillMockupSetting, uc: &UseCase, pattern: Option<&str>) -> MatrixCell {
    let p = cell_profile(tool, uc);
    let ctx = CutContext::from_profile(&p);
    let body = ThermalBody::of_setup(&p.workpiece_setup, &ctx.wp);
    let a = physics::analyze_cut(&ctx, &p.conditions, true, body);
    let fit = crate::appfit::evaluate(&ctx.tool, &ctx.wp, false);
    let compat = &a.tribology.compat;
    let mut cell = MatrixCell {
        tool: tool.name.clone(),
        tool_model: tool.model.clone(),
        coating: ctx.tool.coating.family.clone(),
        substrate: ctx.tool.substrate.clone(),
        use_case: uc.key.into(),
        material: ctx.wp.label.clone(),
        intended: tool.model == uc.intended_model,
        vc_m_min: fin(a.vc_m_min),
        fz_mm: fin(a.fz_mm),
        ap_mm: fin(a.ap_mm),
        ae_mm: fin(a.ae_mm),
        rpm: fin(a.rpm),
        life_min: fin(a.wear.tool_life_min),
        mrr_cm3_min: fin(a.mrr_cm3_min),
        life_volume_cm3: fin(a.wear.tool_life_min * a.mrr_cm3_min),
        dominant: a.wear.dominant.clone(),
        interface_c: fin(a.thermal.interface_c),
        chatter_margin: fin(a.stability.margin),
        ra_um: fin(a.surface.wall_ra_um),
        spindle_kw: fin(a.spindle_power_kw),
        power_ok: a.spindle_power_kw <= a.available_power_kw,
        compat_severity: compat.severity,
        compat: compat.messages.clone(),
        fit_verdict: fit.verdict,
        fit_score: fin(fit.score),
        fit_wear: fin(fit.wear_factor),
        fit_chip: fin(fit.chip_factor),
        fit_bue: fin(fit.bue_factor),
        fit_evac: fin(fit.evac_risk),
        fit_reasons: fit.reasons(),
        ..Default::default()
    };
    if let Some(pk) = pattern {
        cell.path_pattern = pk.into();
        match crate::viewport::build_report_prog(&p, pk, &ctx, &mut |_| true) {
            Ok((_, r, _)) => {
                let tl = &r.tool_life;
                cell.path_state = tl.state.clone();
                cell.path_completed = fin(tl.completed_fraction());
                cell.path_failure = tl.failure_cause.clone();
                cell.path_vb_end_mm = fin(r.vb_end_mm);
                cell.path_life_used_max = fin(tl.life_used_max);
                cell.path_stress_ratio = fin(tl.peak_stress_ratio);
                cell.path_fatigue = fin(tl.fatigue_damage);
                cell.path_chip_p = fin(tl.chip_probability);
                cell.path_wall_max_um = fin(r.max_wall_error_um);
                cell.path_ra_max_um = fin(r.wall_ra_max_um);
                cell.path_cut_min = fin(r.cut_time_min);
            }
            Err(e) => {
                cell.path_state = format!("오류: {}", e);
            }
        }
    }
    // ---- 종합 판정 ----
    let mut verdict: u8 = 0;
    let mut reasons: Vec<String> = Vec::new();
    if compat.severity >= 3 {
        verdict = 2;
        reasons.push(format!("코팅 화학 궁합 심각 ({})", compat.messages.first().cloned().unwrap_or_default()));
    } else if compat.severity >= 2 {
        verdict = verdict.max(1);
        reasons.push(format!("코팅 화학 궁합 주의 ({})", compat.messages.first().cloned().unwrap_or_default()));
    }
    if fit.verdict >= 1 {
        verdict = verdict.max(fit.verdict);
        reasons.push(fit.summary());
    }
    if !cell.power_ok {
        verdict = verdict.max(1);
        reasons.push(format!("필요 동력 {:.2} kW 가 가용 동력 초과", cell.spindle_kw));
    }
    if pattern.is_some() {
        if !cell.path_failure.is_empty() {
            verdict = 2;
            reasons.push(format!("경로 시뮬레이션에서 공구 파손 ({}, 경로 {:.0}% 지점)", cell.path_failure, cell.path_completed * 100.0));
        } else if cell.path_completed < 0.999 {
            verdict = verdict.max(1);
            reasons.push(format!("경로 {:.0}% 에서 수명 판정으로 정지", cell.path_completed * 100.0));
        }
        if cell.path_chip_p >= 0.5 {
            verdict = verdict.max(1);
            reasons.push(format!("경로 치핑 누적 확률 {:.0}%", cell.path_chip_p * 100.0));
        }
    }
    cell.verdict = verdict;
    cell.reasons = reasons;
    cell
}

/// 행렬 전체 (공구 순서 = `EndMillMockupSetting::builtin_tools`, 소재 순서 = `use_cases`)
pub fn run(pattern: Option<&str>) -> Vec<MatrixCell> {
    let tools = EndMillMockupSetting::builtin_tools();
    let ucs = use_cases();
    let mut cells: Vec<MatrixCell> = Vec::with_capacity(tools.len() * ucs.len());
    for uc in ucs.iter() {
        for t in tools.iter() {
            cells.push(evaluate_cell(t, uc, pattern));
        }
    }
    rank(&mut cells);
    cells
}

/// 소재마다 순위와 최고 수명 대비 비를 채운다. 순서는 판정(적합 → 주의 → 부적합)이 먼저이고, 같은 판정 안에서는
/// 수명(분, 그 공구의 카탈로그 조건) ÷ √치핑 배수 × (1 − 치핑 확률/2). 수명 체적(cm³)은 공구 크기가 지배해 순위가 아니라 참고로 남긴다
pub fn rank(cells: &mut [MatrixCell]) {
    let mut keys: Vec<String> = cells.iter().map(|c| c.use_case.clone()).collect();
    keys.dedup();
    for k in keys.iter() {
        let idx: Vec<usize> = (0..cells.len()).filter(|i| &cells[*i].use_case == k).collect();
        let merit = |c: &MatrixCell| -> f64 { c.life_min.max(0.0) / c.fit_chip.max(1.0).sqrt() * (1.0 - 0.5 * c.path_chip_p.clamp(0.0, 1.0)) };
        let best = idx.iter().map(|i| cells[*i].life_min).fold(0.0, f64::max).max(1e-9);
        let mut order = idx.clone();
        order.sort_by(|a, b| {
            let (ca, cb) = (&cells[*a], &cells[*b]);
            ca.verdict.cmp(&cb.verdict).then(merit(cb).partial_cmp(&merit(ca)).unwrap_or(std::cmp::Ordering::Equal))
        });
        for (r, i) in order.iter().enumerate() {
            cells[*i].rank_in_material = r + 1;
            cells[*i].life_vs_best = cells[*i].life_min / best;
        }
    }
}

pub fn csv(cells: &[MatrixCell]) -> String {
    let mut s = String::from("use_case,material,tool,tool_model,coating,substrate,intended,vc_m_min,fz_mm,ap_mm,ae_mm,rpm,life_min,mrr_cm3_min,life_volume_cm3,life_vs_best,rank,dominant,interface_c,chatter_margin,ra_um,spindle_kw,power_ok,compat_severity,fit_verdict,fit_score,fit_wear,fit_chip,fit_bue,fit_evac,path_pattern,path_state,path_completed,path_failure,path_vb_end_mm,path_life_used_max,path_stress_ratio,path_fatigue,path_chip_p,path_wall_max_um,path_ra_max_um,path_cut_min,verdict,reasons\n");
    let q = |x: &str| format!("\"{}\"", x.replace('"', "'"));
    for c in cells.iter() {
        s.push_str(&format!(
            "{},{},{},{},{},{},{},{:.1},{:.4},{:.2},{:.2},{:.0},{:.2},{:.3},{:.1},{:.3},{},{},{:.0},{:.3},{:.3},{:.3},{},{},{},{:.3},{:.3},{:.2},{:.3},{:.3},{},{},{:.3},{},{:.4},{:.3},{:.3},{:.4},{:.3},{:.2},{:.3},{:.3},{},{}\n",
            c.use_case,
            q(&c.material),
            q(&c.tool),
            q(&c.tool_model),
            c.coating,
            c.substrate,
            c.intended,
            c.vc_m_min,
            c.fz_mm,
            c.ap_mm,
            c.ae_mm,
            c.rpm,
            c.life_min,
            c.mrr_cm3_min,
            c.life_volume_cm3,
            c.life_vs_best,
            c.rank_in_material,
            c.dominant,
            c.interface_c,
            c.chatter_margin,
            c.ra_um,
            c.spindle_kw,
            c.power_ok,
            c.compat_severity,
            c.fit_verdict,
            c.fit_score,
            c.fit_wear,
            c.fit_chip,
            c.fit_bue,
            c.fit_evac,
            c.path_pattern,
            q(&c.path_state),
            c.path_completed,
            c.path_failure,
            c.path_vb_end_mm,
            c.path_life_used_max,
            c.path_stress_ratio,
            c.path_fatigue,
            c.path_chip_p,
            c.path_wall_max_um,
            c.path_ra_max_um,
            c.path_cut_min,
            c.verdict,
            q(&c.reasons.join(" / "))
        ));
    }
    s
}

/// 사람이 읽는 줄: 칸마다 `[MATRIX]` 한 줄 + 소재별 판정 격자
pub fn lines(cells: &[MatrixCell]) -> Vec<String> {
    let mut v = Vec::new();
    for c in cells.iter() {
        v.push(format!(
            "[MATRIX] {} × {}{} | {} {} · Vc {:.0} fz {:.4} ap {:.1} ae {:.2} | 수명 {:.1} min × MRR {:.2} cm³/min = {:.0} cm³ (최고 대비 {:.2}, {}위) · {} · 계면 {:.0}°C · 채터 여유 {:.2} · Ra {:.2} µm | 궁합 {} · 적합도 {} (마모 ×{:.2} 치핑 ×{:.1} 구성인선 ×{:.2} 칩막힘 {:.0}%){} | 판정 {}{}",
            c.use_case,
            c.tool,
            if c.intended { " [의도된 짝]" } else { "" },
            c.coating,
            c.substrate,
            c.vc_m_min,
            c.fz_mm,
            c.ap_mm,
            c.ae_mm,
            c.life_min,
            c.mrr_cm3_min,
            c.life_volume_cm3,
            c.life_vs_best,
            c.rank_in_material,
            c.dominant,
            c.interface_c,
            c.chatter_margin,
            c.ra_um,
            c.compat_severity,
            ["적합", "주의", "부적합"][c.fit_verdict.min(2) as usize],
            c.fit_wear,
            c.fit_chip,
            c.fit_bue,
            c.fit_evac * 100.0,
            if c.path_pattern.is_empty() {
                String::new()
            } else {
                format!(
                    " | 경로 {} {} 완료 {:.0}%{} VB {:.3} 수명 {:.0}% 응력 ×{:.2} 피로 {:.3} 치핑 {:.0}% 벽 {:.1} µm Ra {:.2}",
                    c.path_pattern,
                    c.path_state,
                    c.path_completed * 100.0,
                    if c.path_failure.is_empty() { String::new() } else { format!(" 파손({})", c.path_failure) },
                    c.path_vb_end_mm,
                    c.path_life_used_max * 100.0,
                    c.path_stress_ratio,
                    c.path_fatigue,
                    c.path_chip_p * 100.0,
                    c.path_wall_max_um,
                    c.path_ra_max_um
                )
            },
            c.verdict_label(),
            if c.reasons.is_empty() { String::new() } else { format!(" — {}", c.reasons.join(" / ")) }
        ));
    }
    // 격자: 행 = 공구, 열 = 소재 (판정 · 순위)
    let mut ucs: Vec<String> = cells.iter().map(|c| c.use_case.clone()).collect();
    ucs.dedup();
    let mut tools: Vec<String> = Vec::new();
    for c in cells.iter() {
        if !tools.contains(&c.tool) {
            tools.push(c.tool.clone());
        }
    }
    v.push(format!("[MATRIX-GRID] 공구 \\ 소재 | {}", ucs.join(" | ")));
    for t in tools.iter() {
        let row: Vec<String> = ucs
            .iter()
            .map(|u| {
                cells
                    .iter()
                    .find(|c| &c.tool == t && &c.use_case == u)
                    .map(|c| format!("{}{}{}", ["○", "△", "×"][c.verdict.min(2) as usize], c.rank_in_material, if c.intended { "*" } else { "" }))
                    .unwrap_or_else(|| "-".into())
            })
            .collect();
        v.push(format!("[MATRIX-GRID] {} | {}", t, row.join(" | ")));
    }
    v.push("[MATRIX-GRID] ○ 적합 · △ 주의 · × 부적합 · 숫자 = 그 소재에서의 순위 · * = 내장 프리셋의 의도된 짝".into());
    v
}
