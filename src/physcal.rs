//! 물리(MPM) 교차검증 보정.
//!
//! 앱의 해석식(Kienzle·Boothroyd 계열)을 **다른 가정으로 푼 두 번째 계산**과 맞대어 본다. 두 번째 계산은
//! 연속체 절삭 해석기 `endmill_mpm`(2D 평면변형 직교절삭, MLS-MPM)이다. 같은 조건에서 두 계산의 절삭력과
//! 접촉면 온도를 비교하고, 그 비율을 기존 보정 통로(`Calibration.power` / `.temperature`)에 연결한다.
//! 통로 뒤쪽(1차 시뮬레이션 → 시계열 예측 → 규칙·LAYA 판정 → 2차 보정 G-code)은 그대로 쓴다.
//!
//! 이 모듈이 **하지 않는 것**
//! - 실측으로 검증됐다고 주장하지 않는다. 모든 기록은 `validated = false` 이고, 기본은 꺼짐(`apply = false`)이다.
//! - 공구 학습 원장(SDS)이나 재계획 기준점에 가상 측정값을 넣지 않는다. 보정은 계산 직전의 컨텍스트에만
//!   곱해지고, 원장에 실측 보정이 이미 있는 축에는 적용하지 않는다(실측이 우선).
//! - 3D 밀링 형상, 공구 열전도, 냉각유, 마모, 톱니형 칩은 다루지 않는다. 비교 조건은 '건식·새 공구·직교절삭'이다.
//!
//! 보정 계수의 정의
//! - `ratio_power`: 앱의 해석식에 이 값을 `calib.power` 로 곱하면 같은 조건의 절삭방향 힘이 MPM 과 같아진다.
//!   (식이 선형이 아니므로 고정점 반복으로 한 번 더 맞춘다)
//! - `ratio_temperature`: `power` 를 먼저 적용한 뒤에도 남는 접촉면 온도 상승의 차이. 동력을 낮추면 발열도
//!   함께 줄기 때문에, 온도의 잔차만 따로 계수로 둔다.
//! - 실제 적용값은 `1 + 가중치 × (비 − 1)` 이고 0.5 ~ 1.5 로 제한한다.

use crate::physics::{self, Calibration, CutContext, Engagement, ForceResult, JohnsonCook, WorkpieceProps};
use crate::profile::MachiningProfile;
use endmill_mpm::{CutParams, CutResult, Material, Preset, RunPlan};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// 2: 칩 속도 기준 두께비·힘 추세·천장 접촉을 기록하고 해석 길이를 늘린 뒤의 형식. 1 은 짧은 해석이라
/// 절삭력이 약 20% 낮게 나왔으므로 읽을 때 버린다.
pub const RECORD_VERSION: u32 = 2;
/// MPM 에 넣을 칩 두께 범위 (µm). 이보다 얇으면 격자 비용이 급증하고 날 둥글기 해상도가 부족하다.
pub const MPM_H_MIN_UM: f64 = 20.0;
pub const MPM_H_MAX_UM: f64 = 150.0;
pub const FACTOR_MIN: f64 = 0.5;
pub const FACTOR_MAX: f64 = 1.5;
pub const MAX_RECORDS: usize = 200;

/// 연속 칩으로 볼 수 있는 칩 두께비·전단각 범위 (금속 절삭 실측 범위를 조금 넓힌 값)
pub const CHIP_RATIO_MIN: f64 = 0.15;
pub const CHIP_RATIO_MAX: f64 = 1.0;
pub const SHEAR_DEG_MIN: f64 = 8.0;
pub const SHEAR_DEG_MAX: f64 = 50.0;

/// 재료별 직교절삭 칩 두께비 실측 범위 (h = 0.06~0.2 mm, 초경, Vc 60~180 m/min).
/// SUS304: Marcicano 외 COBEM 2005 (Vc 60~67 m/min, r 0.23~0.44, 전단각 13~24°),
/// Zhan 외 2017 (Vc 60~180 m/min 선삭, r 약 0.47~0.51).
pub fn measured_chip_ratio(material_key: &str) -> Option<(f64, f64, &'static str)> {
    match material_key {
        "stainless" => Some((0.23, 0.51, "SUS304, Marcicano 2005 · Zhan 2017")),
        _ => None,
    }
}
/// 비교의 기준 온도 (°C). MPM 의 초기 온도·JC 기준 온도와 같다.
const T0_C: f64 = 20.0;

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

fn opt(x: f64) -> Option<f64> {
    x.is_finite().then_some(x)
}

fn fin(x: f64) -> f64 {
    if x.is_finite() {
        x
    } else {
        0.0
    }
}

// ------------------------------------------------------------------ 기록 형식

/// 비교한 조건. 기록을 나중에 적용할 때 현재 조건과 맞는지 가리는 데에도 쓴다.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhysConditions {
    pub material_key: String,
    pub material_label: String,
    pub tool_signature: String,
    pub profile_name: String,
    /// 가공 조건에서 추정한 평균 칩 두께 (µm) — 앱이 계수를 정할 때 쓰는 값과 같다
    pub h_job_um: f64,
    /// MPM 에 넣은 칩 두께 (µm)
    pub h_um: f64,
    pub vc_m_min: f64,
    pub rake_deg: f64,
    pub clear_deg: f64,
    pub edge_um: f64,
    pub mu: f64,
    pub preset: String,
    pub dx_um: f64,
    pub mscale: f64,
    pub threads: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhysMpm {
    pub fc_n_per_mm: f64,
    pub fc_sd: f64,
    pub ft_n_per_mm: f64,
    pub interface_t_c: Option<f64>,
    pub interface_t_p95_c: Option<f64>,
    pub interface_rise_c: Option<f64>,
    pub tmax_c: f64,
    /// 칩 두께비 r = h/h_c (칩 속도·질량 보존 기준)
    pub chip_ratio: Option<f64>,
    /// 칩 단면 두께를 직접 잰 두께비 (비교용, 예전 기록에는 없음)
    #[serde(default)]
    pub chip_ratio_geom: Option<f64>,
    pub shear_angle_deg: Option<f64>,
    pub mu_apparent: Option<f64>,
    pub tau_shear_mpa: Option<f64>,
    pub wp_over_wtool: f64,
    pub ke_ratio_max: f64,
    pub steady: bool,
    /// 꼬리 구간 뒤 절반 / 앞 절반 절삭력 - 1 (예전 기록에는 없음)
    #[serde(default)]
    pub fc_trend: Option<f64>,
    /// 칩이 계산 영역 천장에 닿아 쌓였는지 (예전 기록에는 없음)
    #[serde(default)]
    pub chip_at_ceiling: bool,
    pub steps: usize,
    pub particles: usize,
    pub wall_s: f64,
    /// [이동 µm, Fc N/mm, Ft N/mm, Tmax °C] — 최대 100점으로 줄인 이력
    pub history: Vec<[f64; 4]>,
    pub notes: Vec<String>,
}

