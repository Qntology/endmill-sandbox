//! 공구–피삭재 조합 기준값 (pair reference).
//!
//! 예전에는 모든 조합에 같은 고정값을 썼다 — 길들이기 VB 는 VB 한계의 15 %, 시상수는 선형 수명의 3 %, 감쇠비 0.03,
//! 여유각 10°, 공구–칩 마찰은 코팅의 핀온디스크 값. 여기서는 피삭재 표(`WorkpieceProps` 의 `cut_mu`·`kre_over_kte`·
//! `breakin_*`·`vb_criterion_mm`·`clearance_deg` 등)에 들어간 문헌 기준값에, 코팅·날끝·홀더·돌출비처럼 공구 쪽에서
//! 정해지는 보정을 곱한다. 각 값의 출처는 피삭재 표의 주석과 README 에 있다.

use crate::physics::{CoatingTribology, CutContext, ToolGeometry, WorkpieceProps};
use crate::profile::HolderType;
use serde::{Deserialize, Serialize};

/// 피삭재 그룹 (기준값 표의 단위)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WpGroup {
    Aluminum,
    CarbonSteel,
    AlloySteel,
    Hardened,
    Stainless,
    Titanium,
    Nickel,
    Cfrp,
}

impl WpGroup {
    pub fn of(wp: &WorkpieceProps) -> Self {
        match wp.key.as_str() {
            "aluminum" => Self::Aluminum,
            "carbon_steel" => Self::CarbonSteel,
            "stainless" => Self::Stainless,
            "titanium" => Self::Titanium,
            "inconel" | "superalloy" => Self::Nickel,
            "cfrp" => Self::Cfrp,
            k if k.starts_with("alloy_steel") => {
                if wp.hardness_hrc > 45.0 {
                    Self::Hardened
                } else {
                    Self::AlloySteel
                }
            }
            _ => Self::CarbonSteel,
        }
    }

    pub fn key(&self) -> &'static str {
        match self {
            Self::Aluminum => "aluminum",
            Self::CarbonSteel => "carbon_steel",
            Self::AlloySteel => "alloy_steel",
            Self::Hardened => "hardened_steel",
            Self::Stainless => "stainless",
            Self::Titanium => "titanium",
            Self::Nickel => "nickel_alloy",
            Self::Cfrp => "cfrp",
        }
    }

    fn ferrous(&self) -> bool {
        matches!(self, Self::CarbonSteel | Self::AlloySteel | Self::Hardened | Self::Stainless)
    }
}

/// 코팅 그룹. PCD 모재는 다이아몬드로 본다
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CoatGroup {
    Uncoated,
    /// TiN · TiCN
    TiNitride,
    /// AlTiN · TiAlN · HiPIMS AlTiN 계열
    AlTiNitride,
    AlCrN,
    /// TiSiN · nACo (Si 나노복합)
    SiNano,
    /// CrN · ZrN
    CrZr,
    Dlc,
    Diamond,
}

impl CoatGroup {
    pub fn of(coat: &CoatingTribology, substrate: &str) -> Self {
        if substrate == "pcd" {
            return Self::Diamond;
        }
        match coat.family.as_str() {
            "uncoated" => Self::Uncoated,
            "TiN" | "TiCN" => Self::TiNitride,
            "AlCrN" => Self::AlCrN,
            "TiSiN" | "nACo" => Self::SiNano,
            "CrN" | "ZrN" => Self::CrZr,
            "DLC" => Self::Dlc,
            "Diamond" => Self::Diamond,
            _ => Self::AlTiNitride,
        }
    }

    pub fn key(&self) -> &'static str {
        match self {
            Self::Uncoated => "uncoated",
            Self::TiNitride => "ti_nitride",
            Self::AlTiNitride => "altin",
            Self::AlCrN => "alcrn",
            Self::SiNano => "si_nanocomposite",
            Self::CrZr => "crn_zrn",
            Self::Dlc => "dlc",
            Self::Diamond => "diamond",
        }
    }
}

