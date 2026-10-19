//! 문헌 실측 벤치마크 — 참조 논문·리서치 정리에 나온 공구·소재·절삭 조건을 샌드박스의 공구 데이터(`EndMillMockupSetting`)와
//! 소재 데이터(`WorkpieceMaterial` → `WorkpieceProps`)로 그대로 옮겨 N:N 으로 돌리고, 공칭 해석(`physics::analyze_cut`)이 낸
//! 수명·마모 속도·계면 온도·절삭력·조도를 논문의 실측과 대 본다.
//!
//! 세 가지를 본다.
//! * 절댓값 — 논문 공구로 논문 조건을 깎았을 때의 실측 (수명 분, 힘 N, 온도 °C, Ra µm). 대역(lo~hi)은 논문의 반복 편차·측정법 차이·
//!   논문이 밝히지 않은 값(날끝 반경·돌출 등)이 만드는 폭이다.
//! * 비 — 같은 논문 안에서 공구(코팅)나 소재만 바꾼 두 칸의 비 (예: TiAlN/무코팅 수명). 기준 앵커·날끝 상태처럼 모르는 값이 약분된다.
//! * 민감도 — 조건 하나를 바꿨을 때 수명의 로그 기울기 d ln T / d ln x (예: Ti-6Al-4V Vc 100→120 m/min 에서 −4, Tatar 외 2020).
//!   절대 수명이 기준 앵커(`WorkpieceProps::ref_life_min`)에 묶여 있어도 물리 기울기는 따로 시험된다.
//!
//! N:N: 논문 공구(행) × 논문 소재·조건(열)을 모두 돌린다. 열의 조건은 그 논문의 Vc·fz·ap/D·ae/D·냉각·판정 VB 이고, 지름이 다른
//! 공구에는 ap/D·ae/D 를 그대로 옮긴다 (fz 는 날당 그대로). 실측이 있는 칸은 오차를 세고, 나머지 칸은 같은 열 안의 순위·판정이
//! 문헌·현장 순서와 맞는지만 본다 (`check`).
//!
//! 이 표의 숫자는 맞추는 대상이 아니라 시험이다 — 모델을 고칠 때는 어긋난 칸의 물리(기구·식·입력 해석)를 찾아 고치고, 이 표의 값에
//! 맞도록 계수를 칸마다 바꾸지 않는다. 고친 내역과 전후 결과는 `docs/literature-benchmark.md`.
use crate::cutting::{CoolantType, CuttingConditions, WorkpieceMaterial};
use crate::physics::{self, CutContext, ThermalBody, WearTrajectory};
use crate::profile::{CoolantConfig, CoolantMethod, EndMillMockupSetting, HolderType, MachiningPreset, MachiningProfile, ToolNose, ToolSubstrate};
use crate::workpiece_setup::WorkpieceSetup;
use serde::Serialize;
use std::f64::consts::PI;

/// 논문 공구 — 샌드박스 공구 데이터 형식으로 옮긴 것
#[derive(Debug, Clone)]
pub struct RefTool {
    pub key: &'static str,
    pub label: &'static str,
    /// 출처와, 논문이 밝히지 않아 가정한 값
    pub source: &'static str,
    pub setting: EndMillMockupSetting,
}

/// 논문 소재·절삭 조건 — 소재는 샌드박스 소재 그룹(`WorkpieceMaterial`)으로 옮긴다
#[derive(Debug, Clone)]
pub struct RefCut {
    pub key: &'static str,
    pub label: &'static str,
    pub source: &'static str,
    /// 이 조건을 실제로 깎은 논문 공구 (`RefTool::key`)
    pub tool: &'static str,
    pub material: WorkpieceMaterial,
    pub vc_m_min: f64,
    pub fz_mm: f64,
    /// 논문 공구 지름 기준 ap/D · ae/D (다른 지름의 공구에는 이 비율로 옮긴다)
    pub ap_over_d: f64,
    pub ae_over_d: f64,
    /// 최대 칩 두께를 일정하게 둔 시험 (Tatar 외 2020 처럼 ae 를 바꿀 때 fz 를 맞춘 것) — 있으면 fz 는 ae/D 로 다시 계산한다
    pub hex_mm: Option<f64>,
    pub down: bool,
    pub coolant: CoolantMethod,
    /// 논문의 수명 판정 VB (mm)
    pub vb_criterion_mm: f64,
}

/// 비교하는 양
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub enum Quantity {
    /// 논문 판정 VB 까지 수명 (분). 수명 시험은 안정 절삭에서 하므로 마모만의 수명 (추정 FRF 의 채터 기대 손실은 따로 본다)
    LifeMin,
    /// 수명 동안 깎은 체적 (cm³)
    LifeVolumeCm3,
    /// 정상 구간(판정 VB 의 1/3 → 2/3) 평균 VB 성장 속도 (µm/min)
    VbRateUmMin,
    /// 절삭 t 분 뒤 VB (mm)
    VbAtMm(f64),
    /// 평균 합력 (N, 이송면 xy)
    ForceMeanN,
    /// 최대 합력 (N, 이송면 xy)
    ForcePeakN,
    /// 평균 힘 벡터의 크기 |(F̄x, F̄y)| (N) — 선형 절삭 계수 식별(평균 힘 회귀)과 같은 정의
    ForceVectorMeanN,
    /// 반경/접선 힘 비 (절삭 계수 Kr = Krc/Ktc)
    RadialRatio,
    /// 공구–칩 계면 온도 (°C, 냉각 포함)
    InterfaceC,
    /// 계면 온도 상승 (°C, 계면 − 20 °C) — 온도의 속도 기울기를 보는 양
    InterfaceRiseC,
    /// 측벽 Ra (µm)
    RaUm,
}

impl Quantity {
    pub fn key(&self) -> String {
        match self {
            Self::LifeMin => "life_min".into(),
            Self::LifeVolumeCm3 => "life_volume_cm3".into(),
            Self::VbRateUmMin => "vb_rate_um_min".into(),
            Self::VbAtMm(t) => format!("vb_at_{:.0}min_mm", t),
            Self::ForceMeanN => "force_mean_n".into(),
            Self::ForcePeakN => "force_peak_n".into(),
            Self::ForceVectorMeanN => "force_vector_mean_n".into(),
            Self::RadialRatio => "radial_ratio".into(),
            Self::InterfaceC => "interface_c".into(),
            Self::InterfaceRiseC => "interface_rise_c".into(),
            Self::RaUm => "ra_um".into(),
        }
    }

    pub fn label(&self) -> String {
        match self {
            Self::LifeMin => "수명 (min)".into(),
            Self::LifeVolumeCm3 => "수명 체적 (cm³)".into(),
            Self::VbRateUmMin => "VB 성장 (µm/min)".into(),
            Self::VbAtMm(t) => format!("{:.0}분 뒤 VB (mm)", t),
            Self::ForceMeanN => "평균 합력 (N)".into(),
            Self::ForcePeakN => "최대 합력 (N)".into(),
            Self::ForceVectorMeanN => "평균 힘 벡터 (N)".into(),
            Self::RadialRatio => "Krc/Ktc".into(),
            Self::InterfaceC => "계면 온도 (°C)".into(),
            Self::InterfaceRiseC => "계면 온도 상승 (°C)".into(),
            Self::RaUm => "측벽 Ra (µm)".into(),
        }
    }

    /// 비교 단위: 온도는 차이(°C)가 의미 있지만 대역 판정은 다른 양과 같게 비로 한다
    fn is_ratio_scale(&self) -> bool {
        true
    }
}