/// 같은 조건에서 앱의 해석식이 내는 값 (보정 전: `Calibration::default()`, 새 공구, 건식 비교).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhysAnalytic {
    pub fc_n_per_mm: f64,
    pub ft_n_per_mm: f64,
    /// 냉각 전 접촉면 온도 상승 (K)
    pub rise_c: f64,
    /// 앱이 냉각까지 반영해 보여주는 접촉면 온도 (°C) — 참고용
    pub interface_cooled_c: f64,
    pub mu: f64,
    pub kc11_eff: f64,
    pub mc: f64,
    pub softening: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhysReliability {
    /// "low" | "medium" — 실험 검증이 없으므로 "high" 는 없다
    pub level: String,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhysCalRecord {
    pub id: String,
    pub created_ms: i64,
    pub version: u32,
    pub cond: PhysConditions,
    pub mpm: PhysMpm,
    pub analytic: PhysAnalytic,
    /// MPM / 해석식 (보정 전, 단순 비) — 화면 표시용
    pub raw_power: f64,
    pub raw_temperature: f64,
    /// 해석식에 곱하면 MPM 과 같아지는 계수 (가중치 적용 전)
    pub ratio_power: f64,
    pub ratio_temperature: f64,
    pub reliability: PhysReliability,
    /// 항상 false — 실측과 맞춰 본 적이 없다
    pub validated: bool,
    pub apply: bool,
    pub apply_temperature: bool,
    pub weight: f64,
}

// ------------------------------------------------------------------ 해석식 쪽 계산

/// 가공 조건에서 뽑은 대표 값. `physics::analyze_cut` 가 계수를 정할 때 쓰는 식과 같다.
#[derive(Debug, Clone, Copy)]
pub struct JobCond {
    pub vce_m_min: f64,
    pub h_job_mm: f64,
}

pub fn job_cond(ctx: &CutContext, p: &MachiningProfile) -> JobCond {
    let c = &p.conditions;
    let d = ctx.tool.diameter_mm;
    let rpm = c.spindle_rpm as f64;
    let z = ctx.tool.flutes.max(1) as f64;
    let fz = if rpm > 0.0 { c.feed_rate_mm_min / (rpm * z) } else { c.feed_per_tooth_mm };
    let ap = c.axial_doc_mm.min(ctx.tool.loc_mm);
    let eng = Engagement::side(ap, c.radial_doc_mm, d, true);
    let vce = physics::effective_vc(&ctx.tool, rpm, ap);
    let h = (fz * eng.max_sin() * 0.64).max(1e-4);
    JobCond { vce_m_min: vce, h_job_mm: h }
}

/// 직교절삭 한 점에서 앱의 해석식이 내는 값. `calib` 로 보정을 바꿔 가며 부를 수 있다.
pub fn analytic_at(ctx: &CutContext, calib: &Calibration, vc_m_min: f64, h_mm: f64) -> PhysAnalytic {
    let mut c = ctx.clone();
    c.calib = calib.clone();
    c.tool.flank_wear_mm = 0.0;
    c.tool_age_min = 0.0;
    let h = h_mm.max(1e-4);
    let co = physics::cutting_coeffs_at(&c, vc_m_min, h);
    let (fc, ft) = physics::slice_forces(&co, h, 1.0, 0.0, 0.0);
    let f = ForceResult { h_mean_mm: h, h_max_mm: h / 0.64, ..Default::default() };
    let (rise, _) = physics::interface_rise_from(&c, &co, &f, vc_m_min, T0_C);
    let cooled = physics::estimate_interface_c(&c, &co, vc_m_min, h);
    PhysAnalytic {
        fc_n_per_mm: fc,
        ft_n_per_mm: ft,
        rise_c: rise,
        interface_cooled_c: cooled,
        mu: co.mu,
        kc11_eff: co.kc11,
        mc: co.mc,
        softening: co.softening,
    }
}

// ------------------------------------------------------------------ 계획

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PhysRunOptions {
    /// "quick"(기본) | "standard"
    #[serde(default)]
    pub preset: Option<String>,
    /// 칩 두께 직접 지정 (µm). 없으면 가공 조건의 평균 칩 두께를 20~150 µm 로 맞춰 쓴다.
    #[serde(default)]
    pub h_um: Option<f64>,
    #[serde(default)]
    pub threads: Option<usize>,
}

pub struct Plan {
    pub ctx: CutContext,
    pub cond: PhysConditions,
    pub params: CutParams,
    pub run_plan: RunPlan,
    pub est_seconds: f64,
}

pub fn default_threads() -> usize {
    let n = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2);
    match n {
        0..=2 => 1,
        3..=4 => n - 1,
        _ => (n - 2).min(12),
    }
}

fn mpm_material(wp: &WorkpieceProps, jc: &JohnsonCook) -> Material {
    // 열물성은 칩 영역의 대표 온도에서 읽는다 (녹는점의 35%, 150~600 °C)
    let t_ref = (0.35 * wp.melt_c).clamp(150.0, 600.0);
    Material {
        name: wp.label.clone(),
        rho: wp.density,
        e_pa: wp.elastic_gpa * 1e9,
        nu: if wp.poisson > 0.05 { wp.poisson } else { 0.3 },
        cp: wp.cp_at(t_ref),
        k: wp.k_at(t_ref),
        jc_a: jc.a * 1e6,
        jc_b: jc.b * 1e6,
        jc_n: jc.n,
        jc_c: jc.c,
        jc_m: jc.m,
        rate_ref: jc.rate_ref,
        t_room_c: T0_C,
        t_melt_c: jc.t_melt_c,
        beta: 0.9,
    }
}

pub fn build_plan(p: &MachiningProfile, o: &PhysRunOptions) -> Result<Plan, String> {
    let ctx = CutContext::from_profile(p);
    let jc = ctx.wp.jc.ok_or_else(|| format!("{} 은(는) Johnson–Cook 구성식이 없어 연속체 해석을 지원하지 않습니다", ctx.wp.label))?;
    if ctx.wp.key == "cfrp" {
        return Err("CFRP 는 이방성 복합재라 이 해석기(등방 금속 소성)로 다룰 수 없습니다".into());
    }
    let preset = match o.preset.as_deref() {
        None => Preset::Quick,
        Some(s) => Preset::from_key(s).ok_or_else(|| format!("알 수 없는 해석 단계: {} (quick 또는 standard)", s))?,
    };
    let job = job_cond(&ctx, p);
    let h_job_um = job.h_job_mm * 1000.0;
    let h_um = o.h_um.unwrap_or(h_job_um).clamp(MPM_H_MIN_UM, MPM_H_MAX_UM);
    if !h_um.is_finite() {
        return Err("칩 두께가 올바르지 않습니다".into());
    }
    let vce = job.vce_m_min;
    if !(vce.is_finite() && vce > 3.0) {
        return Err(format!("유효 절삭속도 {:.1} m/min 로는 해석할 수 없습니다", vce));
    }
    // 앱이 이 조건에서 쓰는 마찰계수를 그대로 넣는다 — 비교에서 마찰 가정의 차이를 없애기 위해서
    let co = physics::cutting_coeffs_at(&ctx, vce, h_um / 1000.0);
    let mu = co.mu.clamp(0.05, 0.9);
    let material = mpm_material(&ctx.wp, &jc);
    let mut params = CutParams::for_conditions(
        material,
        preset,
        h_um,
        vce / 60.0,
        ctx.tool.rake_deg.clamp(-20.0, 35.0),
        ctx.tool.clearance_deg.clamp(2.0, 20.0),
        mu,
        ctx.tool.edge_radius_um.clamp(0.0, 60.0),
    );
    let threads = o.threads.unwrap_or_else(default_threads).clamp(1, 32);
    params.threads = threads;
    let run_plan = endmill_mpm::plan(&params)?;
    let est = run_plan.estimate_seconds(threads);
    let cond = PhysConditions {
        material_key: ctx.wp.key.clone(),
        material_label: ctx.wp.label.clone(),
        tool_signature: p.endmill_setting.signature(),
        profile_name: p.name.clone(),
        h_job_um,
        h_um,
        vc_m_min: vce,
        rake_deg: params.rake_deg,
        clear_deg: params.clear_deg,
        edge_um: params.edge_um,
        mu,
        preset: preset.key().to_string(),
        dx_um: params.dx_um,
        mscale: params.mscale,
        threads,
    };
    Ok(Plan { ctx, cond, params, run_plan, est_seconds: est })
}

// ------------------------------------------------------------------ 신뢰도

