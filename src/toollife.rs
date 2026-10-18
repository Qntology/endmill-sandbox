//! 공구 수명 말기와 파손 — 경로 시뮬레이션이 절삭 스텝마다 공구 상태를 감시하는 규칙.
//!
//! 예전 경로 시뮬레이션은 VB 를 한계의 2배에서 자르기만 하고 경로 끝까지 깎았다. 인코넬 황삭 포켓은 121 s 에 VB 한계(0.20 mm)를
//! 넘은 뒤에도 134 s 를 더 깎아 17.9 cm³ 를 제거했고 합력이 252 N → 8,064 N 까지 올랐다 (Ø12 초경 몸체 굽힘 응력 약 3.7 GPa —
//! 초경 항절력보다 큰 값이라 실제로는 그 전에 부러진다). 고경도 정삭 포켓은 한계를 넘은 뒤 26 분을 더 깎았다.
//!
//! 여기서는 절삭 스텝마다
//!  1) 수명 사용률 u = max(균일 VB / VB 한계, 국부 VBmax / 국부 한계) — ISO 8688-2 의 균일 VB 0.3 / 국부 VBmax 0.5 mm 구조
//!     (`toolwear::LOCAL_LIMIT_RATIO`),
//!  2) 공구 몸체(날 끝 단면 · 넥 · 홀더 면 샹크)의 굽힘 + 비틀림 응력과 정적 유효 강도, 회전 굽힘 피로 (Basquin + Miner),
//!  3) 날 치핑 누적 위험 (채터 · 몸체 응력 · 피삭재 경도 · 열피로 균열 · 한계 초과 · 용도 적합도 배수 → 확률 1 − e^−H),
//!  4) 날끝 소성 변형 온도 (모재가 버티는 온도를 넘은 시간),
//! 를 보고, 수명 판정에 닿으면 기계 설정의 정책(`ToolLifePolicy`)대로 처리한다. 결정적 파손(정적 과부하 · 피로 · 날끝 붕괴 · 소성 변형)은
//! 정책과 관계없이 그 자리에서 정지한다 — 부러진 공구로 더 깎는 기계는 없고(파손 감시: 부하 1.5배 이상 10 ms 안에 이송 정지·후퇴),
//! 부품은 검사 대상이다.
//!
//! 정책은 실제 제어기·감시 장치의 동작을 따른다 (FANUC 30i 공구 수명 관리 A-79409E, SINUMERIK 840D 공구 감시, Caron TMAC, 감시 특허 —
//! 리서치 정리는 `docs/tool-life-and-pair-matrix.md`):
//!  * 정지: 마모 경보(학습 부하 1.2~1.25배 0.5~1 s)에 지금 절삭을 마치고 정지 — 작업자가 교체한다.
//!  * 교체 후 계속: 지금 절삭(세그먼트)을 마치고 예비 공구로 교체한 뒤 이어서 깎는다 (TMAC: "completes the current cut … indexes to the next tool").
//!  * 프로그램 끝까지: 횟수·시간 기반 공구 수명 관리 — 만료돼도 프로그램은 끝까지 가고 다음 부품부터 예비 공구 (FANUC: "Machining … continues
//!    until the end of the program"). 그 사이 3차 마모·파손 판정은 그대로 적용된다.
//!  * 감시 없음: 끝까지 깎고 다음 작업도 같은 공구 (파손까지).
//! 수명 사용률이 판정의 1.2배를 넘었거나 VB 가 3차 마모 구간의 중간(가속 시작 ~ 붕괴)을 넘었으면 '지금 절삭을 마치는' 유예 없이 그 자리에서 처리한다.
//!
//! 확률적 사건(치핑)은 결정적으로 '일어났다' 고 하지 않는다. 누적 확률을 경로를 따라 남기고, 감시가 없는 정책(프로그램 끝까지 · 감시 없음)에서만
//! 확률 90 % 를 넘으면 치핑 파손으로 끝낸다. 수치의 근거와 확신도는 각 상수의 주석과 `docs/tool-life-and-pair-matrix.md` 에 있다.
use crate::pairref::WpGroup;
use crate::physics::{ToolGeometry, WorkpieceProps};
use crate::profile::ToolLifePolicy;
use crate::toolwear::WearField;
use serde::{Deserialize, Serialize};
use std::f64::consts::PI;