/// 코팅이 절삭 중 겉보기 마찰을 바꾸는 배수 (무코팅 = 1).
/// 거시 절삭에서 코팅이 바꾸는 절삭력은 작다: C45 선삭 AlTiN·TiAlCrN −0.4~−4 % (Kovalčík 외 2022, Materials 15:590),
/// Al-Si 선삭 DLC −4~−6 % (효과 없다는 보고도 있음), 7075 미세 밀링 DLC −20 %·NCD −40 % 는 날끝이 지배하는 미세 스케일의 상한.
/// 이 마찰 배수 0.88 은 경사 16~18° 알루미늄 공구에서 kc 를 약 −5 % 바꾼다. Ti·Ni·고경도는 공개 정량 자료가 없어 |Δ| ≤ 5 % 로 둔다
pub fn coating_mu_factor(wp: WpGroup, coat: CoatGroup) -> f64 {
    match (wp, coat) {
        (_, CoatGroup::Uncoated) => 1.0,
        (WpGroup::Aluminum, CoatGroup::Dlc | CoatGroup::Diamond) => 0.88,
        (WpGroup::Aluminum, CoatGroup::CrZr) => 0.95,
        (WpGroup::Aluminum, _) => 1.0,
        (WpGroup::Cfrp, CoatGroup::Diamond) => 0.90,
        (WpGroup::Cfrp, CoatGroup::Dlc) => 0.95,
        (WpGroup::Cfrp, _) => 1.0,
        (g, CoatGroup::Dlc | CoatGroup::Diamond) if g.ferrous() => 1.0,
        (g, _) if g.ferrous() => 0.97,
        _ => 0.98,
    }
}

/// 공구–칩 겉보기 마찰 (코팅 보정 포함, 온도·윤활 보정 전)
pub fn cutting_mu(ctx: &CutContext) -> f64 {
    let wp = WpGroup::of(&ctx.wp);
    let coat = CoatGroup::of(&ctx.tool.coating, &ctx.tool.substrate);
    ctx.wp.cut_mu * coating_mu_factor(wp, coat)
}

/// 길들이기 (초기 급마모)
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct BreakIn {
    pub vb_mm: f64,
    pub tau_min: f64,
    /// 날끝 반경 보정 (날카로울수록 큼)
    pub edge_factor: f64,
    /// 절삭속도 보정 (Ti·Ni)
    pub speed_factor: f64,
}

/// 조합 길들이기: 피삭재 기준값 × 날끝 보정 (10 µm/r)^0.25 (0.75~1.3) × 속도 보정 (Vc/Vc_ref)^지수 (0.7~2.0),
/// VB 한계의 40 % 를 넘지 않는다. 호닝한 코팅 날은 길들이기가 거의 없고 (316L, 30분간 마모 안 보임) 날카로운 날은 수 초 만에
/// 30~60 µm 를 잃는다 (HRC 52, PHM2010) — 방향은 실측으로, 지수는 가정으로 정했다
pub fn breakin(ctx: &CutContext, vc_m_min: f64) -> BreakIn {
    let wp = &ctx.wp;
    let edge = ctx.tool.edge_radius_um.max(1.0);
    let edge_factor = (10.0 / edge).powf(0.25).clamp(0.75, 1.3);
    let speed_factor = if wp.breakin_vc_exp > 0.0 && wp.ref_vc > 0.0 && vc_m_min > 0.0 {
        (vc_m_min / wp.ref_vc).powf(wp.breakin_vc_exp).clamp(0.7, 2.0)
    } else {
        1.0
    };
    let vb = (wp.breakin_vb_um * edge_factor * speed_factor / 1000.0).min(0.4 * ctx.vb_limit_mm());
    BreakIn {
        vb_mm: vb.max(0.0),
        tau_min: wp.breakin_tau_min.max(0.05),
        edge_factor,
        speed_factor,
    }
}

/// 공구 끝 굽힘 모드 감쇠비의 돌출비 기본값. 실측: L/D 3.75 (Ø20 열박음 HSK-A63) 공구 모드 0.046~0.057,
/// L/D 5.8 (Ø12 4날) 0.026, L/D 12 (Ø8 열박음) 0.009 → ζ ≈ 0.28·(L/D)^−1.38 + 홀더 보정.
/// 짧은 공구는 홀더 모드(≈2000 Hz, ζ 0.020~0.025)의 몫이 커지므로 상한을 0.045 로 둔다 (0.010~0.045).
/// 정지 상태 충격 시험 값이라 가공 중(+30~60 %)보다 보수적이다
pub fn default_damping(l_over_d: f64, holder: HolderType) -> f64 {
    let ld = l_over_d.max(1.0);
    (0.28 * ld.powf(-1.38)).clamp(0.010, 0.045) + holder.damping_bonus()
}