/// 조건 하나 (민감도용)
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub enum Param {
    Vc,
    Fz,
    Ae,
    Ap,
}

impl Param {
    pub fn key(&self) -> &'static str {
        match self {
            Self::Vc => "vc",
            Self::Fz => "fz",
            Self::Ae => "ae",
            Self::Ap => "ap",
        }
    }
}

/// 실측의 확신도 — 높음: 조건·실측을 논문 본문에서 다 읽음 · 보통: 조건 일부를 가정 · 낮음: 그림 판독·2차 인용
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Confidence {
    High,
    Medium,
    Low,
}

impl Confidence {
    pub fn label(&self) -> &'static str {
        match self {
            Self::High => "높음",
            Self::Medium => "보통",
            Self::Low => "낮음",
        }
    }
}

/// 절댓값 실측 — 논문 공구(tool)로 논문 조건(cut)을 깎은 결과
#[derive(Debug, Clone)]
pub struct Absolute {
    pub id: &'static str,
    pub tool: &'static str,
    pub cut: &'static str,
    pub quantity: Quantity,
    pub value: f64,
    pub lo: f64,
    pub hi: f64,
    pub confidence: Confidence,
    pub basis: &'static str,
}

/// 같은 논문 안의 두 칸의 비 (num / den)
#[derive(Debug, Clone)]
pub struct Ratio {
    pub id: &'static str,
    pub num: (&'static str, &'static str),
    pub den: (&'static str, &'static str),
    pub quantity: Quantity,
    pub value: f64,
    pub lo: f64,
    pub hi: f64,
    pub confidence: Confidence,
    pub basis: &'static str,
}

/// 조건 하나를 `from` → `to` 배로 바꿀 때 d ln Q / d ln x
#[derive(Debug, Clone)]
pub struct Sensitivity {
    pub id: &'static str,
    pub tool: &'static str,
    pub cut: &'static str,
    pub quantity: Quantity,
    pub param: Param,
    pub from: f64,
    pub to: f64,
    pub value: f64,
    pub lo: f64,
    pub hi: f64,
    pub confidence: Confidence,
    pub basis: &'static str,
}

/// 칸 하나의 샌드박스 예측
#[derive(Debug, Clone, Serialize, Default)]
pub struct Prediction {
    pub tool: String,
    pub cut: String,
    pub material: String,
    pub coating: String,
    pub diameter_mm: f64,
    pub flutes: u32,
    pub vc_m_min: f64,
    pub fz_mm: f64,
    pub ap_mm: f64,
    pub ae_mm: f64,
    pub rpm: f64,
    pub vb_criterion_mm: f64,
    /// 논문 판정 VB 까지 수명 (분) — 채터 기대 손실 포함 / 제외
    pub life_min: f64,
    pub life_no_chatter_min: f64,
    pub vb_rate_um_min: f64,
    pub mrr_cm3_min: f64,
    pub life_volume_cm3: f64,
    pub interface_c: f64,
    pub interface_dry_c: f64,
    pub f_res_mean_n: f64,
    pub f_res_peak_n: f64,
    pub f_vec_mean_n: f64,
    pub torque_nm: f64,
    pub power_kw: f64,
    pub radial_ratio: f64,
    pub ra_um: f64,
    pub chatter_margin: f64,
    pub chatter_factor: f64,
    pub dominant: String,
    pub compat_severity: u8,
    pub fit_verdict: u8,
    #[serde(skip)]
    pub trajectory: WearTrajectory,
}

impl Prediction {
    pub fn value(&self, q: Quantity) -> f64 {
        match q {
            Quantity::LifeMin => self.life_no_chatter_min,
            Quantity::LifeVolumeCm3 => self.life_volume_cm3,
            Quantity::VbRateUmMin => self.vb_rate_um_min,
            Quantity::VbAtMm(t) => self.trajectory.vb_at(t),
            Quantity::ForceMeanN => self.f_res_mean_n,
            Quantity::ForcePeakN => self.f_res_peak_n,
            Quantity::ForceVectorMeanN => self.f_vec_mean_n,
            Quantity::RadialRatio => self.radial_ratio,
            Quantity::InterfaceC => self.interface_c,
            Quantity::InterfaceRiseC => self.interface_c - 20.0,
            Quantity::RaUm => self.ra_um,
        }
    }
}

/// 비교 결과 한 줄
#[derive(Debug, Clone, Serialize)]
pub struct Comparison {
    pub id: String,
    pub kind: String,
    pub cells: String,
    pub quantity: String,
    pub measured: f64,
    pub lo: f64,
    pub hi: f64,
    pub predicted: f64,
    /// 예측 / 실측 (민감도는 차이 예측 − 실측)
    pub ratio: f64,
    /// |log10(예측/실측)| (민감도는 |차이| / 실측 대역 반폭)
    pub error: f64,
    pub in_band: bool,
    pub confidence: String,
    pub basis: String,
}

fn fin(v: f64) -> f64 {
    if v.is_finite() {
        v
    } else {
        0.0
    }
}

/// 소재에 맞는 공작물 블록 (크기·고정은 내장 설정, 소재·경도만 바꿈). 공차는 넉넉히 둬서 수명 판정은 논문 판정 VB 가 정한다
pub fn setup_for(m: &WorkpieceMaterial) -> WorkpieceSetup {
    let mut s = match m {
        WorkpieceMaterial::Aluminum => WorkpieceSetup::default_aluminum_plate(),
        WorkpieceMaterial::Titanium => WorkpieceSetup::default_titanium_plate(),
        WorkpieceMaterial::StainlessSteel => WorkpieceSetup::default_stainless_plate(),
        WorkpieceMaterial::Inconel | WorkpieceMaterial::SuperAlloy => WorkpieceSetup::default_inconel_block(),
        WorkpieceMaterial::CFRP => WorkpieceSetup::default_cfrp_panel(),
        WorkpieceMaterial::AlloySteel { hardness_hrc } if *hardness_hrc > 45 => WorkpieceSetup::default_hardened_block(),
        _ => WorkpieceSetup::default_steel_block(),
    };
    s.material = m.clone();
    s.hardness_hrc = match m {
        WorkpieceMaterial::AlloySteel { hardness_hrc } => Some(*hardness_hrc),
        _ => s.hardness_hrc,
    };
    s.tolerance_mm = Some(0.05);
    s
}

fn coolant_type(m: &CoolantMethod) -> CoolantType {
    match m {
        CoolantMethod::Flood => CoolantType::Flood,
        CoolantMethod::Mist => CoolantType::Mist,
        CoolantMethod::AirBlast => CoolantType::AirBlast,
        CoolantMethod::Dry => CoolantType::Dry,
        CoolantMethod::ThroughTool => CoolantType::ThroughTool,
    }
}