/// 경고(교체 준비)를 남기는 수명 사용률 — 2차 보정 게이트의 '현재 플랭크 마모' 주의(0.8 × 한계)와 같은 값
pub const WARN_FRACTION: f64 = 0.8;
/// 치핑 누적 확률이 이 값을 넘으면 그 공구는 이가 빠진 것으로 본다. 공구가 겪는 위험은 감시 여부와 관계없이 같고 대응만 다르다:
/// 감시 없는 정책(프로그램 끝까지 · 감시 없음)은 치핑 파손으로 끝나고, 부하 감시가 있는 정책(정지 · 교체 후 계속)은
/// 파손 경보가 잡은 것으로 보고 그 자리에서 정지·교체한다 — 감시하는 쪽이 같은 공구로 더 멀리 깎는 일이 없게
pub const CHIP_FAILURE_P: f64 = 0.9;
/// 치핑 누적 확률이 이 값을 넘으면 경고를 한 번 남긴다 (정지·교체 정책에서는 검사 권고)
pub const CHIP_WARN_P: f64 = 0.5;
/// 날끝이 소성 변형 온도를 넘은 시간이 이만큼 쌓이면 날끝 붕괴로 본다 (s). 고온에서 날끝 처짐은 수 초 안에 진행된다 (확신도 낮음)
pub const PLASTIC_SECONDS: f64 = 1.0;
/// 수명 판정의 이 배수를 넘으면 '지금 절삭을 마치고' 의 유예 없이 바로 처리한다 (마모 경보 대역 위의 강한 경보)
pub const HARD_LIMIT_FACTOR: f64 = 1.2;
/// 부하 감시의 마모 경보 배수 (새 날 학습 부하 대비 1.2~1.25배, 감시 특허 US4633424·US4723219)
pub const LOAD_ALARM_RATIO: f64 = 1.2;
/// 부하 감시의 평균 시간 (s) — 마모 경보는 0.5~1 s 동안 넘어야 낸다 (US4633424 700 ms, US4723219 약 1 s)
pub const LOAD_MONITOR_TAU_S: f64 = 0.5;
/// 결정적 파손 판정이 아니라 수치 보호용 VB 상한 = 날끝 붕괴 VB × 이 값
pub const VB_GUARD_FACTOR: f64 = 1.25;

/// 피삭재 그룹별 수명 말기 기준값 (절대 VB, mm). 수명 판정 VB 는 공차·피삭재로 정하는 '관리 값'이고,
/// 3차 마모 시작·날끝 붕괴는 공구가 실제로 겪는 물리라 판정값과 따로 둔다
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct EolParams {
    /// 3차(가속) 마모가 시작되는 균일 VB (mm)
    pub vb_accel_mm: f64,
    /// 날끝 붕괴(결정적 파손)로 보는 균일 VB (mm) — 국부 VBmax 는 이 값 × `toolwear::LOCAL_LIMIT_RATIO`
    pub vb_collapse_mm: f64,
    /// 날끝 붕괴 VB 에서의 정상 마모율 배수: r = r_ss · exp(ln(배수) · (VB − VB_가속) / (VB_붕괴 − VB_가속))
    pub accel_mult: f64,
    /// 적합한 조건·안정 절삭에서 VB 한계에 닿기 전에 치핑할 확률 (치핑 기본 위험)
    pub chip_base_p: f64,
    /// 정적 유효 강도 / 항절력 — 시편보다 큰 응력 체적(Weibull m 10~20)과 이 피삭재 그룹이 주는 충격
    pub static_frac: f64,
    /// 회전 굽힘 피로 한도 / 항절력 (10⁶~10⁷ 회, WC-Co 는 굽힘 강도의 0.5~0.7 → 공구 크기·충격을 넣어 0.40~0.45)
    pub fatigue_frac: f64,
}