/// 여유각에 따른 공정감쇠 보정. 계수 C 는 여유각 15° → 11° 에서 +23~42 % (Tyler·Schmitz 2014, NAMRI/SME 42 — 세 소재) → C ∝ tan(α)^−0.8,
/// 이 모델의 이득은 계수의 제곱으로 커지므로 계수 배수는 (tan 10° / tan α)^0.4 (0.7~1.5)
pub fn pd_clearance_factor(clearance_deg: f64) -> f64 {
    let a = clearance_deg.clamp(2.0, 30.0).to_radians().tan();
    ((10f64).to_radians().tan() / a).powf(0.4).clamp(0.7, 1.5)
}

/// 공구 수명의 불확실성 (로그 수명 표준편차 σ). 교체까지 부품 수의 보수(10 % 분위) 예측 = 중앙값 × exp(−1.2816·σ_total).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct LifeSpread {
    /// 같은 조건에서 공구마다 다른 수명 (측정으로 줄일 수 없음)
    pub sigma_scatter: f64,
    /// 모델 예측 오차 (실측·보정으로 줄어듦)
    pub sigma_model: f64,
    pub sigma_total: f64,
    /// 공구 간 편차만 반영한 10 % 분위 / 중앙값
    pub p10_scatter_only: f64,
    /// 둘 다 반영한 10 % 분위 / 중앙값
    pub p10_total: f64,
    pub chipping_regime: bool,
    pub calibrated: bool,
}

/// 공구 간 수명 편차(σ_ln ≈ CoV, Weibull 형상 β ≈ 1.2/CoV)와 보정 전후 모델 오차.
/// 실측: 강 초경 마모 지배 0.12~0.19 (Karandikar·Abbas·Schmitz 2013, Mach. Sci. Technol. 17:410 — 1018 강 3속도 × 3공구), Ti-6Al-4V 코팅 초경 0.14
/// (Tatar 외 2020, Heliyon 6:e04217 — 반복 12쌍), 스테인리스·Ni 20~30 % (고전 집계), 치핑·채터가 지배하면 파괴형이라 0.40~0.60 (β 2~3).
/// 알루미늄·CFRP 는 자료가 없어 0.30 (가정). 보정 전 모델 오차는 문헌 Taylor 상수가 한 사례에서 8.4배, 맞춤 외삽 1.3~2.1배 → σ 0.5,
/// 같은 조합 실측(VB·수명) 또는 원장 보정이 있으면 0.2. 공구 간 편차는 측정으로 없어지지 않는다
pub fn life_spread(ctx: &CutContext, chatter_fraction: f64, min_margin: f64, measured: bool) -> LifeSpread {
    let g = WpGroup::of(&ctx.wp);
    let base: f64 = match g {
        WpGroup::CarbonSteel | WpGroup::AlloySteel => 0.18,
        WpGroup::Titanium => 0.20,
        WpGroup::Stainless | WpGroup::Nickel => 0.28,
        WpGroup::Hardened => 0.40,
        WpGroup::Aluminum | WpGroup::Cfrp => 0.30,
    };
    let chipping = chatter_fraction > 0.3 && min_margin > 0.0 && min_margin < 1.0;
    let sigma_scatter = if chipping { base.max(0.45) } else { base };
    let calibrated = measured || (ctx.calib.wear - 1.0).abs() > 1e-6;
    let sigma_model = if calibrated { 0.2 } else { 0.5 };
    let sigma_total = (sigma_scatter * sigma_scatter + sigma_model * sigma_model).sqrt();
    LifeSpread {
        sigma_scatter,
        sigma_model,
        sigma_total,
        p10_scatter_only: (-1.2816 * sigma_scatter).exp(),
        p10_total: (-1.2816 * sigma_total).exp(),
        chipping_regime: chipping,
        calibrated,
    }
}

/// 런아웃 기본값의 출처
pub fn runout_source(s: &crate::profile::EndMillMockupSetting) -> &'static str {
    if s.runout_um.is_some() {
        "입력값"
    } else if s.holder.tir_um().is_some() {
        "홀더 사양"
    } else if s.is_high_end {
        "고급형 공구 가정 (홀더 미지정)"
    } else {
        "보급형 공구 가정 (홀더 미지정)"
    }
}

/// 감사 로그용: 이 작업에 쓴 조합 기준값과 근거
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PairSummary {
    pub wp_group: String,
    pub coat_group: String,
    pub holder: String,
    pub cut_mu_base: f64,
    pub coating_mu_factor: f64,
    pub kr_floor: f64,
    pub kte_factor: f64,
    pub kre_over_kte: f64,
    pub min_chip_ratio: f64,
    pub clearance_deg: f64,
    pub clearance_source: String,
    pub edge_radius_um: f64,
    pub runout_um: f64,
    pub runout_source: String,
    pub damping_ratio: f64,
    pub damping_source: String,
    pub process_damping: f64,
    pub pd_clearance_factor: f64,
    pub thermal_factor: f64,
    pub breakin_vb_um: f64,
    pub breakin_tau_min: f64,
    pub vb_limit_mm: f64,
    pub vb_criterion_mm: f64,
    pub vb_limit_basis: String,
    /// 근거가 약한(직접 실측이 없는) 항목
    pub low_confidence: Vec<String>,
}