/// 공구 하나를 조건 하나에 대 본 프로필. ap/D·ae/D 는 공구 지름으로 옮기고 날 길이를 넘지 않게 한다
pub fn cell_profile(tool: &RefTool, cut: &RefCut, scale: &[(Param, f64)]) -> MachiningProfile {
    let s = &tool.setting;
    let d = s.diameter_mm;
    let k = |p: Param| scale.iter().filter(|(q, _)| *q == p).map(|(_, v)| *v).product::<f64>();
    let vc = cut.vc_m_min * k(Param::Vc);
    let ap = (cut.ap_over_d * d * k(Param::Ap)).min(s.loc_mm);
    let ae = (cut.ae_over_d * d * k(Param::Ae)).min(d);
    let fz = match cut.hex_mm {
        // 최대 칩 두께 일정: ae < D/2 이면 h_max = fz·sin φ (φ = 물림 각), 넘으면 h_max = fz
        Some(h) => {
            let sin_phi = if ae < 0.5 * d { (1.0 - 2.0 * ae / d).clamp(-1.0, 1.0).acos().sin().max(0.05) } else { 1.0 };
            h / sin_phi * k(Param::Fz)
        }
        None => cut.fz_mm * k(Param::Fz),
    };
    let rpm = vc * 1000.0 / (PI * d);
    let coolant = CoolantConfig::from_method(&cut.coolant);
    let name = format!("{} × {}", tool.key, cut.key);
    let mut preset = MachiningPreset::from_parts(&name, "문헌 실측 벤치마크", s.clone(), setup_for(&cut.material), coolant, if ae < 0.2 * d { "finishing" } else { "roughing" });
    preset.conditions = CuttingConditions {
        cutting_speed_m_min: vc,
        feed_rate_mm_min: rpm * s.flute_count.max(1) as f64 * fz,
        feed_per_tooth_mm: fz,
        axial_doc_mm: ap,
        radial_doc_mm: ae,
        spindle_rpm: rpm.round().max(1.0) as u32,
        coolant: coolant_type(&cut.coolant),
    };
    let mut p = MachiningProfile::from_preset(&preset, &name);
    // 논문 기계의 회전수·동력은 모른다 — 회전수 제한이 조건을 바꾸지 않도록 넉넉히 둔다 (공칭 해석은 조건의 회전수를 그대로 쓴다)
    p.machine.max_rpm = p.machine.max_rpm.max(1.25 * rpm);
    p.machine.max_power_kw = p.machine.max_power_kw.max(15.0);
    p
}

/// 칸 하나를 계산한다
pub fn predict(tool: &RefTool, cut: &RefCut, scale: &[(Param, f64)]) -> Prediction {
    let p = cell_profile(tool, cut, scale);
    let mut ctx = CutContext::from_profile(&p);
    // 수명 판정은 논문이 쓴 VB (부품 공차 규칙이 끼어들지 않게 공차를 넉넉히 뒀다 — 0.3 mm 를 넘는 판정은 궤적을 이어서 읽는다)
    ctx.wp.vb_criterion_mm = cut.vb_criterion_mm;
    let body = ThermalBody::of_setup(&p.workpiece_setup, &ctx.wp);
    let a = physics::analyze_cut(&ctx, &p.conditions, cut.down, body);
    let traj = a.effective_trajectory();
    let crit = cut.vb_criterion_mm;
    let life = traj.time_at(crit);
    let cwf = a.wear.chatter_factor.max(1.0);
    let (t1, t2) = (traj.time_at(crit / 3.0), traj.time_at(2.0 * crit / 3.0));
    let rate = if t2 > t1 { crit / 3.0 / (t2 - t1) * 1000.0 } else { a.wear.vb_rate_mm_per_min * 1000.0 };
    let fit = crate::appfit::evaluate(&ctx.tool, &ctx.wp, a.ae_mm >= 0.99 * ctx.tool.diameter_mm);
    Prediction {
        tool: tool.key.into(),
        cut: cut.key.into(),
        material: ctx.wp.label.clone(),
        coating: ctx.tool.coating.family.clone(),
        diameter_mm: ctx.tool.diameter_mm,
        flutes: ctx.tool.flutes,
        vc_m_min: fin(a.vc_m_min),
        fz_mm: fin(a.fz_mm),
        ap_mm: fin(a.ap_mm),
        ae_mm: fin(a.ae_mm),
        rpm: fin(a.rpm),
        vb_criterion_mm: crit,
        life_min: fin(life),
        life_no_chatter_min: fin(life * cwf),
        vb_rate_um_min: fin(rate),
        mrr_cm3_min: fin(a.mrr_cm3_min),
        life_volume_cm3: fin(life * cwf * a.mrr_cm3_min),
        interface_c: fin(a.thermal.interface_c),
        interface_dry_c: fin(a.thermal.interface_dry_c),
        f_res_mean_n: fin(a.forces.f_res_mean),
        f_res_peak_n: fin(a.forces.f_res_peak),
        f_vec_mean_n: fin((a.forces.fx_mean.powi(2) + a.forces.fy_mean.powi(2)).sqrt()),
        torque_nm: fin(a.forces.torque_mean_nm),
        power_kw: fin(a.forces.cutting_power_kw),
        radial_ratio: fin(a.coeffs.kr),
        ra_um: fin(a.surface.wall_ra_um),
        chatter_margin: fin(a.stability.margin),
        chatter_factor: fin(cwf),
        dominant: a.wear.dominant.clone(),
        compat_severity: a.tribology.compat.severity,
        fit_verdict: fit.verdict,
        trajectory: traj,
    }
}

/// 벤치마크 전체 결과
#[derive(Debug, Clone, Serialize, Default)]
pub struct Report {
    pub cells: Vec<Prediction>,
    pub comparisons: Vec<Comparison>,
    pub issues: Vec<String>,
}

fn find_tool<'a>(tools: &'a [RefTool], key: &str) -> &'a RefTool {
    tools.iter().find(|t| t.key == key).unwrap_or_else(|| panic!("벤치마크 공구 {} 없음", key))
}

fn find_cut<'a>(cuts: &'a [RefCut], key: &str) -> &'a RefCut {
    cuts.iter().find(|c| c.key == key).unwrap_or_else(|| panic!("벤치마크 조건 {} 없음", key))
}

fn band_compare(id: &str, kind: &str, cells: String, q: Quantity, measured: f64, lo: f64, hi: f64, predicted: f64, conf: Confidence, basis: &str) -> Comparison {
    let ratio = if measured.abs() > 1e-12 { predicted / measured } else { f64::NAN };
    let error = if ratio > 0.0 && ratio.is_finite() && q.is_ratio_scale() { ratio.log10().abs() } else { f64::INFINITY };
    Comparison {
        id: id.into(),
        kind: kind.into(),
        cells,
        quantity: q.key(),
        measured,
        lo,
        hi,
        predicted: fin(predicted),
        ratio: fin(ratio),
        error: if error.is_finite() { error } else { 9.0 },
        in_band: predicted >= lo && predicted <= hi,
        confidence: conf.label().into(),
        basis: basis.into(),
    }
}