fn assess(cond: &PhysConditions, wp: &WorkpieceProps, r: &CutResult) -> PhysReliability {
    let mut low = false;
    let mut why: Vec<String> = Vec::new();
    match wp.key.as_str() {
        "titanium" | "inconel" | "superalloy" => {
            low = true;
            why.push("톱니형(전단 불안정) 칩이 생기는 재료입니다. 이 모델은 균열·전단띠를 다루지 않아 절삭력 변동과 온도를 과소평가할 수 있습니다".into());
        }
        k if k.starts_with("alloy_steel_hrc") => {
            if wp.hardness_hrc >= 45.0 {
                low = true;
                why.push(format!("HRC {:.0} 경화강은 톱니형 칩이 생기기 쉬워 이 모델의 연속 칩 가정과 맞지 않습니다", wp.hardness_hrc));
            } else if wp.hardness_hrc >= 35.0 {
                why.push(format!("HRC {:.0} 부근에서는 칩이 톱니형으로 바뀌기 시작할 수 있습니다", wp.hardness_hrc));
            }
        }
        _ => {}
    }
    let ratio = cond.h_um / cond.h_job_um.max(1e-3);
    if !(0.67..=1.5).contains(&ratio) {
        low = true;
        why.push(format!(
            "해석한 칩 두께 {:.0} µm 가 실제 평균 칩 두께 {:.0} µm 와 {:.1}배 다릅니다. 칩 두께에 따른 비절삭저항 변화(크기 효과)가 모델마다 달라 이 비율은 그만큼 불확실합니다",
            cond.h_um,
            cond.h_job_um,
            if ratio >= 1.0 { ratio } else { 1.0 / ratio }
        ));
    }
    if cond.h_um < 2.5 * cond.edge_um {
        why.push(format!("칩 두께({:.0} µm)가 날 둥글기({:.0} µm)의 2.5배 미만이라 밀림 비중이 큽니다. 이 영역은 격자 해상도에 민감합니다", cond.h_um, cond.edge_um));
    }
    if cond.edge_um > 0.0 && cond.edge_um / cond.dx_um < 2.0 {
        why.push("날 둥글기가 격자 2칸 미만이라 추력(Ft)은 신뢰하지 않습니다. 보정에는 절삭력(Fc)과 온도만 씁니다".into());
    }
    if !r.steady {
        low = true;
        why.push(format!(
            "절삭력이 정상 상태에 들어가지 않았습니다 (꼬리 구간 앞뒤 절반 차이 {:+.0}% — 이동거리가 짧거나 칩 형성이 불안정)",
            r.fc_trend * 100.0
        ));
    }
    if r.chip_at_ceiling {
        low = true;
        why.push("칩이 계산 영역 천장에 닿아 빠져나가지 못하고 쌓였습니다 — 절삭력이 실제보다 크게 나오는 쪽입니다".into());
    }
    if r.ke_ratio_max > 0.05 {
        low = true;
        why.push(format!("운동에너지 비 {:.1}% — 준정적 가정이 흔들립니다 (질량 스케일 과다)", r.ke_ratio_max * 100.0));
    }
    if r.wp_over_wtool.is_finite() && (r.wp_over_wtool < 0.75 || r.wp_over_wtool > 1.15) {
        why.push(format!("에너지 수지(소성일/공구일) {:.2} — 0.85~1.0 이 정상입니다", r.wp_over_wtool));
    }
    if r.tmax_c.is_finite() && r.tmax_c > 1.3 * wp.melt_c {
        why.push(format!("입자 한 곳의 최고온도 {:.0} °C 가 용융점({:.0} °C)을 넘었습니다 — 날 끝의 수치적 열점일 수 있어 평균 접촉면 온도만 씁니다", r.tmax_c, wp.melt_c));
    }
    if !r.chip_ratio.is_finite() {
        low = true;
        why.push("연속 칩이 형성되지 않아 칩 두께비·전단각을 구하지 못했습니다".into());
    } else if !(CHIP_RATIO_MIN..=CHIP_RATIO_MAX).contains(&r.chip_ratio) || (r.shear_angle_deg.is_finite() && !(SHEAR_DEG_MIN..=SHEAR_DEG_MAX).contains(&r.shear_angle_deg)) {
        // 금속 연속 칩의 실측 두께비는 대략 0.2~0.9 (칩 압축비 1.1~5), 전단각 10~45° 이다
        low = true;
        why.push(format!(
            "칩 형성 진단이 비정상입니다 (칩 두께비 {:.2}, 전단각 {:.1}° — 연속 칩의 실측 범위는 대략 {:.2}~{:.1}, {:.0}~{:.0}°). 칩이 정상적으로 분리·배출되지 않았을 수 있으므로 이 기록은 참고용으로만 보세요",
            r.chip_ratio, r.shear_angle_deg, CHIP_RATIO_MIN, CHIP_RATIO_MAX, SHEAR_DEG_MIN, SHEAR_DEG_MAX
        ));
    } else if let Some((lo, hi, src)) = measured_chip_ratio(&wp.key) {
        let verdict = if r.chip_ratio < lo * 0.8 {
            "실측보다 두꺼운 칩(작은 전단각) — 절삭력을 과대평가하는 쪽일 수 있습니다"
        } else if r.chip_ratio > hi * 1.2 {
            "실측보다 얇은 칩(큰 전단각) — 절삭력을 과소평가하는 쪽일 수 있습니다"
        } else {
            "실측 범위 안"
        };
        why.push(format!("칩 두께비 {:.2} (전단각 {:.1}°) · 문헌 실측 {:.2}~{:.2} ({}) → {}", r.chip_ratio, r.shear_angle_deg, lo, hi, src, verdict));
    }
    if r.chip_ratio_geom.is_finite() && r.chip_ratio.is_finite() && (r.chip_ratio_geom / r.chip_ratio - 1.0).abs() > 0.35 {
        why.push(format!(
            "칩 단면으로 잰 두께비 {:.2} 는 칩 속도(질량 보존)로 구한 {:.2} 와 다릅니다 — 날 끝 근처 단면에는 막 전단된 바깥층이 빠져 단면 값이 얇게 나오므로 속도 기준값을 씁니다",
            r.chip_ratio_geom, r.chip_ratio
        ));
    }
    if !r.interface_t_mean_c.is_finite() {
        why.push("공구-칩 접촉면 온도를 구하지 못해 온도 계수는 쓰지 않습니다".into());
    }
    why.push("실험으로 검증되지 않은 2D 평면변형 연속체 계산입니다 (3D 밀링 형상·공구 열전도·냉각유·마모 없음, 건식 직교절삭 비교)".into());
    PhysReliability { level: if low { "low".into() } else { "medium".into() }, reasons: why }
}

fn downsample(h: &[endmill_mpm::HistRow], max: usize) -> Vec<[f64; 4]> {
    if h.is_empty() {
        return Vec::new();
    }
    let step = h.len().div_ceil(max.max(1)).max(1);
    h.iter().step_by(step).map(|r| [fin(r.travel_um), fin(r.fc), fin(r.ft), fin(r.tmax_c)]).collect()
}