/// 피삭재 그룹별 수명 말기 기준값 (리서치 정리 표 — 확신도: 판정 VB 보통, 가속 시작·배수 낮음, 붕괴 VB 낮음~보통, 강도 비 낮음~보통).
/// 3차 마모: 강 VB 0.3~0.4 mm, 스테인리스 0.2~0.3, Ti·Ni 0.15~0.25, 고경도 ≈0.1 mm 부근에서 시작해 정상 마모율의 3~10배
/// (Usui 마모식 B ≈ 5.3e3 → 2.2e4 K 로 바뀌는 880 °C 위에서는 100 K 마다 ×4.6 — 마모 랜드 → 발열 → 마모의 되먹임).
/// 날끝 붕괴는 ISO 8688-2 의 국부 VB 0.5 mm·치핑 0.5 mm 와 Ti-6Al-4V Ø6 엔드밀이 1분 안에 모든 날이 무너진 시험
/// (Kuczmaszewski 2017), Ti VB3 0.5 mm 를 250 m/min 에서 1분 (Matuszak 2023) 의 규모. 치핑 기본 위험은 치핑으로 끝나는 그룹
/// (고경도·Ni·Ti)이 수명의 1~1.5배에서 치핑 한계에 닿도록 둔 값
pub fn eol_params(g: WpGroup) -> EolParams {
    match g {
        WpGroup::Aluminum => EolParams { vb_accel_mm: 0.27, vb_collapse_mm: 0.60, accel_mult: 2.5, chip_base_p: 0.03, static_frac: 0.70, fatigue_frac: 0.45 },
        WpGroup::CarbonSteel => EolParams { vb_accel_mm: 0.30, vb_collapse_mm: 0.60, accel_mult: 4.0, chip_base_p: 0.06, static_frac: 0.70, fatigue_frac: 0.45 },
        WpGroup::AlloySteel => EolParams { vb_accel_mm: 0.25, vb_collapse_mm: 0.50, accel_mult: 5.0, chip_base_p: 0.08, static_frac: 0.70, fatigue_frac: 0.45 },
        WpGroup::Hardened => EolParams { vb_accel_mm: 0.10, vb_collapse_mm: 0.25, accel_mult: 7.0, chip_base_p: 0.35, static_frac: 0.60, fatigue_frac: 0.40 },
        WpGroup::Stainless => EolParams { vb_accel_mm: 0.18, vb_collapse_mm: 0.40, accel_mult: 5.0, chip_base_p: 0.12, static_frac: 0.70, fatigue_frac: 0.45 },
        WpGroup::Titanium => EolParams { vb_accel_mm: 0.15, vb_collapse_mm: 0.40, accel_mult: 6.0, chip_base_p: 0.20, static_frac: 0.65, fatigue_frac: 0.40 },
        WpGroup::Nickel => EolParams { vb_accel_mm: 0.14, vb_collapse_mm: 0.35, accel_mult: 7.0, chip_base_p: 0.25, static_frac: 0.65, fatigue_frac: 0.40 },
        WpGroup::Cfrp => EolParams { vb_accel_mm: 0.16, vb_collapse_mm: 0.35, accel_mult: 2.0, chip_base_p: 0.05, static_frac: 0.70, fatigue_frac: 0.45 },
    }
}

/// 3차 마모 배수 (균일 VB 가 가속 시작 VB 이하면 1, 붕괴 VB 에서 `accel_mult`)
pub fn accel_factor(p: &EolParams, vb_mm: f64) -> f64 {
    let span = (p.vb_collapse_mm - p.vb_accel_mm).max(1e-6);
    let x = ((vb_mm - p.vb_accel_mm) / span).max(0.0);
    (p.accel_mult.max(1.0).ln() * x).exp()
}