/// N:N 행렬 + 실측 대조
pub fn run_with(tools: &[RefTool], cuts: &[RefCut], abs: &[Absolute], ratios: &[Ratio], sens: &[Sensitivity]) -> Report {
    let mut cells: Vec<Prediction> = Vec::with_capacity(tools.len() * cuts.len());
    for c in cuts.iter() {
        for t in tools.iter() {
            cells.push(predict(t, c, &[]));
        }
    }
    let cell = |t: &str, c: &str| cells.iter().find(|p| p.tool == t && p.cut == c).cloned().unwrap_or_else(|| predict(find_tool(tools, t), find_cut(cuts, c), &[]));
    let mut comparisons = Vec::new();
    for a in abs.iter() {
        let p = cell(a.tool, a.cut);
        comparisons.push(band_compare(a.id, "절댓값", format!("{} × {}", a.tool, a.cut), a.quantity, a.value, a.lo, a.hi, p.value(a.quantity), a.confidence, a.basis));
    }
    for r in ratios.iter() {
        let n = cell(r.num.0, r.num.1).value(r.quantity);
        let d = cell(r.den.0, r.den.1).value(r.quantity);
        let pred = if d.abs() > 1e-12 { n / d } else { f64::NAN };
        comparisons.push(band_compare(
            r.id,
            "비",
            format!("{}×{} / {}×{}", r.num.0, r.num.1, r.den.0, r.den.1),
            r.quantity,
            r.value,
            r.lo,
            r.hi,
            pred,
            r.confidence,
            r.basis,
        ));
    }
    for s in sens.iter() {
        let t = find_tool(tools, s.tool);
        let c = find_cut(cuts, s.cut);
        let q0 = predict(t, c, &[(s.param, s.from)]).value(s.quantity);
        let q1 = predict(t, c, &[(s.param, s.to)]).value(s.quantity);
        let slope = if q0 > 0.0 && q1 > 0.0 { (q1 / q0).ln() / (s.to / s.from).ln() } else { f64::NAN };
        let half = (0.5 * (s.hi - s.lo)).max(1e-9);
        comparisons.push(Comparison {
            id: s.id.into(),
            kind: "민감도".into(),
            cells: format!("{} × {} · {} ×{:.2}→×{:.2}", s.tool, s.cut, s.param.key(), s.from, s.to),
            quantity: format!("dln_{}/dln_{}", s.quantity.key(), s.param.key()),
            measured: s.value,
            lo: s.lo,
            hi: s.hi,
            predicted: fin(slope),
            ratio: fin(slope - s.value),
            error: if slope.is_finite() { (slope - s.value).abs() / half } else { 9.0 },
            in_band: slope >= s.lo && slope <= s.hi,
            confidence: s.confidence.label().into(),
            basis: s.basis.into(),
        });
    }
    let issues = check(&cells);
    Report { cells, comparisons, issues }
}

/// 실측이 없는 칸의 물리 점검: 유한값, 온도·수명 범위
pub fn check(cells: &[Prediction]) -> Vec<String> {
    let mut v = Vec::new();
    for c in cells.iter() {
        if !(c.life_min > 0.0) {
            v.push(format!("{} × {}: 수명 {:.3} min", c.tool, c.cut, c.life_min));
        }
        if !(c.interface_c > 0.0 && c.interface_c < crate::pairmatrix::INTERFACE_ABSURD_C) {
            v.push(format!("{} × {}: 계면 {:.0} °C", c.tool, c.cut, c.interface_c));
        }
        if !(c.f_res_mean_n.is_finite() && c.f_res_mean_n >= 0.0) {
            v.push(format!("{} × {}: 합력 {:.1} N", c.tool, c.cut, c.f_res_mean_n));
        }
    }
    v
}

/// 양별 요약: (양, 개수, 대역 안, |log10| 중앙값, 기하 평균 예측/실측)
pub fn summary(rep: &Report) -> Vec<(String, usize, usize, f64, f64)> {
    let mut keys: Vec<String> = Vec::new();
    for c in rep.comparisons.iter() {
        let k = format!("{}:{}", c.kind, c.quantity);
        if !keys.contains(&k) {
            keys.push(k);
        }
    }
    keys.iter()
        .map(|k| {
            let sel: Vec<&Comparison> = rep.comparisons.iter().filter(|c| &format!("{}:{}", c.kind, c.quantity) == k).collect();
            let mut errs: Vec<f64> = sel.iter().map(|c| c.error).collect();
            errs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let med = if errs.is_empty() { 0.0 } else { errs[errs.len() / 2] };
            let pos: Vec<f64> = sel.iter().filter(|c| c.kind != "민감도" && c.ratio > 0.0).map(|c| c.ratio.ln()).collect();
            let bias = if pos.is_empty() { f64::NAN } else { (pos.iter().sum::<f64>() / pos.len() as f64).exp() };
            (k.clone(), sel.len(), sel.iter().filter(|c| c.in_band).count(), med, bias)
        })
        .collect()
}

/// 사람이 읽는 줄: 비교마다 `[BENCH]`, 양별 `[BENCH-SUM]`, 수명 격자 `[BENCH-GRID]`
pub fn lines(rep: &Report) -> Vec<String> {
    let mut v = Vec::new();
    for c in rep.comparisons.iter() {
        v.push(format!(
            "[BENCH] {} {} | {} | {}: 실측 {} (대역 {}~{}) · 샌드박스 {} → {} {} | 확신도 {} | {}",
            if c.in_band { "○" } else { "×" },
            c.id,
            c.cells,
            c.quantity,
            num(c.measured),
            num(c.lo),
            num(c.hi),
            num(c.predicted),
            if c.kind == "민감도" { format!("차이 {:+.2}", c.ratio) } else { format!("비 {:.2}", c.ratio) },
            c.kind,
            c.confidence,
            c.basis
        ));
    }
    for (k, n, inb, med, bias) in summary(rep) {
        v.push(format!(
            "[BENCH-SUM] {} · {}건 · 대역 안 {} · 오차 중앙 {:.2}{}",
            k,
            n,
            inb,
            med,
            if bias.is_finite() { format!(" · 기하 평균 예측/실측 {:.2}", bias) } else { String::new() }
        ));
    }
    let mut cuts: Vec<String> = Vec::new();
    let mut tools: Vec<String> = Vec::new();
    for c in rep.cells.iter() {
        if !cuts.contains(&c.cut) {
            cuts.push(c.cut.clone());
        }
        if !tools.contains(&c.tool) {
            tools.push(c.tool.clone());
        }
    }
    v.push(format!("[BENCH-GRID] 수명 min (판정 VB) · 공구 \\ 조건 | {}", cuts.join(" | ")));
    for t in tools.iter() {
        let row: Vec<String> = cuts
            .iter()
            .map(|c| {
                rep.cells
                    .iter()
                    .find(|p| &p.tool == t && &p.cut == c)
                    .map(|p| format!("{}{}", num(p.life_min), if p.compat_severity >= 3 || p.fit_verdict >= 2 { "×" } else if p.chatter_margin < 1.0 { "~" } else { "" }))
                    .unwrap_or_else(|| "-".into())
            })
            .collect();
        v.push(format!("[BENCH-GRID] {} | {}", t, row.join(" | ")));
    }
    v.push("[BENCH-GRID] × = 화학 궁합 심각 또는 용도 적합도 부적합 · ~ = 추정 FRF 로 채터 여유 < 1".into());
    for i in rep.issues.iter() {
        v.push(format!("[BENCH-CHECK] {}", i));
    }
    v
}

fn num(v: f64) -> String {
    let a = v.abs();
    if a >= 100.0 {
        format!("{:.0}", v)
    } else if a >= 10.0 {
        format!("{:.1}", v)
    } else if a >= 1.0 {
        format!("{:.2}", v)
    } else {
        format!("{:.3}", v)
    }
}

pub fn csv(rep: &Report) -> String {
    let mut s = String::from("kind,id,cells,quantity,measured,lo,hi,predicted,ratio,error,in_band,confidence,basis\n");
    let q = |x: &str| format!("\"{}\"", x.replace('"', "'"));
    for c in rep.comparisons.iter() {
        s.push_str(&format!(
            "{},{},{},{},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{},{},{}\n",
            c.kind,
            c.id,
            q(&c.cells),
            c.quantity,
            c.measured,
            c.lo,
            c.hi,
            c.predicted,
            c.ratio,
            c.error,
            c.in_band,
            c.confidence,
            q(&c.basis)
        ));
    }
    s
}