/// MPM 결과와 해석식을 맞대어 기록을 만든다.
pub fn make_record(plan: &Plan, r: &CutResult) -> Result<PhysCalRecord, String> {
    if r.cancelled {
        return Err("작업을 취소했습니다".into());
    }
    if !(r.fc_n_per_mm.is_finite() && r.fc_n_per_mm > 1.0) {
        return Err(format!("MPM 절삭력을 구하지 못했습니다 (Fc={:.2} N/mm). 칩 형성이 되지 않았거나 이동거리가 짧습니다", r.fc_n_per_mm));
    }
    let ctx = &plan.ctx;
    let vc = plan.cond.vc_m_min;
    let h_mm = plan.cond.h_um / 1000.0;
    let base = Calibration::default();
    let a0 = analytic_at(ctx, &base, vc, h_mm);
    if !(a0.fc_n_per_mm.is_finite() && a0.fc_n_per_mm > 1e-6) {
        return Err("해석식의 절삭력이 0 이라 비교할 수 없습니다".into());
    }
    // 절삭력 계수: 단순 비로 시작해 고정점 반복으로 해석식이 MPM 값을 되찾도록 맞춘다
    let raw_power = r.fc_n_per_mm / a0.fc_n_per_mm;
    let mut cal = Calibration::default();
    cal.power = raw_power.clamp(0.2, 5.0);
    for _ in 0..2 {
        let a = analytic_at(ctx, &cal, vc, h_mm);
        cal.power = (cal.power * r.fc_n_per_mm / a.fc_n_per_mm.max(1e-9)).clamp(0.2, 5.0);
    }
    let ratio_power = cal.power;
    // 온도 계수: 동력 계수를 적용한 뒤 남는 접촉면 온도 상승의 차이
    let rise_mpm = r.interface_t_mean_c - T0_C;
    let (raw_temperature, ratio_temperature) = if r.interface_t_mean_c.is_finite() && rise_mpm > 5.0 && a0.rise_c > 1.0 {
        let raw_t = rise_mpm / a0.rise_c;
        cal.temperature = 1.0;
        let a = analytic_at(ctx, &cal, vc, h_mm);
        cal.temperature = (rise_mpm / a.rise_c.max(1e-9)).clamp(0.2, 5.0);
        for _ in 0..2 {
            let a = analytic_at(ctx, &cal, vc, h_mm);
            cal.temperature = (cal.temperature * rise_mpm / a.rise_c.max(1e-9)).clamp(0.2, 5.0);
        }
        (raw_t, cal.temperature)
    } else {
        (1.0, 1.0)
    };
    let reliability = assess(&plan.cond, &ctx.wp, r);
    let weight = if reliability.level == "low" { 0.25 } else { 0.5 };
    let created = now_ms();
    Ok(PhysCalRecord {
        id: format!("pc{}", created),
        created_ms: created,
        version: RECORD_VERSION,
        cond: plan.cond.clone(),
        mpm: PhysMpm {
            fc_n_per_mm: r.fc_n_per_mm,
            fc_sd: fin(r.fc_sd),
            ft_n_per_mm: fin(r.ft_n_per_mm),
            interface_t_c: opt(r.interface_t_mean_c),
            interface_t_p95_c: opt(r.interface_t_p95_c),
            interface_rise_c: opt(r.interface_t_mean_c - T0_C),
            tmax_c: fin(r.tmax_c),
            chip_ratio: opt(r.chip_ratio),
            chip_ratio_geom: opt(r.chip_ratio_geom),
            shear_angle_deg: opt(r.shear_angle_deg),
            mu_apparent: opt(r.mu_apparent),
            tau_shear_mpa: opt(r.tau_shear_mpa),
            wp_over_wtool: fin(r.wp_over_wtool),
            ke_ratio_max: fin(r.ke_ratio_max),
            steady: r.steady,
            fc_trend: opt(r.fc_trend),
            chip_at_ceiling: r.chip_at_ceiling,
            steps: r.steps,
            particles: r.particles,
            wall_s: fin(r.wall_s),
            history: downsample(&r.history, 100),
            notes: r.notes.clone(),
        },
        analytic: a0,
        raw_power,
        raw_temperature,
        ratio_power,
        ratio_temperature,
        reliability,
        validated: false,
        apply: false,
        apply_temperature: false,
        weight,
    })
}

/// 계획대로 MPM 을 돌려 기록을 만든다 (저장하지는 않는다). `progress` 가 false 를 돌려주면 취소한다.
pub fn run_plan(plan: &Plan, progress: &(dyn Fn(f64, &str) -> bool + Sync)) -> Result<PhysCalRecord, String> {
    let r = endmill_mpm::run(&plan.params, progress)?;
    make_record(plan, &r)
}

// ------------------------------------------------------------------ 기록 저장소 (프로세스 전역)

fn registry() -> &'static RwLock<Vec<PhysCalRecord>> {
    static R: OnceLock<RwLock<Vec<PhysCalRecord>>> = OnceLock::new();
    R.get_or_init(|| RwLock::new(Vec::new()))
}

fn reg_read() -> RwLockReadGuard<'static, Vec<PhysCalRecord>> {
    registry().read().unwrap_or_else(|e| e.into_inner())
}

fn reg_write() -> RwLockWriteGuard<'static, Vec<PhysCalRecord>> {
    registry().write().unwrap_or_else(|e| e.into_inner())
}

fn store_path() -> &'static Mutex<Option<PathBuf>> {
    static P: OnceLock<Mutex<Option<PathBuf>>> = OnceLock::new();
    P.get_or_init(|| Mutex::new(None))
}

#[derive(Serialize, Deserialize)]
struct FileFormat {
    version: u32,
    records: Vec<PhysCalRecord>,
}

fn persist() -> Result<(), String> {
    let dir = match store_path().lock().unwrap_or_else(|e| e.into_inner()).clone() {
        Some(d) => d,
        None => return Ok(()),
    };
    let body = FileFormat { version: RECORD_VERSION, records: reg_read().clone() };
    let text = serde_json::to_string_pretty(&body).map_err(|e| e.to_string())?;
    let tmp = dir.join("records.json.tmp");
    std::fs::write(&tmp, text).map_err(|e| format!("물리 보정 기록 저장 실패: {}", e))?;
    std::fs::rename(&tmp, dir.join("records.json")).map_err(|e| format!("물리 보정 기록 저장 실패: {}", e))
}

/// 저장 폴더를 지정하고 기록을 읽는다. 읽은 기록 수를 돌려준다.
/// 파일이 깨졌으면 `records.bad.json` 으로 옮기고 빈 상태로 시작한다.
pub fn load_dir(dir: &Path) -> Result<usize, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("물리 보정 폴더를 만들 수 없습니다: {}", e))?;
    *store_path().lock().unwrap_or_else(|e| e.into_inner()) = Some(dir.to_path_buf());
    let file = dir.join("records.json");
    if !file.exists() {
        reg_write().clear();
        return Ok(0);
    }
    let text = std::fs::read_to_string(&file).map_err(|e| format!("물리 보정 기록을 읽을 수 없습니다: {}", e))?;
    match serde_json::from_str::<FileFormat>(&text) {
        Ok(f) => {
            let mut v = f.records;
            v.retain(|r| r.version == RECORD_VERSION);
            let n = v.len();
            *reg_write() = v;
            Ok(n)
        }
        Err(e) => {
            let _ = std::fs::rename(&file, dir.join("records.bad.json"));
            reg_write().clear();
            Err(format!("물리 보정 기록 파일이 올바르지 않아 records.bad.json 으로 옮기고 새로 시작합니다: {}", e))
        }
    }
}

/// 새 기록을 저장소에 넣는다 (꺼진 상태). 오래된 기록은 `MAX_RECORDS` 를 넘으면 지운다.
pub fn add_record(rec: PhysCalRecord) -> Result<(), String> {
    {
        let mut g = reg_write();
        g.retain(|r| r.id != rec.id);
        g.push(rec);
        g.sort_by(|a, b| b.created_ms.cmp(&a.created_ms));
        g.truncate(MAX_RECORDS);
    }
    persist()
}

/// 최신순 기록 목록
pub fn records() -> Vec<PhysCalRecord> {
    let mut v = reg_read().clone();
    v.sort_by(|a, b| b.created_ms.cmp(&a.created_ms));
    v
}

pub fn set_flags(id: &str, apply: Option<bool>, apply_temperature: Option<bool>, weight: Option<f64>) -> Result<PhysCalRecord, String> {
    let out = {
        let mut g = reg_write();
        let r = g.iter_mut().find(|r| r.id == id).ok_or_else(|| format!("물리 보정 기록 {} 이(가) 없습니다", id))?;
        if let Some(w) = weight {
            if !w.is_finite() || !(0.0..=1.0).contains(&w) {
                return Err("가중치는 0 ~ 1 범위여야 합니다".into());
            }
            r.weight = w;
        }
        if let Some(a) = apply {
            r.apply = a;
        }
        if let Some(t) = apply_temperature {
            r.apply_temperature = t;
        }
        if !r.apply {
            r.apply_temperature = false;
        }
        r.clone()
    };
    persist()?;
    Ok(out)
}

pub fn remove(id: &str) -> Result<(), String> {
    {
        let mut g = reg_write();
        let n = g.len();
        g.retain(|r| r.id != id);
        if g.len() == n {
            return Err(format!("물리 보정 기록 {} 이(가) 없습니다", id));
        }
    }
    persist()
}

