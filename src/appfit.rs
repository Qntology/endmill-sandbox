//! 공구 용도 적합도 (application fit) — 공구 형상·날끝·모재가 피삭재에 맞는가.
//!
//! 코팅–피삭재의 화학 궁합(확산·흑연화·응착)과 코팅 허용 온도는 `tribology::compatibility` 가 이미 본다. 여기서는 그와 겹치지 않게
//! 공구의 기계적 설계값을 피삭재 그룹별 권장 창(window)과 비교한다 (N:N 공구 × 피삭재 조합의 형상 몫):
//!  * 경사각 · 날끝 반경(호닝+코팅) · 여유각: 창에서 '날을 약하게 하는 쪽'(경사 큼 · 날 날카로움 · 여유 큼)으로 벗어난 거리 d_weak 와
//!    '무디게 하는 쪽'(경사 작음 · 반경 큼 · 여유 작음)으로 벗어난 거리 d_blunt 를 단계(경사 5° — Ti 2° · 반경 5 µm · 여유 3°) 단위로 잰다.
//!    큰 여유각은 쐐기각(90° − 경사 − 여유)이 창의 가장 얇은 쐐기보다 얇아진 만큼만 약함으로 센다 (경사가 작으면 상쇄). 작은 여유각이
//!    VB 를 빨리 키우는 몫은 마모 랜드 기하(`physics::flank_geometry_factor`)가 맡아 마모 배수에는 넣지 않는다 (구성인선·조도 배수에는 남김).
//!  * 피삭재 경도 계수 h (Al 0.1 · CFRP 0.2 · 탄소강 0.4 · 합금강·스테인리스 0.6 · Ti 0.7 · Ni 1.0 · 고경도 1.5)로
//!    마모 수명 배수 L = exp(−0.5·h·Σd_weak − 0.1·Σd_blunt) (0.02~1), 치핑 위험 배수 exp(1.5·h·Σd_weak),
//!    구성인선·응착 배수 1 + 0.5·β·Σd_blunt, 조도 배수 1 + β·Σd_blunt (β: Al 1.0 · CFRP 0.8 · 스테인리스 0.6 · Ti 0.4 · Ni·탄소강 0.3 · 그 밖 0.1).
//!  * 날 수: 측면 가공 창보다 많으면 칩 배출이 나빠지고(연한 소재일수록), 슬롯에서 창보다 많으면 칩 막힘 → 파손 위험 ∝ (Z − Z_max)·ap/D.
//!  * 헬릭스: 창 밖이면 마모·칩 배출이 조금 나빠지고, CFRP 에서 높은 헬릭스는 층간 박리(품질) 위험.
//!  * 모재: HSS 는 45 HRC 위에서 날이 무너짐(파손급), CBN 은 45 HRC 아래에서 이점이 없음(수명 ×0.3), PCD 는 철계·니켈계 금지(화학 항목).
//!  * 코팅 경도 상한: TiN·TiCN 45 · TiAlN 52 · AlCrN 60 · AlTiN 62 · TiSiN·nACo 70 HRC 를 넘는 피삭재는 마모 ×(1 + 0.15·ΔHRC).
//! 기준 사례(리서치): 38 HRC 에서 날카로운 날 대 30 µm 호닝 수명 약 8배, +8° 경사·날카로운 날로 56 HRC D2 는 첫 패스에서 파손,
//! Ti 에서 여유각 8° 는 10° 보다 치핑·플랭크 응착. 창과 계수는 공구 제조사 자료·문헌 정리(`docs/tool-life-and-pair-matrix.md`)이고
//! 배수의 크기는 확신도 낮음 — 방향과 순서를 맞추는 모델이다.
use crate::pairref::WpGroup;
use crate::physics::{ToolGeometry, WorkpieceProps};
use crate::profile::ToolNose;
use serde::{Deserialize, Serialize};

/// 치핑 위험 배수의 상한 — 판정 문턱(×8)의 열 배 남짓. 이보다 크면 형상이 피삭재 창에서 크게 벗어나 첫 진입에서 날이 깨지는 수준이라
/// 배수를 더 키워도 결과(바로 치핑)는 같고 로그의 숫자만 의미를 잃는다
pub const CHIP_FACTOR_MAX: f64 = 100.0;