pub fn cells_csv(rep: &Report) -> String {
    let mut s = String::from("tool,cut,material,coating,d_mm,z,vc,fz,ap,ae,rpm,vb_crit_mm,life_min,life_no_chatter_min,vb_rate_um_min,interface_c,interface_dry_c,f_mean_n,f_peak_n,torque_nm,power_kw,kr,ra_um,chatter_margin,chatter_factor,dominant,compat,fit\n");
    for c in rep.cells.iter() {
        s.push_str(&format!(
            "{},{},\"{}\",{},{:.1},{},{:.1},{:.4},{:.2},{:.2},{:.0},{:.2},{:.3},{:.3},{:.3},{:.0},{:.0},{:.1},{:.1},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{},{},{}\n",
            c.tool,
            c.cut,
            c.material,
            c.coating,
            c.diameter_mm,
            c.flutes,
            c.vc_m_min,
            c.fz_mm,
            c.ap_mm,
            c.ae_mm,
            c.rpm,
            c.vb_criterion_mm,
            c.life_min,
            c.life_no_chatter_min,
            c.vb_rate_um_min,
            c.interface_c,
            c.interface_dry_c,
            c.f_res_mean_n,
            c.f_res_peak_n,
            c.torque_nm,
            c.power_kw,
            c.radial_ratio,
            c.ra_um,
            c.chatter_margin,
            c.chatter_factor,
            c.dominant,
            c.compat_severity,
            c.fit_verdict
        ));
    }
    s
}

// ---------------------------------------------------------------- 선형 절삭 계수로 본 평균 힘 (계수 식별 논문의 '실측' 쪽)

/// 선형 기계론 모델 dFt = (Ktc·h + Kte)·db, dFr = (Krc·h + Kre)·db 의 평균 힘 벡터 크기 (N). 헬릭스는 평균 힘을 바꾸지 않으므로 0 으로 적분한다.
/// 계수(N/mm², N/mm)는 논문이 평균 힘 회귀로 식별한 값이라, 같은 조건의 평균 힘 측정을 그대로 되돌린 것과 같다
pub fn linear_force_mean(z: u32, d_mm: f64, ap_mm: f64, ae_mm: f64, fz_mm: f64, down: bool, k: (f64, f64, f64, f64)) -> f64 {
    let (ktc, krc, kte, kre) = k;
    let eng = physics::Engagement::side(ap_mm, ae_mm.min(d_mm), d_mm, down);
    let n = 3600usize;
    let pitch = 2.0 * PI / z.max(1) as f64;
    let (mut fx, mut fy) = (0.0, 0.0);
    for s in 0..n {
        let theta = 2.0 * PI * s as f64 / n as f64;
        for j in 0..z.max(1) {
            let phi = (theta + j as f64 * pitch).rem_euclid(2.0 * PI);
            if phi < eng.phi_st || phi > eng.phi_ex {
                continue;
            }
            let (sp, cp) = phi.sin_cos();
            let h = fz_mm * sp;
            if h <= 0.0 {
                continue;
            }
            let ft = (ktc * h + kte) * ap_mm;
            let fr = (krc * h + kre) * ap_mm;
            fx += -ft * cp - fr * sp;
            fy += ft * sp - fr * cp;
        }
    }
    ((fx / n as f64).powi(2) + (fy / n as f64).powi(2)).sqrt()
}

// ---------------------------------------------------------------- 논문 공구·소재·조건 (리서치 정리 · 2026-10-19 웹 검색 대조)

#[allow(clippy::too_many_arguments)]
fn setting(name: &str, model: &str, d: f64, z: u8, loc: f64, helix: f64, coat: Option<&str>, substrate: ToolSubstrate, rake: Option<f64>, clearance: Option<f64>, edge_um: Option<f64>, nose: ToolNose, stickout: f64) -> EndMillMockupSetting {
    EndMillMockupSetting {
        name: name.into(),
        model: model.into(),
        diameter_mm: d,
        flute_count: z,
        loc_mm: loc,
        oal_mm: stickout + 2.0 * d + 20.0,
        shank_diameter_mm: d,
        helix_angle_deg: helix,
        coating_name: coat.map(|c| c.to_string()),
        is_high_end: true,
        nose,
        stickout_mm: Some(stickout),
        runout_um: None,
        variable_pitch: false,
        substrate,
        rake_deg: rake,
        clearance_deg: clearance,
        edge_radius_um: edge_um,
        neck_diameter_mm: None,
        reach_mm: None,
        holder: HolderType::Unknown,
        radius_measured_rotating: false,
    }
}

/// 논문이 밝히지 않은 경사·여유·날끝 반경은 그 논문 소재의 기본값으로 고정한다 — 다른 소재에 댈 때 그 소재용 공구로 바뀌지 않게 (`pairmatrix::freeze_geometry`)
fn frozen(mut s: EndMillMockupSetting, design: &WorkpieceMaterial) -> EndMillMockupSetting {
    crate::pairmatrix::freeze_geometry(&mut s, design, &crate::profile::MachineLimits::default());
    s
}

fn rt(key: &'static str, label: &'static str, source: &'static str, s: EndMillMockupSetting, design: &WorkpieceMaterial) -> RefTool {
    RefTool { key, label, source, setting: frozen(s, design) }
}

const AL: WorkpieceMaterial = WorkpieceMaterial::Aluminum;
const C45: WorkpieceMaterial = WorkpieceMaterial::CarbonSteel;
const TI: WorkpieceMaterial = WorkpieceMaterial::Titanium;
const CR41: WorkpieceMaterial = WorkpieceMaterial::AlloySteel { hardness_hrc: 20 };
const H13: WorkpieceMaterial = WorkpieceMaterial::AlloySteel { hardness_hrc: 52 };