/// 공구 몸체 재료 강도
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ToolStrength {
    /// 항절력 (3점 굽힘 시편, MPa)
    pub trs_mpa: f64,
    /// 정적 유효 강도 = 항절력 × 이 값
    pub static_factor: f64,
    /// 10⁷ 회 회전 굽힘 피로 한도 = 항절력 × 이 값
    pub fatigue_factor: f64,
    /// S–N 곡선 지수 (N = 10⁷ · (σ_e/σ)^m) — 초경은 기울기가 평평하다 (n ≈ 20)
    pub sn_exponent: f64,
    /// 날끝 소성 변형이 시작되는 계면 온도 (°C)
    pub plastic_c: f64,
    pub label: String,
}

impl ToolStrength {
    pub fn static_mpa(&self) -> f64 {
        self.trs_mpa * self.static_factor
    }

    pub fn fatigue_mpa(&self) -> f64 {
        self.trs_mpa * self.fatigue_factor
    }

    /// 진폭 σ_a 의 회전 굽힘을 견디는 회전 수 (피로 한도 아래면 무한)
    pub fn cycles_to_failure(&self, sigma_a: f64) -> f64 {
        let se = self.fatigue_mpa().max(1.0);
        if !(sigma_a > se) {
            return f64::INFINITY;
        }
        1.0e7 * (se / sigma_a).powf(self.sn_exponent.max(1.0))
    }
}

/// 공구 몸체 강도. 솔리드 초경 엔드밀 모재 항절력: 서브마이크론 WC-10Co 3,500~4,000 MPa, 초미립 WC-10~12Co 4,000~4,400 MPa
/// (평균 4,220, CrystEngComm 2013; 등급표 CTS24Z ≈ 4,000 MPa · K_IC 11.3), HSS M2 ≈ 3,000~3,500, M42 ≈ 2,800~3,200 MPa.
/// CBN·PCD 는 날만 소결체이고 몸체는 초경이라 몸체 강도는 초경으로 본다. 정적·피로 비는 피삭재 그룹의 충격을 넣은 `EolParams` 값.
/// 소성 변형 온도: 초경 날끝은 큰 압축 응력에서 900~1,000 °C 이상이면 처진다 (Trent & Wright) — Co 가 많은 초미립은 낮은 쪽,
/// HSS 는 템퍼링 연화 온도
pub fn strength_of(tool: &ToolGeometry, p: &EolParams) -> ToolStrength {
    let mk = |trs: f64, plastic: f64, label: &str| ToolStrength {
        trs_mpa: trs,
        static_factor: p.static_frac,
        fatigue_factor: p.fatigue_frac,
        sn_exponent: 20.0,
        plastic_c: plastic,
        label: label.into(),
    };
    match tool.substrate.as_str() {
        "carbide_uf" => mk(4200.0, 1000.0, "초미립 초경 (WC-12Co)"),
        "hss" => ToolStrength { sn_exponent: 10.0, ..mk(3200.0, tool.substrate_max_c.max(500.0) + 50.0, "HSS (M2)") },
        "hss_co" => ToolStrength { sn_exponent: 10.0, ..mk(3000.0, tool.substrate_max_c.max(500.0) + 50.0, "코발트 HSS (M42)") },
        "cbn" => mk(3800.0, 1250.0, "CBN 날 · 초경 몸체"),
        "pcd" => mk(3800.0, 700.0, "PCD 날 · 초경 몸체 (다이아몬드 흑연화 700 °C)"),
        _ => mk(3800.0, 1050.0, "미립 초경 (WC-10Co)"),
    }
}