/// 피삭재 그룹별 권장 창. (하한, 상한)
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct FitWindow {
    pub rake_deg: (f64, f64),
    pub clearance_deg: (f64, f64),
    /// 날끝 반경 (호닝 + 코팅 성장, µm)
    pub hone_um: (f64, f64),
    /// 측면 가공 권장 날 수 (하한, 상한)
    pub flutes_side: (u32, u32),
    /// 슬롯 권장 최대 날 수 (고경도는 슬롯 자체를 피함 → 0)
    pub flutes_slot_max: u32,
    pub helix_deg: (f64, f64),
    /// 경사각 한 단계 (°)
    pub rake_step: f64,
    /// 피삭재 경도 계수 h
    pub hardness: f64,
    /// 구성인선·응착 계수 β
    pub bue: f64,
}

/// 권장 창 (솔리드 초경 엔드밀 Ø6~12). 리서치 정리 표: Al 경사 +10~+20 · 반경 ≤5 · 2~3날 · 헬릭스 35~45 · 여유 10~15,
/// C45 +5~+12 · 10~25 · 4~6 · 30~45 · 6~10, 4140 +3~+10 · 15~30 · 4~7 · 35~45 · 6~9, 304 +6~+15 · 10~30(최적 12) · 4~7 · 35~45 · 8~12,
/// Ti +8~+14 · 10~25 · 4~7 · 35~45 · 10~12, IN718 +4~+12 · 15~40 · 5~7(슬롯 4) · 30~45 · 8~12,
/// 58~62 HRC −15~0 · 15~40 · 4~10(슬롯 피함) · 20~50 · 5~9(볼 8~12), CFRP +5~+30 · ≤10(다이아몬드 코팅 ≤17) · 4~8 · 0~20 · 10~22.
/// 반경 창의 경계 몇 곳은 코팅 성장(+0.8 × 코팅 두께)과 내장 공구 설계를 넣어 넓혔다: 알루미늄 ≤6.5 µm (코팅 공구 5.2 µm),
/// 스테인리스 ≥6 µm (316L 시험 범위 4~15 µm 의 최적 12), 고경도 ≥8 µm (소경 볼 정삭 공구의 미세 호닝 — 문헌 15 µm 는 무거운 절입 기준)
pub fn window(g: WpGroup) -> FitWindow {
    match g {
        WpGroup::Aluminum => FitWindow { rake_deg: (10.0, 20.0), clearance_deg: (10.0, 15.0), hone_um: (1.0, 6.5), flutes_side: (2, 3), flutes_slot_max: 3, helix_deg: (35.0, 45.0), rake_step: 5.0, hardness: 0.1, bue: 1.0 },
        WpGroup::CarbonSteel => FitWindow { rake_deg: (5.0, 12.0), clearance_deg: (6.0, 10.0), hone_um: (10.0, 25.0), flutes_side: (4, 6), flutes_slot_max: 4, helix_deg: (30.0, 45.0), rake_step: 5.0, hardness: 0.4, bue: 0.3 },
        WpGroup::AlloySteel => FitWindow { rake_deg: (3.0, 10.0), clearance_deg: (6.0, 9.0), hone_um: (12.0, 30.0), flutes_side: (4, 7), flutes_slot_max: 4, helix_deg: (35.0, 45.0), rake_step: 5.0, hardness: 0.6, bue: 0.1 },
        WpGroup::Hardened => FitWindow { rake_deg: (-15.0, 0.0), clearance_deg: (5.0, 9.0), hone_um: (8.0, 40.0), flutes_side: (4, 10), flutes_slot_max: 0, helix_deg: (20.0, 50.0), rake_step: 5.0, hardness: 1.5, bue: 0.1 },
        WpGroup::Stainless => FitWindow { rake_deg: (6.0, 15.0), clearance_deg: (8.0, 12.0), hone_um: (6.0, 30.0), flutes_side: (4, 7), flutes_slot_max: 4, helix_deg: (35.0, 45.0), rake_step: 5.0, hardness: 0.6, bue: 0.6 },
        WpGroup::Titanium => FitWindow { rake_deg: (8.0, 14.0), clearance_deg: (10.0, 12.0), hone_um: (10.0, 25.0), flutes_side: (4, 7), flutes_slot_max: 4, helix_deg: (35.0, 45.0), rake_step: 2.0, hardness: 0.7, bue: 0.4 },
        WpGroup::Nickel => FitWindow { rake_deg: (4.0, 12.0), clearance_deg: (8.0, 12.0), hone_um: (15.0, 40.0), flutes_side: (5, 7), flutes_slot_max: 4, helix_deg: (30.0, 45.0), rake_step: 5.0, hardness: 1.0, bue: 0.3 },
        WpGroup::Cfrp => FitWindow { rake_deg: (5.0, 30.0), clearance_deg: (10.0, 22.0), hone_um: (1.0, 17.0), flutes_side: (4, 8), flutes_slot_max: 4, helix_deg: (0.0, 20.0), rake_step: 5.0, hardness: 0.2, bue: 0.8 },
    }
}