/// 테스트·진단용: 저장소를 비우고 저장 폴더 지정을 푼다.
pub fn clear_all_for_tests() {
    reg_write().clear();
    *store_path().lock().unwrap_or_else(|e| e.into_inner()) = None;
}

// ------------------------------------------------------------------ 적용

#[derive(Debug, Clone, Serialize)]
pub struct PhysFactor {
    pub axis: &'static str,
    /// 실제로 곱하는 값 (가중치·한계 적용 후)
    pub factor: f64,
    /// 기록의 전체 비 (가중치 적용 전)
    pub ratio: f64,
    pub weight: f64,
    pub id: String,
    pub level: String,
}

impl PhysFactor {
    /// `CalibrationApplied.scope` 에 넣을 설명
    pub fn scope_label(&self) -> String {
        format!("물리 MPM·비검증 · 전체비 ×{:.3} · 가중 {:.2} · {}", self.ratio, self.weight, self.id)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PhysMatch {
    pub id: String,
    pub matched: bool,
    pub reason: String,
}

fn envelope(rec: &PhysCalRecord, ctx: &CutContext, tool_sig: &str, job: &JobCond) -> Result<(), String> {
    if rec.cond.material_key != ctx.wp.key {
        return Err(format!("피삭재가 다릅니다 (기록 {} / 현재 {})", rec.cond.material_label, ctx.wp.label));
    }
    if rec.cond.tool_signature != tool_sig {
        return Err(format!("공구가 다릅니다 (기록 {} / 현재 {})", rec.cond.tool_signature, tool_sig));
    }
    let rv = job.vce_m_min / rec.cond.vc_m_min.max(1e-6);
    if !(0.65..=1.55).contains(&rv) {
        return Err(format!("절삭속도 {:.0} m/min 가 기록 조건 {:.0} m/min 와 크게 다릅니다 (±35% 초과)", job.vce_m_min, rec.cond.vc_m_min));
    }
    if (ctx.tool.rake_deg - rec.cond.rake_deg).abs() > 3.0 {
        return Err(format!("경사각이 다릅니다 (기록 {:.1}° / 현재 {:.1}°)", rec.cond.rake_deg, ctx.tool.rake_deg));
    }
    let re = ctx.tool.edge_radius_um / rec.cond.edge_um.max(1e-6);
    if rec.cond.edge_um > 0.0 && !(0.7..=1.3).contains(&re) {
        return Err(format!("날 둥글기가 다릅니다 (기록 {:.1} µm / 현재 {:.1} µm)", rec.cond.edge_um, ctx.tool.edge_radius_um));
    }
    Ok(())
}

fn blend(ratio: f64, w: f64) -> f64 {
    (1.0 + w * (ratio - 1.0)).clamp(FACTOR_MIN, FACTOR_MAX)
}

/// 이 프로필에 지금 적용되는 물리 보정 계수. 켜진(apply) 기록 중 조건이 맞는 가장 최근 것을 쓴다.
/// `ctx` 는 보정을 곱하기 전의 컨텍스트여야 한다.
pub fn factors_for(p: &MachiningProfile, ctx: &CutContext) -> Vec<PhysFactor> {
    let g = reg_read();
    if g.iter().all(|r| !r.apply) {
        return Vec::new();
    }
    let job = job_cond(ctx, p);
    let sig = p.endmill_setting.signature();
    let mut best: Option<&PhysCalRecord> = None;
    for rec in g.iter().filter(|r| r.apply && r.version == RECORD_VERSION) {
        if envelope(rec, ctx, &sig, &job).is_ok() && best.map_or(true, |b| rec.created_ms > b.created_ms) {
            best = Some(rec);
        }
    }
    let rec = match best {
        Some(r) => r,
        None => return Vec::new(),
    };
    let w = rec.weight.clamp(0.0, 1.0);
    let mut out = Vec::new();
    if w > 0.0 && rec.ratio_power.is_finite() {
        out.push(PhysFactor { axis: "power", factor: blend(rec.ratio_power, w), ratio: rec.ratio_power, weight: w, id: rec.id.clone(), level: rec.reliability.level.clone() });
    }
    if w > 0.0 && rec.apply_temperature && rec.ratio_temperature.is_finite() && (rec.ratio_temperature - 1.0).abs() > 1e-9 {
        out.push(PhysFactor {
            axis: "temperature",
            factor: blend(rec.ratio_temperature, w),
            ratio: rec.ratio_temperature,
            weight: w,
            id: rec.id.clone(),
            level: rec.reliability.level.clone(),
        });
    }
    out
}

/// 화면용: 모든 기록이 이 프로필 조건에 맞는지와 이유
pub fn matches_for(p: &MachiningProfile) -> Vec<PhysMatch> {
    let ctx = CutContext::from_profile(p);
    let job = job_cond(&ctx, p);
    let sig = p.endmill_setting.signature();
    records()
        .iter()
        .map(|r| match envelope(r, &ctx, &sig, &job) {
            Ok(()) => PhysMatch { id: r.id.clone(), matched: true, reason: "조건이 맞습니다".into() },
            Err(e) => PhysMatch { id: r.id.clone(), matched: false, reason: e },
        })
        .collect()
}

// ------------------------------------------------------------------ 백그라운드 작업

#[derive(Debug, Clone, Serialize)]
pub struct PhysJobStatus {
    pub id: u64,
    pub running: bool,
    pub done: bool,
    pub cancelled: bool,
    pub pct: f64,
    pub label: String,
    pub error: Option<String>,
    pub elapsed_s: f64,
    pub eta_s: Option<f64>,
    pub est_total_s: Option<f64>,
    pub record_id: Option<String>,
    pub profile: String,
    pub preset: String,
    pub material: String,
    pub h_um: f64,
    pub particles: usize,
    pub steps: usize,
    pub threads: usize,
}

struct Slot {
    id: u64,
    running: bool,
    done: bool,
    pct: f64,
    label: String,
    error: Option<String>,
    started: Option<Instant>,
    finished: Option<Instant>,
    est_total_s: Option<f64>,
    record_id: Option<String>,
    profile: String,
    preset: String,
    material: String,
    h_um: f64,
    particles: usize,
    steps: usize,
    threads: usize,
    cancel: Arc<AtomicBool>,
}

fn slot() -> MutexGuard<'static, Slot> {
    static S: OnceLock<Mutex<Slot>> = OnceLock::new();
    S.get_or_init(|| {
        Mutex::new(Slot {
            id: 0,
            running: false,
            done: false,
            pct: 0.0,
            label: String::new(),
            error: None,
            started: None,
            finished: None,
            est_total_s: None,
            record_id: None,
            profile: String::new(),
            preset: String::new(),
            material: String::new(),
            h_um: 0.0,
            particles: 0,
            steps: 0,
            threads: 0,
            cancel: Arc::new(AtomicBool::new(false)),
        })
    })
    .lock()
    .unwrap_or_else(|e| e.into_inner())
}

fn status_of(s: &Slot) -> PhysJobStatus {
    let elapsed = match (s.started, s.finished) {
        (Some(a), Some(b)) => b.duration_since(a).as_secs_f64(),
        (Some(a), None) => a.elapsed().as_secs_f64(),
        _ => 0.0,
    };
    let eta = if s.running && s.pct >= 3.0 { Some(elapsed * (100.0 - s.pct) / s.pct) } else { None };
    PhysJobStatus {
        id: s.id,
        running: s.running,
        done: s.done,
        cancelled: s.cancel.load(Ordering::Relaxed) && s.error.is_some(),
        pct: s.pct,
        label: s.label.clone(),
        error: s.error.clone(),
        elapsed_s: elapsed,
        eta_s: eta,
        est_total_s: s.est_total_s,
        record_id: s.record_id.clone(),
        profile: s.profile.clone(),
        preset: s.preset.clone(),
        material: s.material.clone(),
        h_um: s.h_um,
        particles: s.particles,
        steps: s.steps,
        threads: s.threads,
    }
}

pub fn job_status() -> PhysJobStatus {
    status_of(&slot())
}

/// 실행 전에 규모와 예상 시간을 보여 주기 위한 미리보기 (해석은 하지 않는다)
#[derive(Debug, Clone, Serialize)]
pub struct PhysPreview {
    pub cond: PhysConditions,
    pub particles: usize,
    pub steps: usize,
    pub est_seconds: f64,
    pub analytic: PhysAnalytic,
    pub warnings: Vec<String>,
}

pub fn preview(p: &MachiningProfile, o: &PhysRunOptions) -> Result<PhysPreview, String> {
    let plan = build_plan(p, o)?;
    let a = analytic_at(&plan.ctx, &Calibration::default(), plan.cond.vc_m_min, plan.cond.h_um / 1000.0);
    let mut warnings = Vec::new();
    let ratio = plan.cond.h_um / plan.cond.h_job_um.max(1e-3);
    if !(0.67..=1.5).contains(&ratio) {
        warnings.push(format!("가공 조건의 평균 칩 두께는 {:.0} µm 인데 해석 가능 범위({:.0}~{:.0} µm)로 맞춰 {:.0} µm 에서 비교합니다", plan.cond.h_job_um, MPM_H_MIN_UM, MPM_H_MAX_UM, plan.cond.h_um));
    }
    match plan.cond.material_key.as_str() {
        "titanium" | "inconel" | "superalloy" => warnings.push("톱니형 칩 재료 — 신뢰도 낮음".into()),
        _ => {}
    }
    Ok(PhysPreview { particles: plan.run_plan.particles, steps: plan.run_plan.steps, est_seconds: plan.est_seconds, cond: plan.cond, analytic: a, warnings })
}

/// MPM 계산을 백그라운드 스레드로 시작한다. 이미 돌고 있으면 오류.
pub fn job_start(p: MachiningProfile, o: PhysRunOptions) -> Result<PhysJobStatus, String> {
    let plan = build_plan(&p, &o)?;
    let (id, cancel) = {
        let mut s = slot();
        if s.running {
            return Err("물리 계산이 이미 진행 중입니다. 끝나거나 취소한 뒤 다시 시작하세요".into());
        }
        s.id += 1;
        s.running = true;
        s.done = false;
        s.pct = 0.0;
        s.label = "준비".into();
        s.error = None;
        s.started = Some(Instant::now());
        s.finished = None;
        s.est_total_s = Some(plan.est_seconds);
        s.record_id = None;
        s.profile = p.name.clone();
        s.preset = plan.cond.preset.clone();
        s.material = plan.cond.material_label.clone();
        s.h_um = plan.cond.h_um;
        s.particles = plan.run_plan.particles;
        s.steps = plan.run_plan.steps;
        s.threads = plan.cond.threads;
        s.cancel = Arc::new(AtomicBool::new(false));
        (s.id, s.cancel.clone())
    };
    eprintln!(
        "[PHYS {}] START {} · {} · h={:.0}µm vc={:.0}m/min · 입자 {} · 단계 {} · 스레드 {} · 예상 {:.0}초",
        id, plan.cond.material_label, plan.cond.preset, plan.cond.h_um, plan.cond.vc_m_min, plan.run_plan.particles, plan.run_plan.steps, plan.cond.threads, plan.est_seconds
    );
    let spawn = std::thread::Builder::new().name("physcal-mpm".into()).spawn(move || {
        let t0 = Instant::now();
        let progress = |f: f64, msg: &str| -> bool {
            if cancel.load(Ordering::Relaxed) {
                return false;
            }
            let mut s = slot();
            if s.id == id {
                s.pct = (f * 100.0).clamp(0.0, 99.5);
                s.label = msg.to_string();
            }
            true
        };
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run_plan(&plan, &progress)));
        let outcome: Result<PhysCalRecord, String> = match res {
            Ok(r) => r,
            Err(_) => Err("물리 계산 중 내부 오류가 발생했습니다".into()),
        };
        let stored = match outcome {
            Ok(rec) => {
                let rid = rec.id.clone();
                match add_record(rec) {
                    Ok(()) => Ok(rid),
                    Err(e) => Err(format!("계산은 끝났지만 기록 저장에 실패했습니다: {}", e)),
                }
            }
            Err(e) => Err(e),
        };
        let mut s = slot();
        if s.id == id {
            s.running = false;
            s.done = true;
            s.finished = Some(Instant::now());
            match stored {
                Ok(rid) => {
                    s.pct = 100.0;
                    s.label = "완료".into();
                    s.record_id = Some(rid.clone());
                    eprintln!("[PHYS {}] DONE in {:.1}s → {}", id, t0.elapsed().as_secs_f64(), rid);
                }
                Err(e) => {
                    s.label = if s.cancel.load(Ordering::Relaxed) { "취소됨".into() } else { "실패".into() };
                    eprintln!("[PHYS {}] {}: {}", id, s.label, e);
                    s.error = Some(e);
                }
            }
        }
    });
    if let Err(e) = spawn {
        let mut s = slot();
        s.running = false;
        s.done = true;
        s.error = Some(format!("스레드를 시작하지 못했습니다: {}", e));
        return Err(format!("스레드를 시작하지 못했습니다: {}", e));
    }
    Ok(job_status())
}