/// 공구 몸체 응력 (가장 큰 단면)
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct BodyStress {
    /// 최대 주응력 σ₁ = σ_b/2 + √(σ_b²/4 + τ²) (MPa, 응력 집중 포함) — 초경은 취성이라 주응력 기준
    pub sigma_mpa: f64,
    /// 굽힘 모멘트 (N·mm)
    pub moment_nmm: f64,
    /// 단면 직경 (mm)
    pub diameter_mm: f64,
    /// 0 날 끝(플루트 끝) · 1 넥 시작 · 2 홀더 면 샹크
    pub section: u8,
}

pub fn section_label(s: u8) -> &'static str {
    match s {
        0 => "날 끝 단면(플루트 끝)",
        1 => "넥 시작",
        _ => "홀더 면 샹크",
    }
}

/// 플루트가 끝나는 곳의 응력 집중 (플루트 런아웃 홈) — 확신도 낮음
pub const KT_FLUTE: f64 = 1.3;
/// 넥 → 샹크 전이 필렛
pub const KT_NECK: f64 = 1.2;

/// 공구 끝에서 ap 에 걸쳐 고르게 실린 합력 `force_n` 과 토크 `torque_nm` 이 만드는 몸체 응력.
/// 외팔보: 단면 x 의 모멘트 = F × (하중 중심 − x). 날 부분은 등가 직경 core_ratio·D (Kops & Vo 의 0.8D 계열, 휨 계산과 같은 값) 에
/// 플루트 끝 응력 집중을 곱한다 — 코어 직경(0.55~0.65D)만 보는 보수적 단면과 랜드 몫을 더한 단면의 중간으로, Ø6 2날을 2.5D 팔에서
/// 약 1 kN 에 부러뜨린다 (리서치 추산 26·D² N @ 4 GPa). 넥은 넥 직경, 홀더 면은 샹크 직경.
/// 초경은 취성이라 최대 주응력 σ₁ = σ_b/2 + √(σ_b²/4 + τ²) 가 가장 큰 단면을 돌려준다.
pub fn body_stress(tool: &ToolGeometry, force_n: f64, torque_nm: f64, ap_mm: f64) -> BodyStress {
    let f = force_n.max(0.0);
    let t_nmm = torque_nm.max(0.0) * 1000.0;
    let l = tool.stickout_mm.max(tool.loc_mm.min(tool.stickout_mm)).max(1e-3);
    let ap = ap_mm.clamp(0.0, tool.loc_mm.max(1e-3));
    let mut best = BodyStress::default();
    let mut eval = |arm_mm: f64, d_mm: f64, kt: f64, section: u8| {
        if !(d_mm > 1e-6) || !(arm_mm >= 0.0) {
            return;
        }
        let m = f * arm_mm;
        let w = PI * d_mm.powi(3) / 32.0;
        let sb = kt * m / w;
        let tau = 16.0 * t_nmm / (PI * d_mm.powi(3));
        let s = 0.5 * sb + (0.25 * sb * sb + tau * tau).sqrt();
        if s > best.sigma_mpa {
            best = BodyStress { sigma_mpa: s, moment_nmm: m, diameter_mm: d_mm, section };
        }
    };
    let d_flute = tool.core_ratio * tool.diameter_mm;
    let loc = tool.loc_mm.min(l);
    eval((loc - ap / 2.0).max(0.25 * ap), d_flute, KT_FLUTE, 0);
    if tool.has_neck() {
        let reach = tool.reach_mm.min(l);
        eval((reach - ap / 2.0).max(0.0), tool.neck_diameter_mm.max(0.5 * d_flute), KT_NECK, 1);
    }
    eval((l - ap / 2.0).max(0.0), tool.shank_mm.max(d_flute), 1.0, 2);
    best
}

/// 날 치핑 위험률 배수 (1 = 적합한 조건·안정 절삭의 기본 위험)
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct ChipFactors {
    pub chatter: f64,
    pub stress: f64,
    pub hardness: f64,
    pub thermal: f64,
    pub worn: f64,
    pub fit: f64,
}

impl ChipFactors {
    pub fn total(&self) -> f64 {
        (self.chatter * self.stress * self.hardness * self.thermal * self.worn * self.fit).clamp(0.0, 1.0e4)
    }
}