/// 항목 하나의 판정
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FitItem {
    pub key: String,
    pub value: f64,
    pub lo: f64,
    pub hi: f64,
    /// 날을 약하게 하는 쪽 / 무디게 하는 쪽으로 벗어난 단계 수
    pub d_weak: f64,
    pub d_blunt: f64,
    pub message: String,
}

/// 용도 적합도
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppFit {
    pub wp_group: String,
    pub substrate: String,
    pub sum_weak: f64,
    pub sum_blunt: f64,
    /// 정상 마모율 배수 (≥ 1) = 1 / 마모 수명 배수 L
    pub wear_factor: f64,
    /// 치핑 위험 배수 (≥ 1)
    pub chip_factor: f64,
    /// 구성인선·응착 경향 배수 (≥ 1)
    pub bue_factor: f64,
    /// 측벽 조도 배수 (≥ 1)
    pub ra_factor: f64,
    /// 칩 막힘 위험 (0~1) — 측면 가공은 날 수 과다, 슬롯은 (Z − Z_max)·ap/D
    pub evac_risk: f64,
    /// 0~1 (1 = 모든 창 안)
    pub score: f64,
    /// 0 적합 · 1 주의 · 2 부적합
    pub verdict: u8,
    pub items: Vec<FitItem>,
    /// 배수에 넣지 않고 알려만 주는 사항
    #[serde(default)]
    pub notes: Vec<String>,
}

impl Default for AppFit {
    fn default() -> Self {
        Self {
            wp_group: String::new(),
            substrate: String::new(),
            sum_weak: 0.0,
            sum_blunt: 0.0,
            wear_factor: 1.0,
            chip_factor: 1.0,
            bue_factor: 1.0,
            ra_factor: 1.0,
            evac_risk: 0.0,
            score: 1.0,
            verdict: 0,
            items: Vec::new(),
            notes: Vec::new(),
        }
    }
}

impl AppFit {
    pub fn verdict_label(&self) -> &'static str {
        ["적합", "주의", "부적합"][self.verdict.min(2) as usize]
    }

    pub fn reasons(&self) -> Vec<String> {
        let mut v: Vec<String> = self.items.iter().filter(|i| i.d_weak > 0.05 || i.d_blunt > 0.05).map(|i| i.message.clone()).collect();
        v.extend(self.notes.iter().cloned());
        v
    }

    pub fn summary(&self) -> String {
        let reasons = self.reasons();
        format!(
            "용도 적합도 {} (점수 {:.2}) · 마모 ×{:.2} · 치핑 ×{:.1} · 구성인선 ×{:.2} · 조도 ×{:.2} · 칩 막힘 위험 {:.0}%{}",
            self.verdict_label(),
            self.score,
            self.wear_factor,
            self.chip_factor,
            self.bue_factor,
            self.ra_factor,
            self.evac_risk * 100.0,
            if reasons.is_empty() { String::new() } else { format!(" — {}", reasons.join(" · ")) }
        )
    }
}

/// 창 밖 거리를 단계 수로 (아래쪽, 위쪽)
fn steps_out(v: f64, lo: f64, hi: f64, step: f64) -> (f64, f64) {
    let s = step.max(1e-9);
    (((lo - v) / s).max(0.0), ((v - hi) / s).max(0.0))
}

/// 코팅 계열이 버티는 피삭재 경도 상한 (HRC, 제조사 자료). 무코팅 초경은 일반 형상 기준 50
pub fn coating_hrc_ceiling(family: &str) -> Option<f64> {
    match family {
        "TiN" | "TiCN" | "CrN" | "ZrN" => Some(45.0),
        "TiAlN" => Some(52.0),
        "AlCrN" => Some(60.0),
        "AlTiN" | "HiPIMS" => Some(62.0),
        "TiSiN" | "nACo" => Some(70.0),
        "uncoated" => Some(50.0),
        _ => None,
    }
}