pub fn job_cancel() -> PhysJobStatus {
    let mut s = slot();
    if s.running {
        s.cancel.store(true, Ordering::Relaxed);
        s.label = "취소 중".into();
    }
    status_of(&s)
}

/// 앱을 끝낼 때: 작업을 취소하고 스레드가 끝나길 기다린다. 끝났으면 true.
pub fn cancel_and_wait(timeout: Duration) -> bool {
    {
        let s = slot();
        if !s.running {
            return true;
        }
        s.cancel.store(true, Ordering::Relaxed);
    }
    let end = Instant::now() + timeout;
    while Instant::now() < end {
        if !slot().running {
            return true;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    !slot().running
}

// ------------------------------------------------------------------ 테스트

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::MachiningPreset;
    use crate::WorkpieceMaterial;

    // 전역 저장소·작업 슬롯을 쓰는 테스트는 서로 겹치지 않게 차례로 돈다
    fn guard() -> MutexGuard<'static, ()> {
        static L: Mutex<()> = Mutex::new(());
        L.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn steel() -> MachiningProfile {
        MachiningProfile::from_preset(&MachiningPreset::default_steel_general(), "steel")
    }

    fn stainless() -> MachiningProfile {
        let mut p = steel();
        p.workpiece_setup.material = WorkpieceMaterial::StainlessSteel;
        p
    }

    fn fake_record(p: &MachiningProfile, power: f64, temp: f64) -> PhysCalRecord {
        let plan = build_plan(p, &PhysRunOptions::default()).expect("plan");
        PhysCalRecord {
            id: format!("test{}", (power * 1000.0) as i64),
            created_ms: 1,
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
                chip_ratio_geom: None,
                shear_angle_deg: Some(33.0),
                mu_apparent: Some(0.4),
                tau_shear_mpa: Some(900.0),
                wp_over_wtool: 0.92,
                ke_ratio_max: 0.01,
                steady: true,
                fc_trend: None,
                chip_at_ceiling: false,
                steps: 1000,
                particles: 1000,
                wall_s: 1.0,
                history: vec![],
                notes: vec![],
            },
            analytic: analytic_at(&plan.ctx, &Calibration::default(), plan.cond.vc_m_min, plan.cond.h_um / 1000.0),
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

    #[test]
    fn analytic_matches_slice_formula_and_scales_with_calibration() {
        let p = stainless();
        let ctx = CutContext::from_profile(&p);
        let job = job_cond(&ctx, &p);
        let h = 0.05;
        let a = analytic_at(&ctx, &Calibration::default(), job.vce_m_min, h);
        assert!(a.fc_n_per_mm > 50.0 && a.fc_n_per_mm < 600.0, "fc={}", a.fc_n_per_mm);
        assert!(a.ft_n_per_mm > 5.0 && a.ft_n_per_mm < a.fc_n_per_mm * 2.0, "ft={}", a.ft_n_per_mm);
        assert!(a.rise_c > 100.0 && a.rise_c < 2500.0, "rise={}", a.rise_c);
        // 동력 보정은 힘과 발열에 거의 비례해야 한다
        let mut cal = Calibration::default();
        cal.power = 0.8;
        let b = analytic_at(&ctx, &cal, job.vce_m_min, h);
        assert!((b.fc_n_per_mm / a.fc_n_per_mm - 0.8).abs() < 0.03, "fc ratio {}", b.fc_n_per_mm / a.fc_n_per_mm);
        assert!(b.rise_c < a.rise_c);
        // 온도 보정은 힘을 건드리지 않고 상승분에 비례한다
        let mut cal = Calibration::default();
        cal.temperature = 1.2;
        let c = analytic_at(&ctx, &cal, job.vce_m_min, h);
        assert!((c.fc_n_per_mm / a.fc_n_per_mm - 1.0).abs() < 0.05, "temperature changed force: {}", c.fc_n_per_mm / a.fc_n_per_mm);
        // 온도가 오르면 물성(열확산율 등)이 바뀌어 정확히 1.2 배는 아니다 — 그래서 계수를 고정점 반복으로 맞춘다
        assert!(c.rise_c / a.rise_c > 1.1 && c.rise_c / a.rise_c < 1.25, "rise ratio {}", c.rise_c / a.rise_c);
    }

    #[test]
    fn plan_uses_app_conditions_and_clamps_chip_thickness() {
        let p = stainless();
        let pl = build_plan(&p, &PhysRunOptions::default()).expect("plan");
        assert_eq!(pl.cond.material_key, "stainless");
        assert!(pl.cond.h_um >= MPM_H_MIN_UM && pl.cond.h_um <= MPM_H_MAX_UM);
        assert!(pl.params.vc_m_s > 0.3 && pl.params.vc_m_s < 10.0);
        assert!((pl.params.material.jc_a - 310e6).abs() < 1.0 && (pl.params.material.jc_b - 1000e6).abs() < 1.0);
        assert!(pl.est_seconds > 1.0);
        let big = build_plan(&p, &PhysRunOptions { h_um: Some(900.0), ..Default::default() }).expect("plan");
        assert_eq!(big.cond.h_um, MPM_H_MAX_UM);
        let small = build_plan(&p, &PhysRunOptions { h_um: Some(1.0), ..Default::default() }).expect("plan");
        assert_eq!(small.cond.h_um, MPM_H_MIN_UM);
        assert!(build_plan(&p, &PhysRunOptions { preset: Some("???".into()), ..Default::default() }).is_err());
        let mut cf = steel();
        cf.workpiece_setup.material = WorkpieceMaterial::CFRP;
        assert!(build_plan(&cf, &PhysRunOptions::default()).is_err());
    }

    #[test]
    fn make_record_reproduces_mpm_values_through_the_app_formulas() {
        // 실제 해석 대신 MPM 결과를 흉내 낸 값으로 계수가 식을 되찾는지 본다
        let p = stainless();
        let plan = build_plan(&p, &PhysRunOptions::default()).expect("plan");
        let a0 = analytic_at(&plan.ctx, &Calibration::default(), plan.cond.vc_m_min, plan.cond.h_um / 1000.0);
        let r = CutResult {
            steps: 1000,
            particles: 1000,
            fc_n_per_mm: a0.fc_n_per_mm * 0.78,
            fc_sd: 2.0,
            ft_n_per_mm: 25.0,
            interface_t_mean_c: T0_C + a0.rise_c * 0.95,
            interface_t_p95_c: 900.0,
            tmax_c: 1100.0,
            chip_ratio: 0.6,
            shear_angle_deg: 33.0,
            mu_apparent: 0.4,
            tau_shear_mpa: 900.0,
            wp_over_wtool: 0.92,
            ke_ratio_max: 0.01,
            steady: true,
            ..Default::default()
        };
        let rec = make_record(&plan, &r).expect("record");
        assert!(!rec.validated && !rec.apply && !rec.apply_temperature);
        assert!((rec.raw_power - 0.78).abs() < 1e-9);
        assert!(rec.ratio_power > 0.7 && rec.ratio_power < 0.85, "ratio_power {}", rec.ratio_power);
        // 두 계수를 모두 적용하면 해석식이 MPM 값을 되찾아야 한다
        let mut cal = Calibration::default();
        cal.power = rec.ratio_power;
        cal.temperature = rec.ratio_temperature;
        let after = analytic_at(&plan.ctx, &cal, plan.cond.vc_m_min, plan.cond.h_um / 1000.0);
        assert!((after.fc_n_per_mm / r.fc_n_per_mm - 1.0).abs() < 0.01, "fc {} vs {}", after.fc_n_per_mm, r.fc_n_per_mm);
        let rise_mpm = r.interface_t_mean_c - T0_C;
        assert!((after.rise_c / rise_mpm - 1.0).abs() < 0.02, "rise {} vs {}", after.rise_c, rise_mpm);
        // 동력을 낮추면 발열도 줄므로 온도 계수는 단순 비(0.95)보다 커야 한다
        assert!(rec.ratio_temperature > rec.raw_temperature, "{} vs {}", rec.ratio_temperature, rec.raw_temperature);
        // 직렬화 왕복
        let js = serde_json::to_string(&rec).unwrap();
        let back: PhysCalRecord = serde_json::from_str(&js).unwrap();
        assert_eq!(back.id, rec.id);
        assert_eq!(back.cond.material_key, "stainless");
    }

    #[test]
    fn unusable_results_are_rejected_or_flagged() {
        let p = stainless();
        let plan = build_plan(&p, &PhysRunOptions::default()).expect("plan");
        let bad = CutResult { fc_n_per_mm: f64::NAN, ..Default::default() };
        assert!(make_record(&plan, &bad).is_err());
        let cancelled = CutResult { cancelled: true, fc_n_per_mm: 100.0, ..Default::default() };
        assert!(make_record(&plan, &cancelled).is_err());
        // 연속 칩이 없고 온도도 못 구한 결과 → 낮음, 온도 계수는 중립
        let r = CutResult { fc_n_per_mm: 120.0, chip_ratio: f64::NAN, interface_t_mean_c: f64::NAN, steady: true, wp_over_wtool: 0.9, ..Default::default() };
        let rec = make_record(&plan, &r).expect("record");
        assert_eq!(rec.reliability.level, "low");
        assert_eq!(rec.ratio_temperature, 1.0);
        assert!(rec.weight <= 0.25 + 1e-12);
        // NaN 이 JSON 에 새어 들어가 읽기 실패를 만들면 안 된다
        let js = serde_json::to_string(&rec).unwrap();
        assert!(serde_json::from_str::<PhysCalRecord>(&js).is_ok());
    }

    #[test]
    fn implausible_chip_diagnostics_lower_reliability() {
        let p = stainless();
        let plan = build_plan(&p, &PhysRunOptions::default()).expect("plan");
        let a0 = analytic_at(&plan.ctx, &Calibration::default(), plan.cond.vc_m_min, plan.cond.h_um / 1000.0);
        let mk = |chip_ratio: f64, shear: f64| CutResult {
            fc_n_per_mm: a0.fc_n_per_mm * 0.96,
            interface_t_mean_c: T0_C + a0.rise_c * 0.83,
            chip_ratio,
            shear_angle_deg: shear,
            steady: true,
            wp_over_wtool: 0.92,
            ke_ratio_max: 0.01,
            ..Default::default()
        };
        // 기본 스테인리스 조건의 빠른 해석에서 실제로 나온 값 (칩 두께비 0.10, 전단각 5.8°)
        let bad = make_record(&plan, &mk(0.10, 5.8)).expect("record");
        assert_eq!(bad.reliability.level, "low");
        assert!(bad.reliability.reasons.iter().any(|s| s.contains("칩 형성 진단")), "{:?}", bad.reliability.reasons);
        assert!(bad.weight <= 0.25 + 1e-12);
        // 정상 범위의 칩은 이 이유로 낮아지지 않는다
        let ok = make_record(&plan, &mk(0.62, 34.6)).expect("record");
        assert!(!ok.reliability.reasons.iter().any(|s| s.contains("칩 형성 진단")), "{:?}", ok.reliability.reasons);
        // 같은 조건을 칩 속도(질량 보존)로 다시 잰 값: 두께비 0.22 · 전단각 12.6° (단면 측정은 0.10)
        // → 실측 범위(SUS304 0.23~0.51, 20% 여유) 안이라 신뢰도를 낮추지 않고 문헌 비교와 측정 차이만 남긴다
        let mut r = mk(0.22, 12.6);
        r.chip_ratio_geom = 0.10;
        let fixed = make_record(&plan, &r).expect("record");
        assert_eq!(fixed.reliability.level, "medium", "{:?}", fixed.reliability.reasons);
        assert!(fixed.reliability.reasons.iter().any(|s| s.contains("문헌 실측") && s.contains("실측 범위 안")), "{:?}", fixed.reliability.reasons);
        assert!(fixed.reliability.reasons.iter().any(|s| s.contains("칩 속도")), "{:?}", fixed.reliability.reasons);
        assert_eq!(fixed.mpm.chip_ratio_geom, Some(0.10));
        // 칩이 천장에 닿아 쌓였거나 힘이 계속 오르면 낮음
        let mut jam = mk(0.37, 21.5);
        jam.chip_at_ceiling = true;
        let j = make_record(&plan, &jam).expect("record");
        assert_eq!(j.reliability.level, "low");
        assert!(j.reliability.reasons.iter().any(|s| s.contains("천장")), "{:?}", j.reliability.reasons);
        assert!(j.mpm.chip_at_ceiling);
        let mut rising = mk(0.37, 21.5);
        rising.steady = false;
        rising.fc_trend = 0.15;
        let r2 = make_record(&plan, &rising).expect("record");
        assert_eq!(r2.reliability.level, "low");
        assert!(r2.reliability.reasons.iter().any(|s| s.contains("+15%")), "{:?}", r2.reliability.reasons);
        // 예전 기록(단면 두께비 필드 없음)도 읽힌다
        let mut js: serde_json::Value = serde_json::to_value(&fixed).unwrap();
        for k in ["chip_ratio_geom", "fc_trend", "chip_at_ceiling"] {
            js["mpm"].as_object_mut().unwrap().remove(k);
        }
        let back: PhysCalRecord = serde_json::from_value(js).unwrap();
        assert_eq!(back.mpm.chip_ratio_geom, None);
    }

    #[test]
    fn titanium_is_low_reliability() {
        let mut p = steel();
        p.workpiece_setup.material = WorkpieceMaterial::Titanium;
        let plan = build_plan(&p, &PhysRunOptions::default()).expect("plan");
        let r = CutResult {
            fc_n_per_mm: 150.0,
            interface_t_mean_c: 700.0,
            chip_ratio: 0.5,
            shear_angle_deg: 30.0,
            steady: true,
            wp_over_wtool: 0.9,
            ..Default::default()
        };
        let rec = make_record(&plan, &r).expect("record");
        assert_eq!(rec.reliability.level, "low");
        assert!(rec.reliability.reasons.iter().any(|s| s.contains("톱니형")));
    }

    // 전역 저장소를 쓰므로 한 테스트에서 순서대로 확인한다
    #[test]
    fn registry_gating_weights_and_persistence() {
        let _g = guard();
        clear_all_for_tests();
        let p = stainless();
        let ctx = CutContext::from_profile(&p);
        assert!(factors_for(&p, &ctx).is_empty(), "빈 저장소는 아무것도 적용하지 않아야 한다");
        let rec = fake_record(&p, 0.8, 1.2);
        let id = rec.id.clone();
        add_record(rec).unwrap();
        assert!(factors_for(&p, &ctx).is_empty(), "기본은 꺼짐");
        // 켜면 power 만 (가중치 0.5 → 0.9)
        set_flags(&id, Some(true), None, None).unwrap();
        let f = factors_for(&p, &ctx);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].axis, "power");
        assert!((f[0].factor - 0.9).abs() < 1e-9, "factor {}", f[0].factor);
        assert!(f[0].scope_label().contains("비검증"));
        // 온도까지 켜면 temperature 도 (1 + 0.5·0.2 = 1.1)
        set_flags(&id, None, Some(true), Some(0.5)).unwrap();
        let f = factors_for(&p, &ctx);
        assert_eq!(f.len(), 2);
        assert!((f[1].factor - 1.1).abs() < 1e-9);
        // 가중치 1 은 전체 비, 한계는 0.5 ~ 1.5
        set_flags(&id, None, None, Some(1.0)).unwrap();
        assert!((factors_for(&p, &ctx)[0].factor - 0.8).abs() < 1e-9);
        assert!(set_flags(&id, None, None, Some(1.5)).is_err());
        assert!((blend(10.0, 1.0) - FACTOR_MAX).abs() < 1e-12 && (blend(0.01, 1.0) - FACTOR_MIN).abs() < 1e-12);
        // 다른 피삭재·다른 속도는 적용하지 않는다
        let other = steel();
        let octx = CutContext::from_profile(&other);
        assert!(factors_for(&other, &octx).is_empty());
        assert!(!matches_for(&other)[0].matched);
        let mut fast = stainless();
        fast.conditions.spindle_rpm = (fast.conditions.spindle_rpm as f64 * 2.5) as u32;
        let fctx = CutContext::from_profile(&fast);
        assert!(factors_for(&fast, &fctx).is_empty());
        let m = matches_for(&fast);
        assert!(!m[0].matched && m[0].reason.contains("절삭속도"), "{:?}", m[0]);
        // 끄면 온도 적용도 함께 꺼진다
        set_flags(&id, Some(false), None, None).unwrap();
        assert!(factors_for(&p, &ctx).is_empty());
        assert!(!records()[0].apply_temperature);
        // 저장 → 새로 읽기
        let dir = std::env::temp_dir().join(format!("physcal_test_{}", now_ms()));
        load_dir(&dir).unwrap();
        add_record(fake_record(&p, 0.7, 1.0)).unwrap();
        set_flags("test700", Some(true), None, None).unwrap();
        reg_write().clear();
        assert_eq!(load_dir(&dir).unwrap(), 1);
        assert!(records()[0].apply && records()[0].id == "test700");
        // 깨진 파일은 옮기고 빈 상태로 시작한다
        std::fs::write(dir.join("records.json"), "{ not json").unwrap();
        assert!(load_dir(&dir).is_err());
        assert!(records().is_empty() && dir.join("records.bad.json").exists());
        assert!(remove("nope").is_err());
        let _ = std::fs::remove_dir_all(&dir);
        clear_all_for_tests();
    }

    #[test]
    fn job_start_rejects_double_start_and_cancel_leaves_no_record() {
        let _g = guard();
        clear_all_for_tests();
        let p = stainless();
        // 상태가 비어 있어야 시작할 수 있다
        let st0 = job_status();
        assert!(!st0.running);
        // 너무 작은 격자 설정을 직접 만들 수 없으므로 quick + 가장 얇은 칩으로 계획만 확인하고,
        // 실제 실행은 endmill_mpm 의 짧은 실행 테스트와 아래 취소 경로로 확인한다.
        let st = job_start(p.clone(), PhysRunOptions { preset: Some("quick".into()), h_um: Some(60.0), threads: Some(2) }).expect("start");
        assert!(st.running && st.id >= 1);
        assert!(job_start(p.clone(), PhysRunOptions::default()).is_err(), "동시에 두 작업을 시작할 수 없다");
        let c = job_cancel();
        assert!(c.running || c.done);
        assert!(cancel_and_wait(Duration::from_secs(60)), "취소 뒤 스레드가 끝나야 한다");
        let end = job_status();
        assert!(end.done && !end.running);
        assert!(end.error.is_some(), "취소하면 오류(취소됨)로 끝난다: {:?}", end);
        assert!(records().is_empty(), "취소된 작업은 기록을 남기지 않는다");
        clear_all_for_tests();
    }
}