/// 채터가 치핑 위험을 키우는 최대 배수 (실측 FRF·완전 채터). 추정 FRF 는 깊은 불안정 예측의 신뢰도(`toolwear::frf_credibility`)를 곱한다
pub const CHATTER_CHIP_GAIN: f64 = 15.0;

/// 치핑 위험 배수.
/// 채터: 깊은 채터에서는 마모가 아니라 치핑·파괴로 수명이 끝난다 (공구 간 편차가 σ 0.4~0.6 의 파괴형으로 바뀜) →
/// 1 + 15 × FRF 신뢰도 × 기대 채터 강도 (`chatter_severity` 인자는 신뢰도까지 곱한 값).
/// 몸체 응력: 정적 유효 강도의 35 % 를 넘으면 날 미세 결손이 늘어난다 → exp(5·(σ/σ_s − 0.35)).
/// 경도: 45 HRC 를 넘는 피삭재의 단속 절삭 (고경도 정삭은 치핑이 주 수명 끝) → 1 + ((HRC − 45)/10)².
/// 열피로: 빗살 균열 손상 0.7 부터 → 1 + 4·(D − 0.7)/0.3. 한계 초과: 마모 랜드가 날을 약하게 하고 힘을 키운다 → 1 + 4·(u − 1).
/// `fit` 는 용도 적합도(`appfit`)의 치핑 배수. 각 배수의 크기는 방향이 확실하고 크기는 가정이다 (확신도 낮음)
pub fn chip_factors(chatter_severity: f64, stress_ratio: f64, wp: &WorkpieceProps, thermal_damage: f64, life_used: f64, fit_chip: f64) -> ChipFactors {
    let hrc = wp.hardness_hrc;
    ChipFactors {
        chatter: 1.0 + CHATTER_CHIP_GAIN * chatter_severity.clamp(0.0, 1.0),
        stress: (5.0 * (stress_ratio - 0.35).max(0.0)).exp(),
        hardness: 1.0 + ((hrc - 45.0) / 10.0).max(0.0).powi(2),
        thermal: 1.0 + 4.0 * ((thermal_damage - 0.7) / 0.3).max(0.0),
        worn: 1.0 + 4.0 * (life_used - 1.0).max(0.0),
        fit: fit_chip.max(1.0),
    }
}

/// 공구 수명 사건
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ToolLifeEvent {
    /// warn · life_end · tool_change · accelerated · chip_risk · chip_detected · load_alarm · failure · stop · start_worn
    pub kind: String,
    /// 이 작업에서 몇 번째 공구인가 (1부터)
    pub tool_no: u32,
    pub t_s: f64,
    pub t_cut_s: f64,
    pub seg: usize,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub vb_mm: f64,
    pub vb_max_mm: f64,
    pub life_used: f64,
    pub force_peak_n: f64,
    pub temp_c: f64,
    pub stress_mpa: f64,
    pub stress_ratio: f64,
    pub fatigue: f64,
    pub chip_p: f64,
    /// failure 의 원인 키: overload · fatigue · collapse · plastic · chipping
    #[serde(default)]
    pub cause: String,
    pub message: String,
}

/// 경로 중 교체되어 빠진 공구 (정책 '교체 후 계속')
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RetiredTool {
    pub tool_no: u32,
    pub vb_start_mm: f64,
    pub vb_band_mm: f64,
    pub vb_max_mm: f64,
    pub life_used: f64,
    pub cut_min: f64,
    pub removed_cm3: f64,
    pub thermal_damage: f64,
    pub fatigue: f64,
    pub chip_p: f64,
    pub growth_calib_mm: f64,
    pub growth_fixed_mm: f64,
    pub t_change_s: f64,
    pub t_cut_change_s: f64,
    pub seg: usize,
    pub field: WearField,
    pub reason: String,
    /// 교체한 이유의 키: life_end (수명 판정) · chipping (치핑 감지) · start_worn (시작 전 이미 수명 초과)
    #[serde(default)]
    pub cause: String,
}