/// 논문 공구 (행). 괄호 안 '가정' 은 논문이 밝히지 않아 둔 값
pub fn paper_tools() -> Vec<RefTool> {
    use ToolNose::{Ball, Square};
    use ToolSubstrate::{Carbide, CarbideUltraFine, Hss};
    vec![
        rt(
            "kar_1018_insert",
            "Ø19.05 1날 무코팅 초경 인서트 (경사 0° · 헬릭스 0° · 여유 15°)",
            "Karandikar·Abbas·Schmitz 2013/2014, Karandikar·Schmitz·Smith 2021 — Kennametal KICR073SD30333C + 9.53 mm 사각 무코팅 인서트. 날끝 반경·돌출 60 mm 가정",
            setting("Karandikar 1018 인서트", "PAPER-KAR1018", 19.05, 1, 9.5, 0.0, None, Carbide, Some(0.0), Some(15.0), None, Square, 60.0),
            &C45,
        ),
        rt(
            "tatar_r390",
            "Ø12 PVD 초미립 인서트 커터 (Sandvik R390-012A12-07L, 날끝 날카로움)",
            "Tatar·Sjöberg·Andersson 2020 — 날 수 1·경사 +12°·여유 10°·축 경사 10°·날끝 8 µm 가정 (일정 h_max 시험이라 날 수는 수명에 거의 무관)",
            setting("Tatar R390", "PAPER-TATAR", 12.0, 1, 6.8, 10.0, Some("TiAlN"), CarbideUltraFine, Some(12.0), Some(10.0), Some(8.0), Square, 40.0),
            &TI,
        ),
        rt(
            "liu_ti_em",
            "Ø10 4날 TiAlN 초경 엔드밀 (가정)",
            "Liu·Zhang·Luo·Zhang 2019 — 공구를 찾지 못해 같은 그룹 류의 Ø10 4날 TiAlN 35° 로 가정 (체적 비는 지름에 거의 무관)",
            setting("Liu Ti 엔드밀", "PAPER-LIU", 10.0, 4, 22.0, 35.0, Some("TiAlN"), Carbide, None, None, None, Square, 35.0),
            &TI,
        ),
        rt(
            "matuszak_ti",
            "Ø16 2날 TiAlN 인서트 커터 (가정)",
            "Matuszak·Zaleski·Zyśko 2023 — 공구 미확인, ae 7.5 · fz 0.2 · ap 0.5 에 맞춘 가정 공구",
            setting("Matuszak Ti 커터", "PAPER-MAT", 16.0, 2, 8.0, 10.0, Some("TiAlN"), Carbide, None, None, None, Square, 45.0),
            &TI,
        ),
        rt(
            "grig_al_bare",
            "Ø6 3날 무코팅 초경 (WC-10Co)",
            "Grigoriev·Volosova·Fedorov 외 2020 — 6WH10F 모재. 헬릭스 40°·형상은 알루미늄 기본값 가정",
            setting("Grigoriev Al 무코팅", "PAPER-GRIG-AL", 6.0, 3, 13.0, 40.0, None, Carbide, None, None, None, Square, 25.0),
            &AL,
        ),
        rt(
            "grig_al_dlc",
            "Ø6 3날 DLC (PECVD) 초경",
            "Grigoriev 외 2020 — 같은 공구에 PECVD DLC",
            setting("Grigoriev Al DLC", "PAPER-GRIG-AL", 6.0, 3, 13.0, 40.0, Some("DLC"), Carbide, None, None, None, Square, 25.0),
            &AL,
        ),
        rt(
            "grig_st_bare",
            "Ø6 5날 무코팅 초경 (WC-10Co)",
            "Grigoriev 외 2020 — 41Cr4 용 5날. 헬릭스 40°·형상은 합금강 기본값 가정",
            setting("Grigoriev 41Cr4 무코팅", "PAPER-GRIG-ST", 6.0, 5, 13.0, 40.0, None, Carbide, None, None, None, Square, 25.0),
            &CR41,
        ),
        rt(
            "grig_st_dlc",
            "Ø6 5날 DLC (PECVD) 초경",
            "Grigoriev 외 2020 — 같은 공구에 PECVD DLC",
            setting("Grigoriev 41Cr4 DLC", "PAPER-GRIG-ST", 6.0, 5, 13.0, 40.0, Some("DLC"), Carbide, None, None, None, Square, 25.0),
            &CR41,
        ),
        rt(
            "hov_hss_bare",
            "Ø25 3날 무코팅 HSS (가정)",
            "Hovsepian 외 2006 — HSS 공구 · 24,000 rpm · ap 4. 지름(Vc 1,884 에서 역산)·날 수·이송·ae 는 가정 (확신도 낮음)",
            setting("Hovsepian HSS 무코팅", "PAPER-HOV", 25.0, 3, 40.0, 30.0, None, Hss, None, None, None, Square, 60.0),
            &AL,
        ),
        rt(
            "hov_hss_dlc",
            "Ø25 3날 DLC HSS (가정)",
            "Hovsepian 외 2006 — DLC 코팅 HSS",
            setting("Hovsepian HSS DLC", "PAPER-HOV", 25.0, 3, 40.0, 30.0, Some("DLC"), Hss, None, None, None, Square, 60.0),
            &AL,
        ),
        rt(
            "hov_hss_tialn",
            "Ø25 3날 TiAlCrYN HSS (TiAlN 계열로 둠, 가정)",
            "Hovsepian 외 2006 — TiAlCrYN (샌드박스 코팅 표에 없어 TiAlN 계열)",
            setting("Hovsepian HSS TiAlCrYN", "PAPER-HOV", 25.0, 3, 40.0, 30.0, Some("TiAlN"), Hss, None, None, None, Square, 60.0),
            &AL,
        ),
        rt(
            "dewes_ball",
            "Ø8 2날 TiAlN 초경 볼 (가정)",
            "Dewes·Ng·Chua·Newton·Aspinwall 1999 — 초경 볼 엔드밀 · H13 52 HRC. 지름·날 수·코팅 가정",
            setting("Dewes 볼", "PAPER-DEWES", 8.0, 2, 8.0, 30.0, Some("TiAlN"), CarbideUltraFine, None, None, None, Ball, 30.0),
            &H13,
        ),
        rt(
            "li_ti_slot",
            "Ø12 4날 초경 (Li 2014 계수 식별 공구, 형상 가정)",
            "Li·Zhao·Li·He 2014 — 슬롯 시험으로 식별한 Ti6Al4V 계수. 공구 형상·코팅은 Ti 기본값 가정",
            setting("Li Ti 슬롯", "PAPER-LI", 12.0, 4, 26.0, 30.0, None, Carbide, None, None, None, Square, 40.0),
            &TI,
        ),
    ]
}