pub fn summary(s: &crate::profile::EndMillMockupSetting, ctx: &CutContext, vc_m_min: f64) -> PairSummary {
    let t: &ToolGeometry = &ctx.tool;
    let wp = &ctx.wp;
    let g = WpGroup::of(wp);
    let c = CoatGroup::of(&t.coating, &t.substrate);
    let b = breakin(ctx, vc_m_min);
    let tol_rule = if ctx.tolerance_mm <= 0.02 { 0.2 } else { 0.3 };
    let limit = ctx.vb_limit_mm();
    let measured_frf = t.measured_fn_hz.is_some() && t.measured_k_n_per_um.is_some();
    let mut low = Vec::new();
    match g {
        WpGroup::Nickel => low.push("니켈 합금 절삭 계수(Kr 하한·μ·Kre/Kte)와 길들이기는 공개 실측이 없어 강·스테인리스 사이로 둔 가정값".to_string()),
        WpGroup::Cfrp => low.push("CFRP 는 물리적인 엔드밀 계수 세트가 없음 — μ·Kr 하한·길들이기는 가정값".to_string()),
        WpGroup::Aluminum => low.push("알루미늄 길들이기 크기(15 µm)는 직접 실측 없음".to_string()),
        WpGroup::Hardened => low.push("고경도 Kre/Kte(1.0)·열 계수는 볼 엔드밀 1세트·문헌 대역 하나에 근거".to_string()),
        WpGroup::Stainless => low.push("스테인리스 날끝 계수 배수(1.3)·Kre/Kte(1.0)는 선삭 3점 맞춤에 근거".to_string()),
        _ => {}
    }
    if s.holder == HolderType::MillingChuck || s.holder == HolderType::Polygon {
        low.push("밀링 척·폴리곤 홀더 TIR 은 공개 수치가 없어 가정값".to_string());
    }
    if s.holder == HolderType::Hydraulic {
        low.push("유압 척 감쇠 보정(+0.005)은 정성 보고에 근거".to_string());
    }
    PairSummary {
        wp_group: g.key().into(),
        coat_group: c.key().into(),
        holder: s.holder.label().into(),
        cut_mu_base: wp.cut_mu,
        coating_mu_factor: coating_mu_factor(g, c),
        kr_floor: wp.kr_floor,
        kte_factor: wp.kte_factor,
        kre_over_kte: wp.kre_over_kte,
        min_chip_ratio: wp.min_chip_ratio,
        clearance_deg: t.clearance_deg,
        clearance_source: if s.clearance_deg.is_some() { "입력값".into() } else { format!("피삭재 기본 ({:.0}°)", wp.clearance_deg) },
        edge_radius_um: t.edge_radius_um,
        runout_um: t.runout_um,
        runout_source: runout_source(s).into(),
        damping_ratio: t.damping_ratio,
        damping_source: if ctx.machine.damping_is_user() {
            "입력값 (탭 테스트)".into()
        } else if measured_frf {
            format!("돌출비 기본값 (L/D {:.1}) · FRF 실측", t.stickout_mm / t.diameter_mm.max(1e-9))
        } else {
            format!("돌출비 기본값 (L/D {:.1})", t.stickout_mm / t.diameter_mm.max(1e-9))
        },
        process_damping: wp.process_damping,
        pd_clearance_factor: pd_clearance_factor(t.clearance_deg),
        thermal_factor: wp.thermal_factor,
        breakin_vb_um: b.vb_mm * 1000.0,
        breakin_tau_min: b.tau_min,
        vb_limit_mm: limit,
        vb_criterion_mm: wp.vb_criterion_mm,
        vb_limit_basis: if wp.vb_criterion_mm < tol_rule - 1e-9 {
            format!("피삭재 판정 {:.2} mm (공차 규칙 {:.1} mm 보다 작음)", wp.vb_criterion_mm, tol_rule)
        } else {
            format!("공차 규칙 {:.1} mm (공차 {:.0} µm, 피삭재 판정 {:.2} mm)", tol_rule, ctx.tolerance_mm * 1000.0, wp.vb_criterion_mm)
        },
        low_confidence: low,
    }
}