/// 경로 시뮬레이션의 공구 수명 요약 (`LoadSimReport::tool_life`)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ToolLifeReport {
    pub policy: ToolLifePolicy,
    /// 수명 판정에 쓰는 수명 사용률 (기계 설정 `tool_life_end_fraction`, 1 = VB 한계)
    pub end_fraction: f64,
    pub vb_limit_mm: f64,
    pub local_limit_mm: f64,
    /// 마지막 상태: fresh · normal · warn · life_end · accelerated · failed · stopped
    pub state: String,
    pub events: Vec<ToolLifeEvent>,
    pub tools_used: u32,
    pub tool_changes: u32,
    pub change_time_s: f64,
    pub failed: bool,
    pub failure_cause: String,
    /// 경로를 끝까지 깎지 못했는가 (정지·파손)
    pub unfinished: bool,
    pub stopped_at_s: Option<f64>,
    pub stopped_seg: Option<usize>,
    /// 깎지 못한 이송(G01/G02/G03) 경로 길이와 전체 이송 경로 길이 (mm)
    pub uncut_feed_mm: f64,
    pub total_feed_mm: f64,
    pub life_used_end: f64,
    pub life_used_max: f64,
    pub peak_stress_mpa: f64,
    pub peak_stress_ratio: f64,
    pub critical_section: String,
    pub fatigue_damage: f64,
    pub chip_probability: f64,
    /// 시간 가중 평균 치핑 배수
    pub chip_factors_mean: ChipFactors,
    pub accel_factor_max: f64,
    pub strength: ToolStrength,
    pub params: EolParams,
    /// 마지막 공구의 절삭 시간·제거 체적 (공구 개체 기록용)
    pub last_tool_cut_min: f64,
    pub last_tool_removed_cm3: f64,
    pub retired: Vec<RetiredTool>,
    /// 공구 수명 관리로 이 작업 중 만료됨 (정책 '프로그램 끝까지') — 공구 개체는 작업 기록 뒤 은퇴, 다음 작업은 예비 공구
    #[serde(default)]
    pub expired: bool,
    /// 부하 감시가 보는 값: 같은 결합 조건의 새 날 대비 절삭 동력 비 (최대 · 수명 판정 시점) — 마모 경보는 학습 부하의 1.2~1.25배
    #[serde(default)]
    pub load_ratio_max: f64,
    #[serde(default)]
    pub load_ratio_at_life_end: Option<f64>,
    /// 부하가 처음 1.2배를 넘은 시각 (s) — 수명 판정보다 먼저면 부하 감시가 마모를 잡아낸다
    #[serde(default)]
    pub load_alarm_t_s: Option<f64>,
    /// 부하 감시가 치핑을 잡아 정지·교체한 횟수 (정지 · 교체 후 계속 정책에서 누적 치핑 확률이 `CHIP_FAILURE_P` 에 닿은 공구 수)
    #[serde(default)]
    pub chip_detected: u32,
    /// 모델이 정할 수 없어 가정으로 둔 것 (로그용)
    pub unknowns: Vec<String>,
}

impl ToolLifeReport {
    pub fn stop_reason(&self) -> Option<String> {
        if !self.unfinished {
            return None;
        }
        self.events.iter().rev().find(|e| e.kind == "failure" || e.kind == "stop" || e.kind == "start_worn").map(|e| e.message.clone())
    }

    /// 전체 이송 경로 중 깎은 비율 (0~1)
    pub fn completed_fraction(&self) -> f64 {
        if self.total_feed_mm > 1e-9 {
            (1.0 - self.uncut_feed_mm / self.total_feed_mm).clamp(0.0, 1.0)
        } else {
            1.0
        }
    }
}