/// 공구 하나 × 피삭재 하나의 용도 적합도. `slotting` 은 슬롯(전폭) 가공, `ap_over_d` 는 그때의 축 절입/직경 (칩 막힘 위험에만 쓴다)
pub fn evaluate(tool: &ToolGeometry, wp: &WorkpieceProps, slotting: bool) -> AppFit {
    evaluate_with(tool, wp, slotting, 1.0)
}

pub fn evaluate_with(tool: &ToolGeometry, wp: &WorkpieceProps, slotting: bool, ap_over_d: f64) -> AppFit {
    let g = WpGroup::of(wp);
    let w = window(g);
    let hard = wp.hardness_hrc;
    let ball = tool.nose == ToolNose::Ball;
    let mut items: Vec<FitItem> = Vec::new();
    let mut notes: Vec<String> = Vec::new();
    // ---- 경사각: 크면 날이 약함, 작으면 무딤 ----
    {
        let (below, above) = steps_out(tool.rake_deg, w.rake_deg.0, w.rake_deg.1, w.rake_step);
        items.push(FitItem {
            key: "rake".into(),
            value: tool.rake_deg,
            lo: w.rake_deg.0,
            hi: w.rake_deg.1,
            d_weak: above,
            d_blunt: below,
            message: if above > 0.0 {
                format!("경사각 {:.0}° > 권장 {:.0}° — 날이 약해 치핑", tool.rake_deg, w.rake_deg.1)
            } else if below > 0.0 {
                format!("경사각 {:.0}° < 권장 {:.0}° — 절삭력·발열{} 증가", tool.rake_deg, w.rake_deg.0, if w.bue >= 0.4 { "·구성인선" } else { "" })
            } else {
                format!("경사각 {:.0}° (권장 {:.0}~{:.0}°)", tool.rake_deg, w.rake_deg.0, w.rake_deg.1)
            },
        });
    }
    // ---- 날끝 반경: 작으면 날이 약함, 크면 무딤 ----
    {
        let r = tool.edge_radius_um;
        let (below, above) = steps_out(r, w.hone_um.0, w.hone_um.1, 5.0);
        items.push(FitItem {
            key: "hone".into(),
            value: r,
            lo: w.hone_um.0,
            hi: w.hone_um.1,
            d_weak: below,
            d_blunt: above,
            message: if below > 0.0 {
                format!("날끝 반경 {:.1} µm < 권장 {:.0} µm — 날카로운 날이 이 소재를 버티지 못해 치핑", r, w.hone_um.0)
            } else if above > 0.0 {
                format!("날끝 반경 {:.1} µm > 권장 {:.0} µm — 밀림·발열{}", r, w.hone_um.1, if w.bue >= 0.4 { "·구성인선" } else { "" })
            } else {
                format!("날끝 반경 {:.1} µm (권장 {:.0}~{:.0} µm)", r, w.hone_um.0, w.hone_um.1)
            },
        });
    }
    // ---- 여유각: 작으면 플랭크 마찰(무딤), 크면 쐐기가 얇아 약함. Ti 는 작은 여유각에서 응착으로 치핑까지 (8° vs 10°) ----
    // 날의 강도는 쐐기각 90° − γ − α 가 정한다 — 큰 여유각은 경사각이 창 상한보다 작은 만큼 상쇄된다 (경사 0° · 여유 15° 인서트의 쐐기 75° 는
    // 경사 8° · 여유 8° 엔드밀의 74° 와 같다. 예전에는 여유각만 따로 봐서 이런 인서트를 '쐐기가 얇아 치핑' 으로 판정했다).
    // 작은 여유각이 VB 를 빨리 키우는 몫은 마모 랜드 기하(`physics::flank_geometry_factor`)가 맡아 여기서는 구성인선·조도에만 넣는다
    {
        let (lo, hi) = if ball && g == WpGroup::Hardened { (8.0, 12.0) } else { w.clearance_deg };
        let (below, _) = steps_out(tool.clearance_deg, lo, hi, 3.0);
        let thin_wedge = ((tool.clearance_deg - hi) - (w.rake_deg.1 - tool.rake_deg).max(0.0)).max(0.0) / 3.0;
        let above = thin_wedge;
        let adhesion_chip = if matches!(g, WpGroup::Titanium) { 0.5 * below } else { 0.0 };
        items.push(FitItem {
            key: "clearance".into(),
            value: tool.clearance_deg,
            lo,
            hi,
            d_weak: above + adhesion_chip,
            d_blunt: below,
            message: if below > 0.0 {
                format!(
                    "여유각 {:.0}° < 권장 {:.0}° — 플랭크 마찰로 마모 증가{}",
                    tool.clearance_deg,
                    lo,
                    if adhesion_chip > 0.0 { " · Ti 플랭크 응착 → 치핑" } else { "" }
                )
            } else if above > 0.0 {
                format!("여유각 {:.0}° > 권장 {:.0}° — 쐐기각 {:.0}° 가 얇아 치핑", tool.clearance_deg, hi, 90.0 - tool.rake_deg - tool.clearance_deg)
            } else if tool.clearance_deg > hi {
                format!("여유각 {:.0}° > 권장 {:.0}° 이나 경사각 {:.0}° 라 쐐기각 {:.0}° 는 창 안", tool.clearance_deg, hi, tool.rake_deg, 90.0 - tool.rake_deg - tool.clearance_deg)
            } else {
                format!("여유각 {:.0}° (권장 {:.0}~{:.0}°)", tool.clearance_deg, lo, hi)
            },
        });
    }
    // ---- 헬릭스 (볼 노즈는 끝단 헬릭스가 달라 보지 않음) ----
    if !ball {
        let (below, above) = steps_out(tool.helix_deg, w.helix_deg.0, w.helix_deg.1, 10.0);
        if g == WpGroup::Cfrp && above > 0.0 {
            notes.push(format!("헬릭스 {:.0}° > CFRP 권장 {:.0}° — 섬유를 들어 올려 층간 박리·미절단 섬유 (품질 위험, 수치 자료 없음)", tool.helix_deg, w.helix_deg.1));
        }
        items.push(FitItem {
            key: "helix".into(),
            value: tool.helix_deg,
            lo: w.helix_deg.0,
            hi: w.helix_deg.1,
            d_weak: 0.0,
            d_blunt: 0.5 * (below + above),
            message: if below > 0.0 {
                format!("헬릭스 {:.0}° < 권장 {:.0}° — 칩 배출·충격 흡수 불리", tool.helix_deg, w.helix_deg.0)
            } else if above > 0.0 {
                format!("헬릭스 {:.0}° > 권장 {:.0}° — 축방향 힘 증가", tool.helix_deg, w.helix_deg.1)
            } else {
                format!("헬릭스 {:.0}° (권장 {:.0}~{:.0}°)", tool.helix_deg, w.helix_deg.0, w.helix_deg.1)
            },
        });
    }
    let sum_weak: f64 = items.iter().map(|i| i.d_weak).sum();
    let sum_blunt: f64 = items.iter().map(|i| i.d_blunt).sum();
    let sum_blunt_wear: f64 = items.iter().filter(|i| i.key != "clearance").map(|i| i.d_blunt).sum();
    let h = w.hardness;
    let mut wear = (0.5 * h * sum_weak + 0.1 * sum_blunt_wear).exp();
    let mut chip = (1.5 * h * sum_weak).exp();
    let bue = 1.0 + 0.5 * w.bue * sum_blunt;
    let ra = 1.0 + w.bue * sum_blunt;
    // ---- 날 수 (칩 배출) ----
    let z = tool.flutes.max(1);
    let soft = match g {
        WpGroup::Aluminum => 1.0,
        WpGroup::Stainless | WpGroup::Titanium => 0.6,
        WpGroup::CarbonSteel | WpGroup::AlloySteel | WpGroup::Nickel => 0.4,
        WpGroup::Hardened | WpGroup::Cfrp => 0.2,
    };
    let mut evac: f64 = 0.0;
    let side_over = z.saturating_sub(w.flutes_side.1) as f64;
    if side_over > 0.0 {
        let e = (1.0 - (-0.4 * side_over * soft).exp()).clamp(0.0, 1.0);
        evac = evac.max(e);
        wear *= 1.0 + 0.1 * side_over * soft;
        notes.push(format!("{}날 > 측면 가공 권장 {}날 — 칩 포켓이 작아 칩 배출 불리 (위험 {:.0}%)", z, w.flutes_side.1, e * 100.0));
    }
    if slotting {
        if w.flutes_slot_max == 0 {
            evac = evac.max(0.6);
            chip *= 3.0;
            notes.push(format!("{} 은 슬롯 가공을 피함 — 전폭 절입의 충격·발열로 치핑", wp.label));
        } else if z > w.flutes_slot_max {
            let over = (z - w.flutes_slot_max) as f64;
            let e = (1.0 - (-over * soft * ap_over_d.clamp(0.1, 3.0)).exp()).clamp(0.0, 1.0);
            evac = evac.max(e);
            chip *= 1.0 + 4.0 * e;
            notes.push(format!("슬롯에서 {}날 > 권장 {}날 — 칩 막힘·재절삭 → 파손 위험 {:.0}% (ap/D {:.1})", z, w.flutes_slot_max, e * 100.0, ap_over_d));
        }
    }
    // ---- 모재 ----
    match tool.substrate.as_str() {
        "hss" | "hss_co" if hard > 45.0 => {
            wear *= 10.0;
            chip *= 50.0;
            notes.push(format!("HSS 공구로 {:.0} HRC — 45 HRC 위에서 HSS 날은 버티지 못함 (파손급, 초경 이상 필요)", hard));
        }
        "hss" | "hss_co" if hard > 35.0 => {
            wear *= 1.0 + 0.2 * (hard - 35.0);
            notes.push(format!("HSS 공구로 {:.0} HRC — 경도 차가 작아 마모 ×{:.1}", hard, 1.0 + 0.2 * (hard - 35.0)));
        }
        "cbn" if hard < 45.0 => {
            wear /= 0.3;
            notes.push(format!("CBN 은 45 HRC 이상 경화강용 — {:.0} HRC 에서는 수명 ×0.3", hard));
        }
        "pcd" if !matches!(g, WpGroup::Aluminum | WpGroup::Cfrp) => {
            chip *= 2.0;
            notes.push(format!("PCD 는 비철·복합재용 — {} 에서는 탄소 확산(코팅 궁합)과 날 치핑", wp.label));
        }
        _ => {}
    }
    // ---- 코팅 경도 상한 ----
    if let Some(ceil) = coating_hrc_ceiling(tool.coating.family.as_str()) {
        if hard > ceil {
            let k = 1.0 + 0.15 * (hard - ceil);
            wear *= k;
            notes.push(format!("{} 코팅 권장 경도 상한 {:.0} HRC < 피삭재 {:.0} HRC — 마모 ×{:.2}", tool.coating.family, ceil, hard, k));
        }
    }
    let wear_factor = wear.clamp(1.0, 50.0);
    // 치핑 배수 상한: 이 정도면 형상이 피삭재 창에서 크게 벗어나 첫 진입에서 날이 깨지는 수준이다 (판정은 이미 '부적합').
    // 지수식을 그대로 두면 ×수천이 나와 (2026-10-18 앱 로그: CFRP 용 다이아몬드 공구로 58 HRC → ×3,294) 숫자가 의미를 잃는다
    if chip > CHIP_FACTOR_MAX {
        notes.push(format!(
            "치핑 배수 ×{:.0} → 상한 ×{:.0} 로 자름 — 형상이 이 피삭재 창에서 크게 벗어나 첫 진입에서 날이 깨질 수준",
            chip, CHIP_FACTOR_MAX
        ));
    }
    let chip_factor = chip.clamp(1.0, CHIP_FACTOR_MAX);
    let bue_factor = bue.clamp(1.0, 4.0);
    let ra_factor = ra.clamp(1.0, 6.0);
    let badness = wear_factor.ln() * 1.5 + chip_factor.ln() * 0.5 + bue_factor.ln() + 2.0 * evac;
    let score = (-badness).exp().clamp(0.0, 1.0);
    // 판정 문턱은 리서치 표의 등급과 맞춘다: 알루미늄용 날을 C45 에 → '주의'(수명 0.3~0.7배), +8° 날카로운 날로 58 HRC → '부적합',
    // 음의 경사 고경도 볼을 알루미늄에 → '부적합'(구성인선)
    let verdict = if wear_factor > 2.5 || chip_factor > 8.0 || evac > 0.6 || bue_factor > 2.5 || score < 0.15 {
        2
    } else if wear_factor > 1.15 || chip_factor > 1.8 || bue_factor > 1.3 || evac > 0.25 || score < 0.8 {
        1
    } else {
        0
    };
    AppFit {
        wp_group: g.key().into(),
        substrate: tool.substrate.clone(),
        sum_weak,
        sum_blunt,
        wear_factor,
        chip_factor,
        bue_factor,
        ra_factor,
        evac_risk: evac,
        score,
        verdict,
        items,
        notes,
    }
}