/// 샌드박스 내장 공구 8개 (행) — 형상을 넣지 않은 공구는 설계 소재 기본값으로 고정 (`pairmatrix::cell_profile` 과 같은 규칙)
pub fn sandbox_tools() -> Vec<RefTool> {
    EndMillMockupSetting::builtin_tools()
        .into_iter()
        .map(|mut s| {
            if let Some(design) = crate::pairmatrix::builtin_design_material(&s) {
                crate::pairmatrix::freeze_geometry(&mut s, &design, &crate::profile::MachineLimits::default());
            }
            let key: &'static str = Box::leak(format!("sb_{}", s.model.to_lowercase().replace([' ', '-', '.'], "_")).into_boxed_str());
            let label: &'static str = Box::leak(s.name.clone().into_boxed_str());
            RefTool { key, label, source: "샌드박스 내장 공구", setting: s }
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn rc(key: &'static str, tool: &'static str, label: &'static str, source: &'static str, material: WorkpieceMaterial, vc: f64, fz: f64, ap_over_d: f64, ae_over_d: f64, hex: Option<f64>, coolant: CoolantMethod, vb: f64) -> RefCut {
    RefCut { key, label, source, tool, material, vc_m_min: vc, fz_mm: fz, ap_over_d, ae_over_d, hex_mm: hex, down: true, coolant, vb_criterion_mm: vb }
}

/// 논문 소재·조건 (열). ap/D·ae/D 는 논문 공구 지름 기준
pub fn paper_cuts() -> Vec<RefCut> {
    use CoolantMethod::{Dry, Flood};
    let d19 = 19.05;
    vec![
        rc("kar1018_090", "kar_1018_insert", "AISI 1018 · Vc 89.8 · fz 0.06 · ap 3 · ae 4.7 (25 %) · 하향 · 건식(가정) · FWW 0.3", "Karandikar 외 2013/2014 — 1,500 rpm", C45, 89.8, 0.06, 3.0 / d19, 4.7 / d19, None, Dry, 0.3),
        rc("kar1018_224", "kar_1018_insert", "AISI 1018 · Vc 224.4 (3,750 rpm)", "Karandikar 외 2013/2014", C45, 224.4, 0.06, 3.0 / d19, 4.7 / d19, None, Dry, 0.3),
        rc("kar1018_374", "kar_1018_insert", "AISI 1018 · Vc 374.0 (6,250 rpm)", "Karandikar 외 2013/2014", C45, 374.0, 0.06, 3.0 / d19, 4.7 / d19, None, Dry, 0.3),
        rc("kar2021_150", "kar_1018_insert", "AISI 1018 · Vc 149.6 · fz 0.06 · ap 2 · ae 25 % (2,500 rpm)", "Karandikar·Schmitz·Smith 2021 (ap·fz·ae 는 같은 그룹 자료)", C45, 149.6, 0.06, 2.0 / d19, 0.25, None, Dry, 0.3),
        rc("kar2021_299", "kar_1018_insert", "AISI 1018 · Vc 299.2 (5,000 rpm)", "Karandikar·Schmitz·Smith 2021", C45, 299.2, 0.06, 2.0 / d19, 0.25, None, Dry, 0.3),
        rc("tatar_ti", "tatar_r390", "Ti-6Al-4V · Vc 110 · h_max 0.05 · ap 1.5 · ae 25 % · 습식 · VB 0.2", "Tatar 외 2020 — 완전 요인 시험의 가운데 칸 (Vc 100~120, ae 10~70 %)", TI, 110.0, 0.05, 1.5 / 12.0, 0.25, Some(0.05), Flood, 0.2),
        rc("liu_ti_t1", "liu_ti_em", "Ti-6Al-4V 건식 · Vc 60 · fz 0.1 · ap 2 · ae 0.2 mm (시험 1)", "Liu 외 2019", TI, 60.0, 0.1, 0.2, 0.02, None, Dry, 0.2),
        rc("liu_ti_t5", "liu_ti_em", "Ti-6Al-4V 건식 · Vc 130 · ae 0.4 mm (시험 5)", "Liu 외 2019", TI, 130.0, 0.1, 0.2, 0.04, None, Dry, 0.2),
        rc("liu_ti_t9", "liu_ti_em", "Ti-6Al-4V 건식 · Vc 200 · ae 0.6 mm (시험 9)", "Liu 외 2019", TI, 200.0, 0.1, 0.2, 0.06, None, Dry, 0.2),
        rc("mat_ti_70", "matuszak_ti", "Ti-6Al-4V · Vc 70 · fz 0.2 · ap 0.5 · ae 7.5 · 냉각(가정) · VB 0.3(가정)", "Matuszak 외 2023", TI, 70.0, 0.2, 0.5 / 16.0, 7.5 / 16.0, None, Flood, 0.3),
        rc("grig_al", "grig_al_bare", "AlCuMg2 · Vc 188.4 · fz 0.068 · ap 2 · ae 0.5 · 건식(가정) · VB 0.4", "Grigoriev 외 2020 — F 2,000 mm/min", AL, 188.4, 0.0667, 2.0 / 6.0, 0.5 / 6.0, None, Dry, 0.4),
        rc("grig_41cr4", "grig_st_bare", "41Cr4 · Vc 188.4 · fz 0.015 · ap 2 · ae 0.2 · 건식(가정) · VB 0.4", "Grigoriev 외 2020 — F 750 mm/min", CR41, 188.4, 0.015, 2.0 / 6.0, 0.2 / 6.0, None, Dry, 0.4),
        rc("hov_al7010", "hov_hss_bare", "Al7010 · Vc 1,884 · fz 0.1 · ap 4 · ae 25 % (fz·ae 가정) · 건식 · VB 0.23", "Hovsepian 외 2006", AL, 1884.0, 0.1, 4.0 / 25.0, 0.25, None, Dry, 0.23),
        rc("dewes_h13", "dewes_ball", "H13 52 HRC · Vc 200 · fz 0.1 · ap 0.5 · ae 0.5 (fz·ap·ae 가정) · 건식", "Dewes 외 1999 — Vc 200 m/min 까지 계면 198~385 °C", H13, 200.0, 0.1, 0.5 / 8.0, 0.5 / 8.0, None, Dry, 0.15),
        rc("li_ti_slot05", "li_ti_slot", "Ti-6Al-4V 슬롯 · Vc 50 (가정) · fz 0.05 · ap 1", "Li 외 2014 계수 식별 조건(슬롯)", TI, 50.0, 0.05, 1.0 / 12.0, 1.0, None, Dry, 0.2),
        rc("li_ti_slot10", "li_ti_slot", "Ti-6Al-4V 슬롯 · Vc 50 (가정) · fz 0.10 · ap 1", "Li 외 2014", TI, 50.0, 0.10, 1.0 / 12.0, 1.0, None, Dry, 0.2),
    ]
}

fn ab(id: &'static str, tool: &'static str, cut: &'static str, quantity: Quantity, value: f64, lo: f64, hi: f64, confidence: Confidence, basis: &'static str) -> Absolute {
    Absolute { id, tool, cut, quantity, value, lo, hi, confidence, basis }
}

/// 절댓값 실측. 수명 대역은 ×/÷2 (같은 조건 공구 간 편차 σ_ln 0.12~0.19 의 ±2σ 에 소재 대응·미공개 형상 몫을 더한 폭),
/// 공구를 가정한 칸은 ×/÷3
pub fn absolutes() -> Vec<Absolute> {
    use Confidence::{Low, Medium};
    use Quantity::*;
    let li = (2000.0, 1000.0, 41.0, 77.0);
    let f05 = linear_force_mean(4, 12.0, 1.0, 12.0, 0.05, true, li);
    let f10 = linear_force_mean(4, 12.0, 1.0, 12.0, 0.10, true, li);
    vec![
        ab("kar1018_090", "kar_1018_insert", "kar1018_090", LifeMin, 255.3, 127.7, 510.6, Medium, "FWW 0.3 mm 까지 255.3 분 (1,500 rpm) — Karandikar 외 2014 Precis. Eng. part 1 (2013 MST 와 같은 시험 설정)"),
        ab("kar1018_224", "kar_1018_insert", "kar1018_224", LifeMin, 35.5, 17.8, 71.0, Medium, "35.5 분 (3,750 rpm)"),
        ab("kar1018_374", "kar_1018_insert", "kar1018_374", LifeMin, 8.5, 4.25, 17.0, Medium, "8.5 분 (6,250 rpm)"),
        ab("kar2021_150", "kar_1018_insert", "kar2021_150", LifeMin, 63.5, 31.8, 127.0, Medium, "반복 3회 50.1 · 68.5 · 72.0 분 (평균 63.5) — Karandikar·Schmitz·Smith 2021"),
        ab("kar2021_299", "kar_1018_insert", "kar2021_299", LifeMin, 9.8, 4.9, 19.7, Medium, "반복 3회 8.5 · 11.5 · 9.5 분 (평균 9.8)"),
        ab("grig_al_bare", "grig_al_bare", "grig_al", LifeMin, 99.0, 49.5, 198.0, Low, "VB 0.4 mm 까지 99 분 (무코팅) — 냉각과 B 2 mm·t 0.5 mm 의 축·반경 구분이 본문 미확인이라 낮음"),
        ab("grig_al_dlc", "grig_al_dlc", "grig_al", LifeMin, 342.0, 171.0, 684.0, Low, "VB 0.4 mm 까지 342 분 (DLC, 3.45배)"),
        ab("grig_st_bare", "grig_st_bare", "grig_41cr4", LifeMin, 32.0, 16.0, 64.0, Low, "VB 0.4 mm 까지 32 분 (무코팅)"),
        ab("grig_st_dlc", "grig_st_dlc", "grig_41cr4", LifeMin, 36.0, 18.0, 72.0, Low, "DLC 는 무코팅보다 4 분 이하 김 (≈36 분)"),
        ab("liu_t1_vmr", "liu_ti_em", "liu_ti_t1", LifeVolumeCm3, 77.28, 25.8, 231.8, Low, "수명 끝 제거 체적 77,280 mm³ (시험 1) — 공구 가정이라 ×/÷3"),
        ab("liu_t5_vmr", "liu_ti_em", "liu_ti_t5", LifeVolumeCm3, 2.76, 0.92, 8.28, Low, "2,760 mm³ (시험 5)"),
        ab("liu_t9_vmr", "liu_ti_em", "liu_ti_t9", LifeVolumeCm3, 1.656, 0.552, 4.97, Low, "1,656 mm³ (시험 9)"),
        ab("mat_ti_70", "matuszak_ti", "mat_ti_70", LifeMin, 70.0, 23.3, 210.0, Low, "Vc 70 에서 가장 긴 수명 70 분 (판정 VB3 값 미확인, 공구 가정)"),
        ab("dewes_h13_t", "dewes_ball", "dewes_h13", InterfaceC, 300.0, 198.0, 600.0, Low, "공작물에 심은 열전대 198~301 °C (0°) · 247~385 °C (60°) — 샌드박스 값은 공구–칩 계면이라 이보다 높아야 해 상한은 리서치 정리의 측정법 보정 대역 600 °C"),
        ab("li_slot05_f", "li_ti_slot", "li_ti_slot05", ForceVectorMeanN, f05, 0.75 * f05, 1.25 * f05, Medium, "Ktc 2000 · Krc 1000 N/mm² · Kte 41 · Kre 77 N/mm 로 되돌린 평균 힘 (fz 0.05) — 계수 모델 정확도 ±25 % (Budak 1996)"),
        ab("li_slot10_f", "li_ti_slot", "li_ti_slot10", ForceVectorMeanN, f10, 0.75 * f10, 1.25 * f10, Medium, "같은 계수, fz 0.10"),
        ab("ti_kr", "li_ti_slot", "li_ti_slot10", RadialRatio, 0.5, 0.29, 0.55, Medium, "Krc/Ktc 0.5 (Li 2014) — 다른 실측 0.37 (Tian 2026 볼) · 0.29 (Budak 1996 직교 변환) 까지 대역"),
        ab("grig_al_ra", "grig_al_dlc", "grig_al", RaUm, 1.3, 0.8, 2.0, Low, "DLC 공구 150 분 동안 Ra 평균 1.3 µm (측정 위치 미확인)"),
    ]
}

fn ra(id: &'static str, num: (&'static str, &'static str), den: (&'static str, &'static str), quantity: Quantity, value: f64, lo: f64, hi: f64, confidence: Confidence, basis: &'static str) -> Ratio {
    Ratio { id, num, den, quantity, value, lo, hi, confidence, basis }
}

/// 같은 논문 안의 비
pub fn ratios() -> Vec<Ratio> {
    use Confidence::{Low, Medium};
    use Quantity::*;
    vec![
        ra("grig_al_dlc_gain", ("grig_al_dlc", "grig_al"), ("grig_al_bare", "grig_al"), LifeMin, 3.45, 2.5, 4.5, Medium, "AlCuMg2: DLC / 무코팅 = 342 / 99 분"),
        ra("grig_st_dlc_gain", ("grig_st_dlc", "grig_41cr4"), ("grig_st_bare", "grig_41cr4"), LifeMin, 1.12, 0.9, 1.3, Medium, "41Cr4: DLC 는 무코팅보다 4 분 이하 김 (철계에서 탄소 코팅 이득 거의 없음)"),
        ra("hov_dlc_gain", ("hov_hss_dlc", "hov_al7010"), ("hov_hss_bare", "hov_al7010"), LifeMin, 3.5, 2.5, 5.0, Low, "Al7010: DLC ≈ TiAlN/VN, 무코팅의 3.5배 (조건 일부 가정)"),
        ra("hov_tialn_gain", ("hov_hss_tialn", "hov_al7010"), ("hov_hss_bare", "hov_al7010"), LifeMin, 1.52, 1.1, 2.1, Low, "Al7010: TiAlCrYN = 무코팅 × 3.5 / 2.3"),
        ra("liu_vmr_t1_t5", ("liu_ti_em", "liu_ti_t1"), ("liu_ti_em", "liu_ti_t5"), LifeVolumeCm3, 28.0, 14.0, 56.0, Medium, "제거 체적 77,280 / 2,760 mm³ — Vc 60→130 · ae 0.2→0.4 (같은 공구라 지름 가정이 거의 약분)"),
        ra("liu_vmr_t5_t9", ("liu_ti_em", "liu_ti_t5"), ("liu_ti_em", "liu_ti_t9"), LifeVolumeCm3, 1.67, 0.9, 3.0, Medium, "2,760 / 1,656 mm³ — Vc 130→200 · ae 0.4→0.6"),
    ]
}

#[allow(clippy::too_many_arguments)]
fn se(id: &'static str, tool: &'static str, cut: &'static str, quantity: Quantity, param: Param, from: f64, to: f64, value: f64, lo: f64, hi: f64, confidence: Confidence, basis: &'static str) -> Sensitivity {
    Sensitivity { id, tool, cut, quantity, param, from, to, value, lo, hi, confidence, basis }
}

/// 민감도 — 수명의 속도·반경 절입 기울기 (Taylor 지수 n 이면 d ln T / d ln Vc = −1/n)
pub fn sensitivities() -> Vec<Sensitivity> {
    use Confidence::{Low, Medium};
    use Quantity::*;
    vec![
        se("kar_taylor_lo", "kar_1018_insert", "kar1018_224", LifeMin, Param::Vc, 89.8 / 224.4, 1.0, -2.15, -2.8, -1.5, Medium, "1018 무코팅 초경 Vc 89.8→224.4: 255.3→35.5 분"),
        se("kar_taylor_hi", "kar_1018_insert", "kar1018_224", LifeMin, Param::Vc, 1.0, 374.0 / 224.4, -2.80, -3.5, -2.1, Medium, "Vc 224.4→374.0: 35.5→8.5 분"),
        se("kar21_taylor", "kar_1018_insert", "kar2021_150", LifeMin, Param::Vc, 1.0, 2.0, -2.69, -3.3, -2.1, Medium, "Vc 149.6→299.2: 63.5→9.8 분 (로지스틱 맞춤 n 0.378 → −2.65)"),
        se("tatar_vc", "tatar_r390", "tatar_ti", LifeMin, Param::Vc, 100.0 / 110.0, 120.0 / 110.0, -4.0, -5.5, -2.5, Medium, "ln T = b0 + b1·ln ae + b2·ln Vc, Vc 1 % 오르면 수명 4 % 감소 (24 응답 회귀)"),
        se("tatar_ae", "tatar_r390", "tatar_ti", LifeMin, Param::Ae, 0.4, 2.8, -1.0, -1.5, -0.5, Medium, "h_max 0.05 일정, ae 10→70 % D: 1 % 늘면 수명 1 % 감소"),
        se("mat_vc", "matuszak_ti", "mat_ti_70", LifeMin, Param::Vc, 1.0, 250.0 / 70.0, -3.3, -4.5, -2.0, Low, "Vc 70 에서 70 분, 250 에서 VB3 0.5 mm 까지 약 1 분 (판정 값이 달라 확신도 낮음)"),
        se("dewes_t_vc", "dewes_ball", "dewes_h13", InterfaceRiseC, Param::Vc, 0.5, 1.0, 0.4, 0.1, 0.8, Medium, "계면 온도는 Vc 와 함께 오르고 고속에서 줄지 않음 (Dewes 1999), 플랭크 온도는 속도와 함께 빠르게 오름 (Hosokawa 2000)"),
    ]
}

/// 논문 공구 + 샌드박스 내장 공구 × 논문 조건 N:N 과 실측 대조
pub fn run() -> Report {
    let mut tools = paper_tools();
    tools.extend(sandbox_tools());
    run_with(&tools, &paper_cuts(), &absolutes(), &ratios(), &sensitivities())
}