/// 정책 설명 (로그·UI)
pub fn policy_label(p: ToolLifePolicy) -> &'static str {
    match p {
        ToolLifePolicy::Stop => "마모 경보 → 지금 절삭을 마치고 정지, 치핑 감지 → 즉시 정지 (작업자 교체)",
        ToolLifePolicy::ChangeTool => "마모 경보 → 지금 절삭을 마치고, 치핑 감지 → 즉시 예비 공구로 교체 후 계속",
        ToolLifePolicy::FinishPart => "공구 수명 관리(횟수·시간) → 프로그램 끝까지 가공, 다음 부품부터 예비 공구",
        ToolLifePolicy::RunToFailure => "감시 없음 — 파손까지 계속 (무엇이 일어나는지 보는 용도)",
    }
}

/// 이 정책은 수명 판정 뒤에도 이 작업을 끝까지 깎는가 (감시 없이 3차 마모·파손 판정만 남음)
pub fn continues_past_limit(p: ToolLifePolicy) -> bool {
    matches!(p, ToolLifePolicy::FinishPart | ToolLifePolicy::RunToFailure)
}

/// 공구 교체 뒤 벽 위치 단차 불확도 (µm, 1σ 근사) = √(재클램프 런아웃 변화² + 반경 측정²).
/// 재클램프: 긴 날이 벽을 만드는 과삭(0.85·e, e = TIR/2)이 클램프마다 달라진다 — 편심이 레일리 분포(산포 ≈ 평균의 50 %)인 두 클램프의 차 →
/// ≈ 0.3·TIR (열박음·유압 TIR 1.5~3 µm → 0.5~1 µm, ER 표준 6~12 µm → 2~4 µm; 홀더 제조사 반복도 자료).
/// 반경 측정: 회전 상태 레이저(런아웃 포락선까지 잼) 2σ 0.25~1 µm → 1σ 1 µm 로 두고 재클램프 항을 30 % 만 남긴다,
/// 정지 프리세터 ±2 µm + 편향 1~3 µm → 2.5 µm (Renishaw NC4·Blum LaserControl·Zoller 사양, 실측 정리)
pub fn change_step_um(tool: &ToolGeometry) -> f64 {
    let tir = tool.runout_um.max(0.0);
    let (reclamp, measure) = if tool.runout_compensated { (0.3 * 0.3 * tir, 1.0) } else { (0.3 * tir, 2.5) };
    (reclamp * reclamp + measure * measure).sqrt()
}

/// 이 모듈이 정할 수 없는 값 (모든 작업에 같이 남긴다)
pub fn standing_unknowns(tool: &ToolGeometry, s: &ToolStrength) -> Vec<String> {
    vec![
        format!(
            "공구 몸체 강도: {} 항절력 {:.0} MPa × 정적 {:.2} = {:.0} MPa · 피로 한도 ×{:.2} = {:.0} MPa (S–N 지수 {:.0}) 는 등급표·문헌 대표값 — 실제 공구의 등급·연삭 상태·Weibull 계수(10~20)는 모름",
            s.label,
            s.trs_mpa,
            s.static_factor,
            s.static_mpa(),
            s.fatigue_factor,
            s.fatigue_mpa(),
            s.sn_exponent
        ),
        format!(
            "날 부분 단면을 등가 직경 {:.2}·D 원으로 보고 플루트 끝 응력 집중 {:.1} 을 곱함 — 실제 플루트 단면(코어 직경·랜드 폭)은 공구마다 다름",
            tool.core_ratio, KT_FLUTE
        ),
        "치핑은 확률(누적 위험률)로만 남긴다 — 실제 한 공구가 언제 이가 빠질지는 정할 수 없음 (같은 조건 공구 10자루 이상의 수명 분포가 필요)".to_string(),
        "3차 마모 배수와 날끝 붕괴 수명 사용률은 피삭재 그룹 대표값 — 이 공구·조건에서 VB 한계를 넘긴 실측 마모 곡선이 없으면 확정 불가".to_string(),
    ]
}
