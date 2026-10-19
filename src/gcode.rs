use serde::{Deserialize, Serialize};
use crate::profile::MachiningProfile;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GCodeLine {
    pub line_number: u32,
    pub code: String,
    pub comment: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GCodeProgram {
    pub program_name: String,
    pub header: Vec<GCodeLine>,
    pub body: Vec<GCodeLine>,
    pub footer: Vec<GCodeLine>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ToolPathPattern {
    /// 사각형 외곽 프로파일
    RectangularProfile,
    /// 포켓 가공 (내부 지그재그)
    PocketZigZag,
    /// 원형 프로파일 (보간)
    CircularProfile { center_x: f64, center_y: f64, radius: f64 },
    /// 헬리컬 진입 + 원형 포켓
    HelicalPocket { center_x: f64, center_y: f64, radius: f64, depth_per_rev: f64 },
    /// 등고선 (Contour) 다중 패스
    ContourMultiPass { offset_count: u8, step_over_mm: f64 },
    /// 슬롯 (직선 홈)
    Slot { start_x: f64, start_y: f64, end_x: f64, end_y: f64, width: f64 },
}

impl GCodeProgram {
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        out.push_str("%\n");
        out.push_str(&format!("O{} ({})\n", self.program_name, Self::comment_text(&self.program_name)));
        for line in &self.header {
            out.push_str(&Self::format_line(line));
        }
        for line in &self.body {
            out.push_str(&Self::format_line(line));
        }
        for line in &self.footer {
            out.push_str(&Self::format_line(line));
        }
        out.push_str("%\n");
        out
    }

    fn comment_text(c: &str) -> String {
        c.replace('(', "[").replace(')', "]").replace('%', "pct")
    }

    fn format_line(line: &GCodeLine) -> String {
        let base = format!("N{:04} {}", line.line_number, line.code);
        match &line.comment {
            Some(c) => format!("{} ({})\n", base, Self::comment_text(c)),
            None => format!("{}\n", base),
        }
    }

    pub fn line_count(&self) -> usize {
        self.header.len() + self.body.len() + self.footer.len()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ToolPathSegment {
    Linear { x: f64, y: f64, z: f64, feed: f64 },
    ArcCW { x: f64, y: f64, z: f64, i: f64, j: f64, feed: f64 },
    ArcCCW { x: f64, y: f64, z: f64, i: f64, j: f64, feed: f64 },
    Rapid { x: f64, y: f64, z: f64 },
    Plunge { x: f64, y: f64, z_start: f64, z_end: f64, feed: f64 },
}

impl ToolPathSegment {
    pub fn end_point(&self) -> (f64, f64, f64) {
        match self {
            Self::Linear { x, y, z, .. } | Self::ArcCW { x, y, z, .. } | Self::ArcCCW { x, y, z, .. } | Self::Rapid { x, y, z } => (*x, *y, *z),
            Self::Plunge { x, y, z_end, .. } => (*x, *y, *z_end),
        }
    }

    pub fn feed(&self) -> Option<f64> {
        match self {
            Self::Linear { feed, .. } | Self::ArcCW { feed, .. } | Self::ArcCCW { feed, .. } | Self::Plunge { feed, .. } => Some(*feed),
            Self::Rapid { .. } => None,
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Self::Linear { .. } => "Linear",
            Self::ArcCW { .. } => "ArcCW",
            Self::ArcCCW { .. } => "ArcCCW",
            Self::Rapid { .. } => "Rapid",
            Self::Plunge { .. } => "Plunge",
        }
    }

    pub fn arc_center(&self, start: (f64, f64, f64)) -> Option<(f64, f64, bool)> {
        match self {
            Self::ArcCW { i, j, .. } => Some((start.0 + i, start.1 + j, true)),
            Self::ArcCCW { i, j, .. } => Some((start.0 + i, start.1 + j, false)),
            _ => None,
        }
    }

    pub fn arc_sweep(&self, start: (f64, f64, f64)) -> Option<(f64, f64, f64, f64)> {
        let (cx, cy, cw) = self.arc_center(start)?;
        let (ex, ey, _) = self.end_point();
        let r = ((start.0 - cx).powi(2) + (start.1 - cy).powi(2)).sqrt();
        let a0 = (start.1 - cy).atan2(start.0 - cx);
        let a1 = (ey - cy).atan2(ex - cx);
        let two_pi = 2.0 * std::f64::consts::PI;
        let mut sweep = if cw { a0 - a1 } else { a1 - a0 };
        sweep = sweep.rem_euclid(two_pi);
        if sweep < 1e-9 {
            sweep = two_pi;
        }
        Some((cx, cy, r, if cw { -sweep } else { sweep }))
    }

    pub fn length_from(&self, start: (f64, f64, f64)) -> f64 {
        let (ex, ey, ez) = self.end_point();
        match self.arc_sweep(start) {
            Some((_, _, r, sweep)) => {
                let planar = r * sweep.abs();
                (planar * planar + (ez - start.2).powi(2)).sqrt()
            }
            None => ((ex - start.0).powi(2) + (ey - start.1).powi(2) + (ez - start.2).powi(2)).sqrt(),
        }
    }

    pub fn polyline_from(&self, start: (f64, f64, f64), max_step: f64) -> Vec<(f64, f64, f64)> {
        let end = self.end_point();
        let step = max_step.max(1e-3);
        match self.arc_sweep(start) {
            Some((cx, cy, r, sweep)) => {
                let a0 = (start.1 - cy).atan2(start.0 - cx);
                let n = ((r * sweep.abs()) / step).ceil().max(2.0) as usize;
                (1..=n)
                    .map(|k| {
                        let t = k as f64 / n as f64;
                        let a = a0 + sweep * t;
                        (cx + r * a.cos(), cy + r * a.sin(), start.2 + (end.2 - start.2) * t)
                    })
                    .collect()
            }
            None => {
                let len = ((end.0 - start.0).powi(2) + (end.1 - start.1).powi(2) + (end.2 - start.2).powi(2)).sqrt();
                let n = (len / step).ceil().max(1.0) as usize;
                (1..=n)
                    .map(|k| {
                        let t = k as f64 / n as f64;
                        (start.0 + (end.0 - start.0) * t, start.1 + (end.1 - start.1) * t, start.2 + (end.2 - start.2) * t)
                    })
                    .collect()
            }
        }
    }
}

impl ToolPathPattern {
    pub fn key(&self) -> &'static str {
        match self {
            Self::RectangularProfile => "rect_profile",
            Self::PocketZigZag => "pocket_zigzag",
            Self::CircularProfile { .. } => "circular",
            Self::HelicalPocket { .. } => "helical_pocket",
            Self::ContourMultiPass { .. } => "contour_multi",
            Self::Slot { .. } => "slot",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::RectangularProfile => "사각형 프로파일",
            Self::PocketZigZag => "포켓 지그재그",
            Self::CircularProfile { .. } => "원형 프로파일",
            Self::HelicalPocket { .. } => "헬리컬 포켓",
            Self::ContourMultiPass { .. } => "등고선 다중 패스",
            Self::Slot { .. } => "슬롯 가공",
        }
    }

    pub fn from_key(key: &str, profile: &MachiningProfile) -> Result<Self, String> {
        let w = profile.workpiece_setup.width_mm;
        let h = profile.workpiece_setup.height_mm;
        let d = profile.endmill_setting.diameter_mm;
        match key.trim() {
            "rect_profile" => Ok(Self::RectangularProfile),
            "pocket_zigzag" => Ok(Self::PocketZigZag),
            "circular" => Ok(Self::CircularProfile {
                center_x: w / 2.0,
                center_y: h / 2.0,
                radius: w.min(h) * 0.35,
            }),
            "helical_pocket" => Ok(Self::HelicalPocket {
                center_x: w / 2.0,
                center_y: h / 2.0,
                radius: w.min(h) * 0.3,
                depth_per_rev: 0.5,
            }),
            "contour_multi" => Ok(Self::ContourMultiPass {
                offset_count: 4,
                step_over_mm: pattern_stepover(profile, 0.1, 0.4),
            }),
            "slot" => Ok(Self::Slot {
                start_x: 10.0,
                start_y: h / 2.0,
                end_x: w - 10.0,
                end_y: h / 2.0,
                width: d,
            }),
            other => Err(format!("지원하지 않는 패턴: {}", other)),
        }
    }

    pub fn all_keys() -> [&'static str; 6] {
        ["pocket_zigzag", "rect_profile", "circular", "helical_pocket", "contour_multi", "slot"]
    }
}

/// 합성 경로가 프리셋 반경 절입(ae)과 크게 다른 피치로 깎을 때 알리는 문구.
/// 포켓 지그재그는 피치를 0.2~0.6·D, 등고선 다중 패스는 0.1~0.4·D 로 묶으므로, 정삭 조건(ae 0.03~0.1·D)도
/// 포켓 황삭처럼 넓게 물려 1차 마모·휨·채터가 프리셋 조건보다 크게 나온다 (기본 정삭 프리셋에서 공칭 대비 마모 7~9배).
pub fn stepover_note(profile: &MachiningProfile, pattern: &ToolPathPattern) -> Option<String> {
    let (lo, hi) = match pattern {
        ToolPathPattern::PocketZigZag => (0.2, 0.6),
        ToolPathPattern::ContourMultiPass { .. } => (0.1, 0.4),
        _ => return None,
    };
    let ae = profile.conditions.radial_doc_mm;
    if !(ae.is_finite() && ae > 0.0) {
        return None;
    }
    let step = pattern_stepover(profile, lo, hi);
    let d = profile.endmill_setting.diameter_mm.max(0.1);
    if step > 1.5 * ae {
        Some(format!(
            "{} 패턴은 피치를 {:.0}~{:.0}% D 로 묶어 {:.2} mm 피치로 깎습니다 — 프리셋 반경 절입 {:.2} mm 의 {:.1}배라 1차 마모·휨·채터가 프리셋 조건보다 크게 나옵니다{}",
            pattern.label(),
            lo * 100.0,
            hi * 100.0,
            step,
            ae,
            step / ae,
            if ae <= 0.12 * d { " (정삭 조건은 사각형 프로파일 패턴으로 평가하는 편이 맞습니다)" } else { "" }
        ))
    } else if step < ae / 1.5 {
        Some(format!("{} 패턴은 피치를 {:.0}% D 이하로 묶어 프리셋 반경 절입 {:.2} mm 보다 좁은 {:.2} mm 피치로 깎습니다", pattern.label(), hi * 100.0, ae, step))
    } else {
        None
    }
}

/// 합성 경로의 슬롯형 이동이 프리셋의 측면 절삭 ap 를 그대로 쓸 때 알리는 문구 (슬롯 패턴). 측면 절삭용 ap(인코넬 프리셋 1·D)는 제조사의
/// 슬롯 권장(`WorkpieceProps::slot_ap_ratio`, 확인된 소재만)보다 깊고, 작은 ae 용 칩 얇아짐 보정이 든 프리셋 fz 는 슬롯에서 그대로 칩 두께가 된다 —
/// 2026-10-19 인코넬 포켓의 최종 벽 오차 약 1 mm 중 공구 휨 0.5 mm 가 예전 지그재그의 피치 이동(법선 힘 약 7 kN, 몸체 응력이 정적 강도에 닿음)에서
/// 나왔다. 포켓 지그재그는 이제 슬롯 ap 이하로 단계 절입하고 슬롯형 이동의 이송을 낮추므로(`pocket_plan`) 단계 깊이가 상한을 넘을 때만 알린다
pub fn slot_note(profile: &MachiningProfile, pattern: &ToolPathPattern) -> Option<String> {
    let wp = crate::physics::WorkpieceProps::of(&profile.workpiece_setup.effective_material());
    let lim = wp.slot_ap_ratio?;
    let d = profile.endmill_setting.diameter_mm.max(0.1);
    let (moves, ap) = match pattern {
        ToolPathPattern::PocketZigZag => ("첫 줄 슬롯·줄 끝 피치 이동", pocket_plan(profile).level_ap_mm),
        ToolPathPattern::Slot { .. } => ("슬롯", profile.conditions.axial_doc_mm),
        _ => return None,
    };
    let ap = ap.min(profile.endmill_setting.loc_mm);
    if !(ap / d > lim + 1e-9) {
        return None;
    }
    let fz = profile.conditions.feed_per_tooth_mm;
    let target = (wp.fz_coeff * d).clamp(0.003, 0.25);
    Some(format!(
        "{} 패턴의 슬롯형 이동({})은 ap {:.1} mm = {:.2}·D 로 깎습니다 — {} 초경 엔드밀 슬롯 권장 ap 는 제조사 자료 {:.2}·D 이하라 공구 응력·휨·벽 오차가 현장 조건보다 크게 나옵니다{} · 현장에서는 헬리컬·램핑 진입(깊이 0.5·D 이하), 단계 절입, 정삭 윤곽 패스로 포켓 벽 치수를 정합니다",
        pattern.label(),
        moves,
        ap,
        ap / d,
        wp.label,
        lim,
        if fz > 1.2 * target {
            format!(" (슬롯에서는 칩 얇아짐이 없어 칩 두께 = 프로그램 fz {:.3} mm, 목표 칩 두께 {:.3} mm 의 {:.1}배)", fz, target, fz / target)
        } else {
            String::new()
        }
    ))
}

pub fn pattern_stepover(profile: &MachiningProfile, min_frac: f64, max_frac: f64) -> f64 {
    let d = profile.endmill_setting.diameter_mm.max(0.1);
    let ae = profile.conditions.radial_doc_mm;
    if ae.is_finite() && ae > 0.0 {
        ae.clamp(min_frac * d, max_frac * d)
    } else {
        max_frac * d
    }
}

/// 슬롯·슬롯형 이동의 fz (mm/날) — 소재의 목표 칩 두께 (`cutting::recommend_conditions` 가 작은 ae 의 칩 얇아짐 배수 1/sin φs 를 곱하기 전 값:
/// fz_coeff·D, 고급 공구가 아니면 ×0.8), 프로그램 fz 를 넘지 않게. 슬롯(ae = D)은 칩 얇아짐이 없어 측면 절삭용 fz 가 그대로 칩 두께가 된다 —
/// 인코넬 프리셋 fz 0.060 은 ae 0.1·D 용 ×1.67 이 든 값이라 슬롯에서는 목표 0.036 의 1.7배였다. 프로파일 ae 의 배수로 나누지 않는 것은
/// 정삭 프리셋이 ae 만 줄이고 fz 는 그대로 두기 때문 (스테인리스 정삭: fz 는 ae 0.25·D 의 ×1.15, ae 는 0.1·D 로 줄임)
pub fn slot_fz_mm(profile: &MachiningProfile) -> f64 {
    let wp = crate::physics::WorkpieceProps::of(&profile.workpiece_setup.effective_material());
    let d = profile.endmill_setting.diameter_mm.max(0.1);
    let target = (wp.fz_coeff * d).clamp(0.003, 0.25) * if profile.endmill_setting.is_high_end { 1.0 } else { 0.8 };
    let fz = profile.conditions.feed_per_tooth_mm;
    if fz.is_finite() && fz > 0.0 {
        target.min(fz)
    } else {
        target
    }
}

/// 안쪽 원호의 공구 중심 이송 — 공구 중심이 반경 ρ 의 원호를 돌고 벽(소재)이 원호 바깥에 있으면 날 끝이 벽에서 중심보다 (ρ + r)/ρ 배 빨리 가므로,
/// 날 끝 이송을 `edge_feed` 로 두려면 중심 이송을 ρ/(ρ + r) 배로 줄인다 (Kennametal 카탈로그 M443 'F2 = F1·(D − d1)/D', Tungaloy 'tool-center command feed
/// = uncorrected feed × (Dh − Dc)/Dh' — 헬리컬 보간·안쪽 원호 이송 보정, CAM·컨트롤러의 안쪽 모서리 감속과 같은 식). 보정하지 않으면 반경 0.5 mm 모서리
/// 원호에서 Ø6 공구의 날 끝 이송이 7배가 된다 (`loadsim` 이 원호 측면 절삭에 곱하는 배수)
pub fn inner_arc_feed(edge_feed: f64, rho_mm: f64, tool_r_mm: f64) -> f64 {
    if rho_mm > 1e-9 {
        edge_feed * rho_mm / (rho_mm + tool_r_mm.max(0.0))
    } else {
        edge_feed
    }
}

/// 헬리컬 진입 구멍 지름 / 공구 지름 — Kennametal 헬리컬 예제 (Ø16 에 구멍 24 mm), 허용 범위 1.15~1.9·D (HARVI I TE: 최소 1.1·D + 2·Re,
/// 최대 2·D − 2·Re), Seco 권장 1.2~1.3·Dc, Harvey 1.1~1.2·D 초과, Sandvik·Walter 2·D 까지
pub const HELIX_BORE_RATIO: f64 = 1.5;
/// 헬리컬 진입 구멍 지름의 하한 (공구 지름 배수, Seco Ø8~12 권장 1.2·Dc) — 이보다 좁은 포켓은 직선 램프로 들어간다
pub const HELIX_BORE_MIN_RATIO: f64 = 1.2;
/// 직선 램프 한 번에 내려가는 깊이 상한 (D 배수) — IMCO 'Max Ramp Depth 50 % of D' (대부분의 계열; 알루미늄 Streaker 는 100 %).
/// 헬리컬은 제조사가 진입 깊이를 따로 묶지 않는다 (IMCO: 원하는 깊이까지 나선으로 내려간 뒤 그 깊이에서 넓힘) — 단계 깊이(슬롯 ap)만 따른다
pub const RAMP_DEPTH_MAX_RATIO: f64 = 0.5;
/// 진입 시작 높이: 그 단계의 바닥(첫 단계는 소재 윗면)보다 이만큼 위에서 헬리컬·램프를 시작한다 (CAM 의 '이송 높이', 공중 이송)
pub const ENTRY_CLEAR_MM: f64 = 0.5;
/// 헬리컬 경사각 자료가 없는 소재(CFRP)에 쓰는 각 (°) — 확인한 소재 중 가장 작은 값 (Seco 고경도·탄소강 회전당 2 % Dc → 1.2°)
pub const ENTRY_RAMP_DEG_UNKNOWN: f64 = 1.2;
/// 포켓 벽 높이/두께 상한 — RapidDirect '최적 H/t 3:1', Protolabs 'wall width-to-height ratio of 3:1' (얇은 벽이 아닌 보통 벽).
/// 더 느슨한 규칙(SendCutSend t ≥ H/8 · Boeing 정삭 ap ≤ 8·t · Xometry 10:1 · Sandvik 15:1 미만 '작음')도 모두 만족한다
pub const WALL_HEIGHT_PER_THICKNESS: f64 = 3.0;
/// 금속 벽 최소 두께 권장 (mm) — Protolabs Network(Hubs) 0.8 권장 · 0.5 가능, RapidDirect 0.8, Xometry 0.79
pub const WALL_MIN_MM: f64 = 0.8;
/// 포켓 안쪽 모서리 반경 / 공구 반경 — Sandvik 정삭 '공구 지름 ≤ 1.5 × 모서리 반경' (R ≥ 1.33·r), RapidDirect·Mechanical-Engineering.com '공구 반경보다
/// 30 % 크게', Xometry '10~20 % 크게' 중 가장 엄한 값. 반경이 공구 반경과 같으면 정삭 공구가 모서리에서 약 137° 를 물어(인코넬 프리셋 공구 휨
/// 449 µm · Ti 159 µm) 최종 벽 최대 오차가 모서리 과부하로 정해졌다
pub const CORNER_RADIUS_PER_TOOL_RADIUS: f64 = 4.0 / 3.0;

/// 합성 포켓 지그재그의 계획 — CAM 포켓 황삭처럼 헬리컬 진입 · 슬롯 ap 이하로 단계 절입 · 슬롯형 이동은 칩 얇아짐 보정을 뺀 이송 ·
/// 벽에 정삭 여유를 남기고 마지막에 정삭 윤곽 패스(전체 깊이 · 하향 · 원호 진입/이탈)로 벽 치수를 정한다.
/// 예전 경로(직선 플런지로 ap 전체, 첫 패스 슬롯·패스 끝 피치 이동을 측면 절삭 ap 와 칩 얇아짐 보정 fz 로, 정삭 없이 황삭 벽이 최종 벽,
/// 스톡 가장자리 1 mm 스킨)는 인코넬 프리셋에서 공구 휨 0.5 mm · 몸체 응력이 정적 강도에 닿았다 (`docs/pocket-wall-error.md`)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PocketPlan {
    /// 포켓 깊이 = 프리셋 ap
    pub depth_mm: f64,
    /// 단계 수 · 단계 깊이 (모든 단계 같은 깊이)
    pub levels: usize,
    pub level_ap_mm: f64,
    /// 슬롯형 이동(첫 줄 슬롯 · 줄 끝 피치 이동)의 ap 상한 — 제조사 슬롯 자료가 있는 소재만 (`WorkpieceProps::slot_ap_ratio`)
    pub slot_ap_max_mm: Option<f64>,
    /// 스톡 가장자리에 남기는 포켓 벽 두께 (`pocket_wall_mm`)
    pub wall_mm: f64,
    /// 벽 정삭 여유 (황삭이 남기고 정삭 윤곽이 깎는 반경 두께 = 스톡 여유, D/2 이하) — 0 이면 정삭 윤곽 없이 황삭 벽이 최종 벽
    pub finish_allow_mm: f64,
    /// 황삭 공구 중심 사각형 (x0, y0, x1, y1) · 정삭 공구 중심 사각형
    pub rough_rect: (f64, f64, f64, f64),
    pub finish_rect: (f64, f64, f64, f64),
    /// 지그재그 피치 (줄 간격을 고르게 — 상한은 `pattern_stepover` 0.2~0.6·D) · 줄 수
    pub stepover_mm: f64,
    pub rows: usize,
    /// 진입: "helix" · "ramp" (포켓이 좁아 헬리컬 구멍이 안 들어감) · "plunge" (램프할 길이도 없음)
    pub entry: String,
    /// 헬리컬 진입 구멍 중심 · 지름 · 공구 중심 원 반경 · 회전당 하강 · 경사각 (공구 중심 경로 기준)
    pub helix_center: (f64, f64),
    pub helix_bore_mm: f64,
    pub helix_radius_mm: f64,
    pub helix_pitch_mm: f64,
    pub ramp_deg: f64,
    /// 헬리컬 진입과 정삭 윤곽을 상향(업)밀링으로 (소재가 그렇게 지시할 때만 — CFRP), 아니면 하향
    pub up_milling: bool,
    /// 이송 (mm/min, 날 끝 기준): 측면 절삭 = 프리셋, 슬롯형 이동 = 프리셋 × slot_feed_ratio, 진입 = 슬롯 이송, 정삭 윤곽 = 프리셋
    pub feed_side: f64,
    pub feed_slot: f64,
    pub feed_entry: f64,
    pub feed_finish: f64,
    /// 안쪽 원호의 공구 중심 이송 (`inner_arc_feed`): 헬리컬 진입 (날 끝 = 진입 이송) · 황삭 모서리 원호 (날 끝 = 슬롯 이송) ·
    /// 정삭 원호 진입 (날 끝 = 정삭 이송의 절반 — Harvey 'roll-in' 50 % 감속) · 원호 이탈 (날 끝 = 정삭 이송)
    pub feed_helix: f64,
    pub feed_rough_arc: f64,
    pub feed_lead_in: f64,
    pub feed_lead_out: f64,
    /// 슬롯형 이동 fz / 프로그램 fz = `slot_fz_mm` / fz — 프리셋 fz 에 든 작은 ae 용 칩 얇아짐 배수를 뺀다
    pub slot_feed_ratio: f64,
    /// 정삭 윤곽 원호 진입·이탈 반경
    pub lead_radius_mm: f64,
    /// 포켓 안쪽 모서리 반경 (벽) · 정삭 공구 중심이 모서리에서 도는 원호 반경 (= 모서리 반경 − r) · 황삭 경계 모서리 원호 반경 (= 정삭 원호 − 정삭 여유,
    /// 같은 중심 — 모서리에도 정삭 여유가 고르게 남는다; 0 이면 모서리가 각진 황삭)
    pub corner_radius_mm: f64,
    pub finish_corner_mm: f64,
    pub rough_corner_mm: f64,
    /// 정삭 모서리 원호의 공구 중심 이송 — 날 끝이 벽에서 직선 구간과 같은 이송으로 가게 F·rc/(rc + r) (Kennametal M443 'F2 = F1·(D − d1)/D',
    /// Tungaloy 'tool-center feed = feed × (Dh − Dc)/Dh' — 안쪽 원호 이송 보정)
    pub feed_finish_arc: f64,
    /// 근거 · 모르는 값 (감사의 `[UNKNOWN] pocket_path_strategy` 로 나간다)
    pub notes: Vec<String>,
}

/// 스톡 가장자리에 남기는 포켓 벽 두께 (mm): 벽 높이(포켓 깊이, 판이 더 얇으면 판 두께)의 1/3, 금속 최소 권장 0.8 mm 이상.
/// 예전의 1 mm 스킨(인코넬 12 mm 깊이면 H/t 12)은 얇은 벽 규칙을 모두 어겼고, 정삭 윤곽이 벽을 실제로 깎자 스킨이 휘고(Ti 209 µm · 탄소강 73 µm)
/// 뿌리가 항복했다 (탄소강 σy 의 6.5배) — 기본 작업이 얇은 벽 가공이 되지 않게 보통 벽으로 둔다
pub fn pocket_wall_mm(profile: &MachiningProfile) -> f64 {
    let depth = profile.conditions.axial_doc_mm.max(0.0);
    let height = depth.min(profile.workpiece_setup.thickness_mm.max(0.0));
    (height / WALL_HEIGHT_PER_THICKNESS).max(WALL_MIN_MM)
}

/// 내장 프리셋·사용자 프로파일의 포켓 지그재그 계획 (경로 생성·감사·시험이 같은 값을 쓴다)
pub fn pocket_plan(profile: &MachiningProfile) -> PocketPlan {
    let wp = crate::physics::WorkpieceProps::of(&profile.workpiece_setup.effective_material());
    let d = profile.endmill_setting.diameter_mm.max(0.1);
    let r = d / 2.0;
    let w = profile.workpiece_setup.width_mm;
    let h = profile.workpiece_setup.height_mm;
    let depth = profile.conditions.axial_doc_mm.max(0.0);
    let mut notes: Vec<String> = Vec::new();

    // ---- 벽 두께 · 정삭 여유 · 공구 중심 사각형 ----
    let wall = pocket_wall_mm(profile);
    let allow = profile.workpiece_setup.stock_allowance_mm.max(0.0).min(r);
    let allow = if allow < 0.01 { 0.0 } else { allow };
    let span = |lo: f64, hi: f64| if hi >= lo { (lo, hi) } else { (0.5 * (lo + hi), 0.5 * (lo + hi)) };
    let (fx0, fx1) = span(wall + r, w - wall - r);
    let (fy0, fy1) = span(wall + r, h - wall - r);
    let (rx0, rx1) = span(fx0 + allow, fx1 - allow);
    let (ry0, ry1) = span(fy0 + allow, fy1 - allow);

    // ---- 모서리: 벽 모서리 반경 4/3·r → 정삭 공구 중심 원호 r/3, 황삭 경계는 같은 중심에서 정삭 여유만큼 작은 원호 (좁은 포켓은 반폭까지) ----
    let finish_corner = ((CORNER_RADIUS_PER_TOOL_RADIUS - 1.0) * r).min(0.5 * (fx1 - fx0).min(fy1 - fy0)).max(0.0);
    let rough_corner = (finish_corner - allow).min(0.5 * (rx1 - rx0).min(ry1 - ry0)).max(0.0);

    // ---- 지그재그 줄: 피치 상한은 예전과 같은 0.2~0.6·D, 줄 간격을 고르게 해 마지막 줄이 황삭 경계에 닿게 ----
    let step_max = pattern_stepover(profile, 0.2, 0.6);
    let rows = if ry1 - ry0 > 1e-6 { ((ry1 - ry0) / step_max - 1e-9).ceil().max(1.0) as usize + 1 } else { 1 };
    let step = if rows > 1 { (ry1 - ry0) / (rows - 1) as f64 } else { 0.0 };

    // ---- 진입: 헬리컬 구멍이 들어가면 헬리컬, 아니면 첫 줄 위 직선 램프, 그 길이도 없으면 플런지 ----
    let ramp_deg = wp.ramp_deg.unwrap_or(ENTRY_RAMP_DEG_UNKNOWN);
    let room = 0.5 * (rx1 - rx0).min(ry1 - ry0);
    let rh = ((HELIX_BORE_RATIO - 1.0) * r).min(room);
    let (entry, helix_center, helix_radius, helix_pitch) = if rh >= (HELIX_BORE_MIN_RATIO - 1.0) * r - 1e-9 {
        // 회전당 하강 = 공구 중심 원 둘레 × tan α (제조사 경사각은 공구 중심 경로 기준: Tungaloy·Kennametal·Walter).
        // 구멍 중심은 황삭 모서리 원호 중심보다 안쪽 — 헬리컬 원(반경 rh ≥ 모서리 원호)이 둥근 황삭 경계 안에 든다
        let c = rh.max(rough_corner).min(room);
        ("helix", (rx0 + c, ry0 + c), rh, 2.0 * std::f64::consts::PI * rh * ramp_deg.to_radians().tan())
    } else if rx1 - rx0 >= d {
        ("ramp", (rx0, ry0), 0.0, 0.0)
    } else {
        notes.push("포켓이 좁아 헬리컬 구멍(1.2·D)도 램프 길이(D)도 없어 직선 플런지로 진입 — 제조사들이 권하지 않는 진입 (Harvey · SGS)".into());
        ("plunge", (rx0, ry0), 0.0, 0.0)
    };

    // ---- 단계 절입: 슬롯형 이동(첫 줄 · 줄 끝 피치 이동)이 슬롯 ap 를 넘지 않게, 직선 램프면 램프 깊이 상한도 ----
    let slot_ap_max = wp.slot_ap_ratio.map(|k| k * d);
    let mut cap = slot_ap_max.unwrap_or(f64::INFINITY);
    if entry == "ramp" {
        cap = cap.min(RAMP_DEPTH_MAX_RATIO * d);
    }
    let levels = if cap.is_finite() && cap > 1e-6 && depth > 1e-9 { ((depth / cap) - 1e-9).ceil().max(1.0) as usize } else { 1 };
    let level_ap = depth / levels as f64;

    // ---- 이송 ----
    // 원호 진입 반경은 2차 보정 경로와 같이 공구 반경(최대 4 mm) — 시작점이 황삭으로 비운 곳에 있도록 정삭 여유보다 크게
    let lead_radius = r.min(4.0).max(1.2 * allow);
    let feed_side = profile.conditions.feed_rate_mm_min;
    let fz = profile.conditions.feed_per_tooth_mm;
    let slot_feed_ratio = if fz.is_finite() && fz > 0.0 { slot_fz_mm(profile) / fz } else { 1.0 };
    let feed_slot = feed_side * slot_feed_ratio;

    // ---- 근거가 갈리거나 없는 값 (감사의 [UNKNOWN] 으로) ----
    match slot_ap_max {
        None => notes.push(format!("{} 의 초경 엔드밀 슬롯 권장 ap 자료가 없어 단계를 나누지 않음 — 슬롯형 이동이 ap {:.1} mm 전체로 깎음", wp.label, depth)),
        Some(_) if profile.endmill_setting.nose == crate::profile::ToolNose::Ball => notes.push(format!(
            "볼 엔드밀이 폭 전체로 깎는 ap 는 제조사 자료가 없어(고경도 볼은 윤곽 가공 자료뿐 — NS Tool·Harvey) 스퀘어·코너 R 공구의 {} 슬롯 ap {:.2}·D 로 단계를 나눔",
            wp.label,
            wp.slot_ap_ratio.unwrap_or(0.0)
        )),
        _ => {}
    }
    if wp.ramp_deg.is_none() {
        notes.push(format!("{} 의 헬리컬 경사각·구멍 지름 자료가 없어(직선 램프 5° 만 — SGS 복합재 라우터) 확인된 소재 중 가장 작은 {:.1}° 를 씀", wp.label, ENTRY_RAMP_DEG_UNKNOWN));
    }
    notes.push("헬리컬 진입은 날 끝 이송 = 슬롯 이송 (IMCO 헬리컬 '슬롯 속도·이송', SGS 경사 1~2° 는 슬롯 이송), 공구 중심 이송은 ×(구멍 − D)/구멍 — Seco 는 날 끝 이송을 스테인리스·Ti·내열합금 35~40 %, 고경도 볼 20 % 로, Kennametal 은 구멍/D 1.35~1.6 에서 −10 % 로 더 줄임".into());
    notes.push(format!(
        "벽 정삭 여유 = 스톡 여유 {:.2} mm ({:.1} % D) — Seco 측면 정삭 ae 2~4 % D (소재별), Haas ≤ 2 % D, Helical 4~6 % D, CAM 예제 0.3~0.5 mm",
        allow,
        allow / d * 100.0
    ));
    notes.push(format!("포켓 벽 두께 {:.2} mm = 벽 높이의 1/3 (RapidDirect·Protolabs 3:1) · 안쪽 모서리 반경 {:.2} mm = 공구 반경의 4/3 (Sandvik 정삭 D ≤ 1.5·R)", wall, r + finish_corner));
    PocketPlan {
        depth_mm: depth,
        levels,
        level_ap_mm: level_ap,
        slot_ap_max_mm: slot_ap_max,
        wall_mm: wall,
        finish_allow_mm: allow,
        rough_rect: (rx0, ry0, rx1, ry1),
        finish_rect: (fx0, fy0, fx1, fy1),
        stepover_mm: step,
        rows,
        entry: entry.into(),
        helix_center,
        helix_bore_mm: if entry == "helix" { d + 2.0 * helix_radius } else { 0.0 },
        helix_radius_mm: helix_radius,
        helix_pitch_mm: helix_pitch,
        ramp_deg,
        up_milling: wp.finish_up_milling,
        feed_side,
        feed_slot,
        feed_entry: feed_slot,
        feed_finish: feed_side,
        feed_helix: inner_arc_feed(feed_slot, helix_radius, r),
        feed_rough_arc: inner_arc_feed(feed_slot, rough_corner, r),
        // 원호로 굴러 들어가는 동안은 날 끝 이송 절반 (Harvey 'roll-in' 50 % 감속, Sandvik: 원호 진입으로 이탈 칩 두께 0)
        feed_lead_in: inner_arc_feed(0.5 * feed_side, lead_radius, r),
        feed_lead_out: inner_arc_feed(feed_side, lead_radius, r),
        slot_feed_ratio,
        lead_radius_mm: lead_radius,
        corner_radius_mm: r + finish_corner,
        finish_corner_mm: finish_corner,
        rough_corner_mm: rough_corner,
        feed_finish_arc: inner_arc_feed(feed_side, finish_corner, r),
        notes,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GCodeMetadata {
    pub generated_at: String,
    pub profile_name: String,
    pub workpiece_material: String,
    pub tool_model: String,
    pub coolant_method: String,
    pub estimated_time_min: f64,
    pub total_distance_mm: f64,
    pub warnings: Vec<String>,
    #[serde(default)]
    pub cutting_time_min: f64,
    #[serde(default)]
    pub rapid_time_min: f64,
    #[serde(default)]
    pub cutting_distance_mm: f64,
}

pub struct GCodeGenerator;

impl GCodeGenerator {
    pub fn generate_synthetic_segments_with_pattern(
        profile: &MachiningProfile,
        pattern: &ToolPathPattern,
    ) -> Vec<ToolPathSegment> {
        let feed = profile.conditions.feed_rate_mm_min;
        let cut_z = -profile.conditions.axial_doc_mm;
        let safe_z = cut_z + profile.endmill_setting.loc_mm + 10.0;
        let w = profile.workpiece_setup.width_mm;
        let h = profile.workpiece_setup.height_mm;
        let dia = profile.endmill_setting.diameter_mm;
        let plunge_feed = feed * 0.4;

        let mut segments = Vec::new();

        segments.push(ToolPathSegment::Rapid { x: 0.0, y: 0.0, z: safe_z });

        match pattern {
            ToolPathPattern::RectangularProfile => {
                let allowance = profile.workpiece_setup.stock_allowance_mm.max(0.0).min(dia / 2.0);
                let off = dia / 2.0 - allowance;
                segments.push(ToolPathSegment::Rapid { x: -off - dia, y: -off, z: safe_z });
                segments.push(ToolPathSegment::Plunge {
                    x: -off - dia, y: -off,
                    z_start: safe_z, z_end: cut_z, feed: plunge_feed,
                });
                segments.push(ToolPathSegment::Linear { x: w + off, y: -off, z: cut_z, feed });
                segments.push(ToolPathSegment::Linear { x: w + off, y: h + off, z: cut_z, feed });
                segments.push(ToolPathSegment::Linear { x: -off, y: h + off, z: cut_z, feed });
                segments.push(ToolPathSegment::Linear { x: -off, y: -off - dia, z: cut_z, feed });
            }

            ToolPathPattern::PocketZigZag => {
                Self::pocket_zigzag(&pocket_plan(profile), safe_z, &mut segments);
            }

            ToolPathPattern::CircularProfile { center_x, center_y, radius } => {
                let start_x = center_x + radius;
                let start_y = *center_y;
                segments.push(ToolPathSegment::Rapid { x: start_x, y: start_y, z: safe_z });
                segments.push(ToolPathSegment::Plunge {
                    x: start_x, y: start_y,
                    z_start: safe_z, z_end: cut_z, feed: plunge_feed,
                });
                for q in 0..4 {
                    let a0 = -(q as f64) * std::f64::consts::FRAC_PI_2;
                    let a1 = a0 - std::f64::consts::FRAC_PI_2;
                    segments.push(ToolPathSegment::ArcCW {
                        x: center_x + radius * a1.cos(),
                        y: center_y + radius * a1.sin(),
                        z: cut_z,
                        i: -radius * a0.cos(),
                        j: -radius * a0.sin(),
                        feed,
                    });
                }
            }

            ToolPathPattern::HelicalPocket { center_x, center_y, radius, depth_per_rev } => {
                let total_depth = profile.conditions.axial_doc_mm;
                let pitch = depth_per_rev.max(0.01);
                let revolutions = (total_depth / pitch).ceil().max(1.0) as usize;
                let start_x = center_x + radius;
                let start_y = *center_y;

                segments.push(ToolPathSegment::Rapid { x: start_x, y: start_y, z: safe_z });
                segments.push(ToolPathSegment::Plunge {
                    x: start_x, y: start_y, z_start: safe_z, z_end: 0.0, feed: plunge_feed,
                });

                for rev in 0..revolutions {
                    for q in 0..4 {
                        let a0 = q as f64 * std::f64::consts::FRAC_PI_2;
                        let a1 = a0 + std::f64::consts::FRAC_PI_2;
                        let frac = (rev as f64 + (q + 1) as f64 / 4.0) * pitch;
                        let pz = (-frac).max(cut_z);
                        segments.push(ToolPathSegment::ArcCCW {
                            x: center_x + radius * a1.cos(),
                            y: center_y + radius * a1.sin(),
                            z: pz,
                            i: -radius * a0.cos(),
                            j: -radius * a0.sin(),
                            feed,
                        });
                    }
                }

                for q in 0..4 {
                    let a0 = q as f64 * std::f64::consts::FRAC_PI_2;
                    let a1 = a0 + std::f64::consts::FRAC_PI_2;
                    segments.push(ToolPathSegment::ArcCCW {
                        x: center_x + radius * a1.cos(),
                        y: center_y + radius * a1.sin(),
                        z: cut_z,
                        i: -radius * a0.cos(),
                        j: -radius * a0.sin(),
                        feed,
                    });
                }
            }

            ToolPathPattern::ContourMultiPass { offset_count, step_over_mm } => {
                for pass in 0..*offset_count {
                    let offset = (pass as f64 + 1.0) * step_over_mm;
                    let pw = w - offset * 2.0;
                    let ph = h - offset * 2.0;
                    if pw <= 0.0 || ph <= 0.0 { break; }

                    let sx = offset;
                    let sy = offset;

                    if pass == 0 {
                        segments.push(ToolPathSegment::Rapid { x: sx, y: sy, z: safe_z });
                        segments.push(ToolPathSegment::Plunge {
                            x: sx, y: sy, z_start: safe_z, z_end: cut_z, feed: plunge_feed,
                        });
                    } else {
                        segments.push(ToolPathSegment::Rapid { x: sx, y: sy, z: safe_z });
                        segments.push(ToolPathSegment::Plunge {
                            x: sx, y: sy, z_start: safe_z, z_end: cut_z, feed: plunge_feed,
                        });
                    }

                    segments.push(ToolPathSegment::Linear { x: sx + pw, y: sy, z: cut_z, feed });
                    segments.push(ToolPathSegment::Linear { x: sx + pw, y: sy + ph, z: cut_z, feed });
                    segments.push(ToolPathSegment::Linear { x: sx, y: sy + ph, z: cut_z, feed });
                    segments.push(ToolPathSegment::Linear { x: sx, y: sy, z: cut_z, feed });
                }
            }

            ToolPathPattern::Slot { start_x, start_y, end_x, end_y, width } => {
                let half_w = width / 2.0;
                let dx = end_x - start_x;
                let dy = end_y - start_y;
                let len = (dx * dx + dy * dy).sqrt();
                if len < 0.001 {
                    segments.push(ToolPathSegment::Rapid { x: 0.0, y: 0.0, z: safe_z });
                    return Self::insert_safe_retracts(segments, safe_z);
                }
                let nx = -dy / len * half_w;
                let ny = dx / len * half_w;

                segments.push(ToolPathSegment::Rapid { x: start_x + nx, y: start_y + ny, z: safe_z });
                segments.push(ToolPathSegment::Plunge {
                    x: start_x + nx, y: start_y + ny,
                    z_start: safe_z, z_end: cut_z, feed: plunge_feed,
                });
                segments.push(ToolPathSegment::Linear { x: end_x + nx, y: end_y + ny, z: cut_z, feed });
                segments.push(ToolPathSegment::Linear { x: end_x - nx, y: end_y - ny, z: cut_z, feed });
                segments.push(ToolPathSegment::Linear { x: start_x - nx, y: start_y - ny, z: cut_z, feed });
                segments.push(ToolPathSegment::Linear { x: start_x + nx, y: start_y + ny, z: cut_z, feed });
            }
        }

        segments.push(ToolPathSegment::Rapid { x: 0.0, y: 0.0, z: safe_z });
        Self::insert_safe_retracts(segments, safe_z)
    }

    /// 포켓 지그재그 (`pocket_plan`): 단계마다 헬리컬 진입 → 첫 줄 슬롯 → 줄 끝 피치 이동(슬롯형) · 측면 절삭 줄을 번갈아, 마지막에 정삭 윤곽
    fn pocket_zigzag(plan: &PocketPlan, safe_z: f64, segments: &mut Vec<ToolPathSegment>) {
        let (rx0, ry0, rx1, ry1) = plan.rough_rect;
        let (fs, fslot, fent) = (plan.feed_side, plan.feed_slot, plan.feed_entry);
        // 황삭 경계 = 모서리를 반경 rcr 로 둥글린 사각형. 높이 y 에서 경계가 옆 변보다 안쪽으로 들어온 거리
        let rcr = plan.rough_corner_mm;
        let inset = |y: f64| {
            let dy = (y - ry0).min(ry1 - y).max(0.0);
            if rcr <= 0.0 || dy >= rcr {
                0.0
            } else {
                rcr - (rcr * rcr - (rcr - dy).powi(2)).max(0.0).sqrt()
            }
        };
        // 줄 끝 피치 이동: 한쪽 옆 변(side +1 오른쪽, −1 왼쪽)의 경계를 따라 ya → yb 로 올라간다 — 아래 모서리 원호, 곧은 변, 위 모서리 원호
        // (오른쪽 변을 올라가면 반시계, 왼쪽은 시계). 모서리 원호는 바깥이 소재라 안쪽 원호 이송 보정 — 날 끝 = 슬롯 이송 (`PocketPlan::feed_rough_arc`)
        let follow = |segments: &mut Vec<ToolPathSegment>, side: f64, ya: f64, yb: f64, z: f64| {
            let edge = if side > 0.0 { rx1 } else { rx0 };
            let cx = edge - side * rcr;
            let (bot, top) = (ry0 + rcr, ry1 - rcr);
            let x_at = |y: f64, cy: f64| cx + side * (rcr * rcr - (y - cy).powi(2)).max(0.0).sqrt();
            let push_arc = |segments: &mut Vec<ToolPathSegment>, cy: f64, y0: f64, y1: f64| {
                let (xs, xe) = (x_at(y0, cy), x_at(y1, cy));
                if (xe - xs).abs() + (y1 - y0).abs() > 1e-9 {
                    let (i, j) = (cx - xs, cy - y0);
                    segments.push(if side > 0.0 {
                        ToolPathSegment::ArcCCW { x: xe, y: y1, z, i, j, feed: plan.feed_rough_arc }
                    } else {
                        ToolPathSegment::ArcCW { x: xe, y: y1, z, i, j, feed: plan.feed_rough_arc }
                    });
                }
            };
            let mut y = ya;
            if rcr > 0.0 && y < bot - 1e-12 {
                let ye = yb.min(bot);
                push_arc(segments, bot, y, ye);
                y = ye;
            }
            let ye = yb.min(top.max(bot));
            if y < ye - 1e-12 {
                segments.push(ToolPathSegment::Linear { x: edge, y: ye, z, feed: fslot });
                y = ye;
            }
            if rcr > 0.0 && yb > y + 1e-12 {
                push_arc(segments, top, y.max(top), yb);
            }
        };
        // 진입점으로: 안전 높이에서 옮겨 소재 윗면 위 이송 높이까지 급이송, 그 아래(이미 비운 단계)는 이송으로 내려간다 — 소재 윗면 아래의 G00 은 없다
        let descend = |segments: &mut Vec<ToolPathSegment>, x: f64, y: f64, z_to: f64| {
            segments.push(ToolPathSegment::Rapid { x, y, z: safe_z });
            segments.push(ToolPathSegment::Rapid { x, y, z: ENTRY_CLEAR_MM });
            if z_to < ENTRY_CLEAR_MM - 1e-9 {
                segments.push(ToolPathSegment::Plunge { x, y, z_start: ENTRY_CLEAR_MM, z_end: z_to, feed: fent });
            }
        };
        for k in 0..plan.levels {
            let z_from = -(k as f64) * plan.level_ap_mm;
            let z = -((k + 1) as f64) * plan.level_ap_mm;
            let z_start = z_from + ENTRY_CLEAR_MM;
            // ---- 진입 ----
            match plan.entry.as_str() {
                "helix" => {
                    let (cx, cy) = plan.helix_center;
                    let rh = plan.helix_radius_mm;
                    descend(segments, cx + rh, cy, z_start);
                    let quarter = 0.25 * plan.helix_pitch_mm.max(1e-3);
                    let n = ((z_start - z) / quarter - 1e-9).ceil().max(1.0) as usize;
                    // 공구 중심 원을 반시계(하향 — 구멍 벽이 진행 방향 오른쪽, 상향 소재는 시계)로 돌며 내려간 뒤, 바닥에서 한 바퀴 더 돌아 나선 바닥을 고른다
                    let sgn = if plan.up_milling { -1.0 } else { 1.0 };
                    for q in 0..(n + 4) {
                        let a0 = sgn * (q % 4) as f64 * std::f64::consts::FRAC_PI_2;
                        let a1 = a0 + sgn * std::f64::consts::FRAC_PI_2;
                        let zq = if q < n { z_start - (z_start - z) * (q + 1) as f64 / n as f64 } else { z };
                        let (x, y, i, j) = (cx + rh * a1.cos(), cy + rh * a1.sin(), -rh * a0.cos(), -rh * a0.sin());
                        segments.push(if plan.up_milling {
                            ToolPathSegment::ArcCW { x, y, z: zq, i, j, feed: plan.feed_helix }
                        } else {
                            ToolPathSegment::ArcCCW { x, y, z: zq, i, j, feed: plan.feed_helix }
                        });
                    }
                    segments.push(ToolPathSegment::Linear { x: rx0 + inset(ry0), y: ry0, z, feed: fslot });
                }
                "ramp" => {
                    // 첫 줄 위(둥근 모서리 사이)를 오가며 경사각으로 내려간다 (지그재그 램프)
                    let (lx0, lx1) = (rx0 + inset(ry0), rx1 - inset(ry0));
                    descend(segments, lx0, ry0, z_start);
                    let leg = (lx1 - lx0).max(1e-3);
                    let drop = (leg * plan.ramp_deg.to_radians().tan()).max(1e-3);
                    let (mut zc, mut forward) = (z_start, true);
                    while zc > z + 1e-9 {
                        let zn = (zc - drop).max(z);
                        let run = leg * ((zc - zn) / drop).min(1.0);
                        let x = if forward { lx0 + run } else { lx1 - run };
                        segments.push(ToolPathSegment::Linear { x, y: ry0, z: zn, feed: fent });
                        zc = zn;
                        forward = !forward;
                    }
                }
                _ => descend(segments, rx0 + inset(ry0), ry0, z),
            }
            // ---- 지그재그: 첫 줄은 폭 전체 슬롯, 줄 끝 피치 이동은 공구 앞 띠를 폭 전체로 깎는 슬롯형 — 둘 다 슬롯 이송 ----
            // 줄 끝은 둥근 황삭 경계 위 (모서리 근처 줄은 짧아짐), 피치 이동은 그 경계를 따라간다
            if (segments.last().map(|s| s.end_point().0).unwrap_or(rx0) - (rx0 + inset(ry0))).abs() > 1e-9 {
                segments.push(ToolPathSegment::Linear { x: rx0 + inset(ry0), y: ry0, z, feed: fslot });
            }
            for i in 0..plan.rows {
                let y = ry0 + plan.stepover_mm * i as f64;
                let right = i % 2 == 0;
                if i > 0 {
                    let y_prev = ry0 + plan.stepover_mm * (i - 1) as f64;
                    follow(segments, if right { -1.0 } else { 1.0 }, y_prev, y, z);
                }
                let x_end = if right { rx1 - inset(y) } else { rx0 + inset(y) };
                segments.push(ToolPathSegment::Linear { x: x_end, y, z, feed: if i == 0 { fslot } else { fs } });
            }
        }
        // ---- 정삭 윤곽: 전체 깊이 한 번, 하향(포켓 안쪽 벽은 반시계 — 상향 소재는 시계), 아래 벽 가운데에서 원호로 들어가 원호로 나온다 ----
        if plan.finish_allow_mm > 0.0 {
            let (x0, y0, x1, y1) = plan.finish_rect;
            let z = -plan.depth_mm;
            let rl = plan.lead_radius_mm;
            let xm = 0.5 * (x0 + x1);
            let s = if plan.up_milling { -1.0 } else { 1.0 };
            let arc = |x: f64, y: f64, i: f64, j: f64, feed: f64| {
                if plan.up_milling {
                    ToolPathSegment::ArcCW { x, y, z, i, j, feed }
                } else {
                    ToolPathSegment::ArcCCW { x, y, z, i, j, feed }
                }
            };
            let (sx, sy) = (xm - s * rl, y0 + rl);
            descend(segments, sx, sy, z);
            segments.push(arc(xm, y0, s * rl, 0.0, plan.feed_lead_in));
            // 둥근 모서리 사각형: 변 끝까지 직선, 모서리는 공구 중심 원호 rc (벽 모서리 반경 r + rc) — 원호는 안쪽 원호 이송 보정
            let rc = plan.finish_corner_mm;
            let corners: [((f64, f64), (f64, f64), (f64, f64)); 4] = if plan.up_milling {
                [
                    ((x0 + rc, y0), (x0, y0 + rc), (x0 + rc, y0 + rc)),
                    ((x0, y1 - rc), (x0 + rc, y1), (x0 + rc, y1 - rc)),
                    ((x1 - rc, y1), (x1, y1 - rc), (x1 - rc, y1 - rc)),
                    ((x1, y0 + rc), (x1 - rc, y0), (x1 - rc, y0 + rc)),
                ]
            } else {
                [
                    ((x1 - rc, y0), (x1, y0 + rc), (x1 - rc, y0 + rc)),
                    ((x1, y1 - rc), (x1 - rc, y1), (x1 - rc, y1 - rc)),
                    ((x0 + rc, y1), (x0, y1 - rc), (x0 + rc, y1 - rc)),
                    ((x0, y0 + rc), (x0 + rc, y0), (x0 + rc, y0 + rc)),
                ]
            };
            for (a, b, c) in corners {
                segments.push(ToolPathSegment::Linear { x: a.0, y: a.1, z, feed: plan.feed_finish });
                if rc > 1e-9 {
                    segments.push(arc(b.0, b.1, c.0 - a.0, c.1 - a.1, plan.feed_finish_arc));
                }
            }
            segments.push(ToolPathSegment::Linear { x: xm, y: y0, z, feed: plan.feed_finish });
            segments.push(arc(xm + s * rl, y0 + rl, 0.0, rl, plan.feed_lead_out));
        }
    }

    pub fn insert_safe_retracts(segments: Vec<ToolPathSegment>, safe_z: f64) -> Vec<ToolPathSegment> {
        let mut out: Vec<ToolPathSegment> = Vec::with_capacity(segments.len() + 8);
        let mut prev: Option<(f64, f64, f64)> = None;
        for seg in segments.into_iter() {
            if let (ToolPathSegment::Rapid { x, y, .. }, Some((px, py, pz))) = (&seg, prev) {
                let lateral = (x - px).abs() > 1e-9 || (y - py).abs() > 1e-9;
                if lateral && pz < safe_z - 1e-6 {
                    out.push(ToolPathSegment::Rapid { x: px, y: py, z: safe_z });
                }
            }
            prev = Some(seg.end_point());
            out.push(seg);
        }
        out
    }

    pub fn generate_gcode_with_pattern(
        profile: &MachiningProfile,
        pattern: &ToolPathPattern,
    ) -> GCodeProgram {
        let segments = Self::generate_synthetic_segments_with_pattern(profile, pattern);
        let name = format!("{}_{}", profile.name, format!("{:?}", pattern).split('{').next().unwrap_or("").trim());
        Self::program_from_segments(profile, &segments, &name)
    }

    pub fn program_from_segments(
        profile: &MachiningProfile,
        segments: &[ToolPathSegment],
        program_name: &str,
    ) -> GCodeProgram {
        let mut line_num: u32 = 10;
        let mut header = Vec::new();
        let mut body = Vec::new();
        let mut footer = Vec::new();

        header.push(GCodeLine {
            line_number: line_num,
            code: "G90 G94 G17 G21".into(),
            comment: Some("절대좌표, mm, XY평면".into()),
        });
        line_num += 10;

        header.push(GCodeLine {
            line_number: line_num,
            code: "G54".into(),
            comment: Some("워크 좌표계".into()),
        });
        line_num += 10;

        header.push(GCodeLine {
            line_number: line_num,
            code: format!("T1 M06"),
            comment: Some(format!("공구: {} (Ø{}mm, {}날)",
                profile.endmill_setting.model,
                profile.endmill_setting.diameter_mm,
                profile.endmill_setting.flute_count)),
        });
        line_num += 10;

        header.push(GCodeLine {
            line_number: line_num,
            code: format!("S{} M03", profile.conditions.spindle_rpm),
            comment: Some(format!("스핀들 {} RPM", profile.conditions.spindle_rpm)),
        });
        line_num += 10;

        header.push(GCodeLine {
            line_number: line_num,
            code: "G43 H1".into(),
            comment: Some("공구 길이 보정".into()),
        });
        line_num += 10;

        let coolant_code = profile.coolant_config.method.gcode_m_code();
        header.push(GCodeLine {
            line_number: line_num,
            code: coolant_code.to_string(),
            comment: Some(format!("냉각: {} ({}bar, {}L/min)",
                profile.coolant_config.method.label(),
                profile.coolant_config.pressure_bar,
                profile.coolant_config.flow_rate_l_min)),
        });
        line_num += 10;

        for seg in segments {
            match seg {
                ToolPathSegment::Rapid { x, y, z } => {
                    body.push(GCodeLine {
                        line_number: line_num,
                        code: format!("G00 X{:.3} Y{:.3} Z{:.3}", x, y, z),
                        comment: None,
                    });
                }
                ToolPathSegment::Linear { x, y, z, feed } => {
                    body.push(GCodeLine {
                        line_number: line_num,
                        code: format!("G01 X{:.3} Y{:.3} Z{:.3} F{:.0}", x, y, z, feed),
                        comment: None,
                    });
                }
                ToolPathSegment::Plunge { x, y, z_start: _, z_end, feed } => {
                    body.push(GCodeLine {
                        line_number: line_num,
                        code: format!("G01 X{:.3} Y{:.3} Z{:.3} F{:.0}", x, y, z_end, feed),
                        comment: Some("절입".into()),
                    });
                }
                ToolPathSegment::ArcCW { x, y, z, i, j, feed } => {
                    body.push(GCodeLine {
                        line_number: line_num,
                        code: format!("G02 X{:.3} Y{:.3} Z{:.3} I{:.3} J{:.3} F{:.0}", x, y, z, i, j, feed),
                        comment: None,
                    });
                }
                ToolPathSegment::ArcCCW { x, y, z, i, j, feed } => {
                    body.push(GCodeLine {
                        line_number: line_num,
                        code: format!("G03 X{:.3} Y{:.3} Z{:.3} I{:.3} J{:.3} F{:.0}", x, y, z, i, j, feed),
                        comment: None,
                    });
                }
            }
            line_num += 10;
        }

        footer.push(GCodeLine { line_number: line_num, code: profile.coolant_config.method.gcode_off_code().into(), comment: Some("냉각 정지".into()) });
        line_num += 10;
        footer.push(GCodeLine { line_number: line_num, code: "M05".into(), comment: Some("스핀들 정지".into()) });
        line_num += 10;
        footer.push(GCodeLine { line_number: line_num, code: "G91 G28 Z0.".into(), comment: Some("Z 원점".into()) });
        line_num += 10;
        footer.push(GCodeLine { line_number: line_num, code: "G91 G28 X0. Y0.".into(), comment: Some("XY 원점".into()) });
        line_num += 10;
        footer.push(GCodeLine { line_number: line_num, code: "M30".into(), comment: Some("종료".into()) });

        GCodeProgram {
            program_name: program_name.to_string(),
            header,
            body,
            footer,
        }
    }

    pub fn generate_metadata_with_pattern(
        profile: &MachiningProfile,
        pattern: &ToolPathPattern,
    ) -> GCodeMetadata {
        let segments = Self::generate_synthetic_segments_with_pattern(profile, pattern);
        Self::metadata_for_segments(profile, &segments, profile.machine.rapid_mm_min)
    }

    pub fn metadata_for_segments(
        profile: &MachiningProfile,
        segments: &[ToolPathSegment],
        rapid_mm_min: f64,
    ) -> GCodeMetadata {
        let mut total_dist = 0.0;
        let mut cutting_dist = 0.0;
        let mut cutting_time = 0.0;
        let mut rapid_time = 0.0;
        let mut prev: Option<(f64, f64, f64)> = None;
        let mut warnings = Vec::new();

        let safe_z_threshold = 0.0;
        let loc_limit = profile.endmill_setting.loc_mm;

        for seg in segments {
            let (x, y, z) = seg.end_point();
            if let Some(start) = prev {
                let d = seg.length_from(start);
                total_dist += d;
                match seg.feed() {
                    Some(f) if f > 0.0 => {
                        cutting_dist += d;
                        cutting_time += d / f;
                    }
                    Some(_) => {
                        let msg = "이송 속도가 0 인 절삭 이동이 있습니다".to_string();
                        if !warnings.contains(&msg) {
                            warnings.push(msg);
                        }
                    }
                    None => {
                        rapid_time += d / rapid_mm_min.max(1.0);
                    }
                }
            }
            prev = Some((x, y, z));

            match seg {
                ToolPathSegment::Rapid { z, .. } => {
                    if *z < safe_z_threshold {
                        let msg = format!("충돌 위험: 안전 높이 미만에서 G00 급이속 이동 감지 (Z={:.3})", z);
                        if !warnings.contains(&msg) {
                            warnings.push(msg);
                        }
                    }
                }
                ToolPathSegment::Plunge { z_end, .. } => {
                    if z_end.abs() > loc_limit {
                        let msg = format!("샹크 간섭: 플런지 깊이가 공구 유효 길이 초과 (Z={:.3}, LOC={:.1})", z_end, loc_limit);
                        if !warnings.contains(&msg) {
                            warnings.push(msg);
                        }
                    }
                }
                ToolPathSegment::Linear { z, .. } | ToolPathSegment::ArcCW { z, .. } | ToolPathSegment::ArcCCW { z, .. } => {
                    if z.abs() > loc_limit {
                        let msg = format!("샹크 간섭: 가공 깊이가 공구 유효 길이 초과 (Z={:.3}, LOC={:.1})", z, loc_limit);
                        if !warnings.contains(&msg) {
                            warnings.push(msg);
                        }
                    }
                }
            }
        }

        let est_time = cutting_time + rapid_time;

        GCodeMetadata {
            generated_at: format!("{}", std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()),
            profile_name: profile.name.clone(),
            workpiece_material: profile.workpiece_setup.material_label(),
            tool_model: profile.endmill_setting.model.clone(),
            coolant_method: profile.coolant_config.method.label(),
            estimated_time_min: est_time,
            total_distance_mm: total_dist,
            warnings,
            cutting_time_min: cutting_time,
            rapid_time_min: rapid_time,
            cutting_distance_mm: cutting_dist,
        }
    }

    pub fn generate_synthetic_segments(profile: &MachiningProfile) -> Vec<ToolPathSegment> {
        Self::generate_synthetic_segments_with_pattern(profile, &ToolPathPattern::PocketZigZag)
    }

    pub fn generate_from_profile(profile: &MachiningProfile) -> GCodeProgram {
        Self::generate_gcode_with_pattern(profile, &ToolPathPattern::PocketZigZag)
    }

    pub fn generate_metadata(profile: &MachiningProfile) -> GCodeMetadata {
        Self::generate_metadata_with_pattern(profile, &ToolPathPattern::PocketZigZag)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::MachiningPreset;

    #[test]
    fn stepover_note_flags_finishing_ae_on_pocket_pattern() {
        let mut p = MachiningProfile::from_preset(&MachiningPreset::default_steel_general(), "t");
        p.conditions.radial_doc_mm = 0.3;
        let n = stepover_note(&p, &ToolPathPattern::PocketZigZag).expect("note");
        assert!(n.contains("6.7배") && n.contains("사각형 프로파일"), "{}", n);
        assert!(stepover_note(&p, &ToolPathPattern::RectangularProfile).is_none());
        p.conditions.radial_doc_mm = 0.3 * p.endmill_setting.diameter_mm;
        assert!(stepover_note(&p, &ToolPathPattern::PocketZigZag).is_none());
    }

    #[test]
    fn slot_note_flags_side_milling_depth_on_slot_like_moves_where_vendor_data_exists() {
        // 인코넬 프리셋의 슬롯 패턴: 측면 절삭 ap 1·D 를 슬롯에 그대로 → 제조사 슬롯 상한 0.75·D 초과, fz 0.06 = 목표 칩 0.036 의 1.7배
        let mut p = MachiningProfile::from_preset(&MachiningPreset::default_superalloy_roughing(), "t");
        let slot = ToolPathPattern::from_key("slot", &p).unwrap();
        let n = slot_note(&p, &slot).expect("note");
        assert!(n.contains("1.00·D") && n.contains("0.75·D") && n.contains("1.7배"), "{}", n);
        assert!(slot_note(&p, &ToolPathPattern::RectangularProfile).is_none(), "side milling at 1·D is within the vendor envelope");
        // 포켓 지그재그는 슬롯 ap 이하로 단계 절입하므로 같은 프리셋에서 알리지 않는다
        assert!(slot_note(&p, &ToolPathPattern::PocketZigZag).is_none());
        p.conditions.axial_doc_mm = 0.5 * p.endmill_setting.diameter_mm;
        assert!(slot_note(&p, &slot).is_none());
    }

    fn plan_of(preset: &MachiningPreset) -> (MachiningProfile, PocketPlan, Vec<ToolPathSegment>) {
        let p = MachiningProfile::from_preset(preset, "t");
        let plan = pocket_plan(&p);
        let segs = GCodeGenerator::generate_synthetic_segments_with_pattern(&p, &ToolPathPattern::PocketZigZag);
        (p, plan, segs)
    }

    /// 슬롯형 이동의 fz 는 프리셋 fz 에서 작은 ae 용 칩 얇아짐 배수를 뺀 소재 목표 칩 두께 — `cutting::recommend_conditions` 가 그 배수를
    /// 곱하기 전 값이고, 프로그램 fz 를 넘지 않는다 (배수가 없는 프리셋은 그대로)
    #[test]
    fn slot_like_moves_run_at_the_target_chip_without_the_thinning_boost() {
        for preset in MachiningPreset::builtin() {
            let (p, plan, _) = plan_of(&preset);
            let c = &p.conditions;
            let wp = crate::physics::WorkpieceProps::of(&p.workpiece_setup.effective_material());
            let d = p.endmill_setting.diameter_mm;
            let target = (wp.fz_coeff * d).clamp(0.003, 0.25) * if p.endmill_setting.is_high_end { 1.0 } else { 0.8 };
            let fz_slot = c.feed_per_tooth_mm * plan.slot_feed_ratio;
            assert!((fz_slot - target.min(c.feed_per_tooth_mm)).abs() < 1e-9, "{}: slot fz {:.4} vs target {:.4}", preset.name, fz_slot, target);
            assert!(plan.slot_feed_ratio <= 1.0 + 1e-12 && (plan.feed_slot - c.feed_rate_mm_min * plan.slot_feed_ratio).abs() < 1e-9);
        }
        // 인코넬: fz 0.060 은 ae 0.1·D 의 ×1.67 이 든 값 → 슬롯형 이동 0.036 (×0.6)
        let (_, plan, _) = plan_of(&MachiningPreset::default_superalloy_roughing());
        assert!((plan.slot_feed_ratio - 0.6).abs() < 0.01, "{}", plan.slot_feed_ratio);
    }

    /// 단계 절입: 모든 단계가 같은 깊이로 슬롯 ap · 진입 깊이 상한 이하, 단계 합 = 포켓 깊이. 경로의 모든 이동이 단계 바닥보다 깊지 않고,
    /// 이송 플런지는 이미 비운 곳을 내려가는 것뿐이며(소재 윗면 아래 G00 없음), 헬리컬은 회전당 피치만큼씩 내려간다
    #[test]
    fn pocket_steps_down_within_the_slot_depth_and_enters_by_helix() {
        // 인코넬 프리셋: 깊이 12 mm (1·D), 슬롯 ap 0.75·D = 9 mm → 6 mm 두 단계
        let (p, plan, segs) = plan_of(&MachiningPreset::default_superalloy_roughing());
        let d = p.endmill_setting.diameter_mm;
        assert!(plan.levels == 2 && (plan.level_ap_mm - 6.0).abs() < 1e-9, "{:?}", (plan.levels, plan.level_ap_mm));
        assert!(plan.level_ap_mm <= plan.slot_ap_max_mm.expect("Ni alloys have vendor slot data") + 1e-9);
        // 슬롯 ap 가 깊이 이상이면 한 단계 (탄소강 1.0·D, 깊이 1·D)
        assert_eq!(pocket_plan(&MachiningProfile::from_preset(&MachiningPreset::default_steel_general(), "s")).levels, 1);
        assert_eq!(plan.entry, "helix");
        assert!(plan.helix_bore_mm >= HELIX_BORE_MIN_RATIO * d - 1e-9 && plan.helix_bore_mm <= HELIX_BORE_RATIO * d + 1e-9);
        let pitch_max = 2.0 * std::f64::consts::PI * plan.helix_radius_mm * plan.ramp_deg.to_radians().tan();
        assert!((plan.helix_pitch_mm - pitch_max).abs() < 1e-9);
        // 헬리컬의 공구 중심 이송 = 슬롯 이송 × (구멍 − D)/구멍 (구멍 1.5·D → 1/3)
        assert!((plan.feed_helix - plan.feed_slot * (plan.helix_bore_mm - d) / plan.helix_bore_mm).abs() < 1e-9);
        let mut prev = (0.0, 0.0, 0.0);
        let mut helix_quarters = 0;
        for s in segs.iter() {
            let e = s.end_point();
            assert!(e.2 >= -plan.depth_mm - 1e-9);
            match s {
                ToolPathSegment::Rapid { z, .. } => assert!(*z >= 0.0, "no G00 below the stock top: {:?}", s),
                ToolPathSegment::ArcCCW { z, .. } if *z < prev.2 - 1e-9 => {
                    helix_quarters += 1;
                    assert!(prev.2 - z <= 0.25 * plan.helix_pitch_mm + 1e-9, "helix drops at most a quarter pitch per quarter turn");
                }
                _ => {}
            }
            prev = e;
        }
        let per_level = ((plan.level_ap_mm + ENTRY_CLEAR_MM) / (0.25 * plan.helix_pitch_mm) - 1e-9).ceil() as usize;
        assert_eq!(helix_quarters, plan.levels * per_level);
        // 이송 플런지는 공중(이미 비운 단계 위 · 정삭 전 비운 포켓)만: 소재를 깎는 깊이는 이송 높이 이하
        let gm = GCodeGenerator::metadata_for_segments(&p, &segs, p.machine.rapid_mm_min);
        assert!(!gm.warnings.iter().any(|w| w.contains("충돌")), "{:?}", gm.warnings);
    }

    /// 지그재그: 줄 간격이 고르고 상한(`pattern_stepover`) 이하이며 첫 줄과 마지막 줄이 황삭 경계에 닿는다. 첫 줄(폭 전체 슬롯)과 줄 끝 피치 이동
    /// (경계를 따라 — 모서리는 원호)은 슬롯 이송, 나머지 줄은 프리셋 이송. 황삭 경계는 정삭 경계에서 정삭 여유만큼 안쪽
    #[test]
    fn pocket_rows_cover_the_rough_area_at_slot_and_side_feeds() {
        for preset in MachiningPreset::builtin() {
            let (p, plan, segs) = plan_of(&preset);
            let (rx0, ry0, rx1, ry1) = plan.rough_rect;
            let (fx0, fy0, fx1, fy1) = plan.finish_rect;
            let rcr = plan.rough_corner_mm;
            let fin = segs.iter().rposition(|s| matches!(s, ToolPathSegment::Plunge { .. })).unwrap();
            // 황삭(정삭 하강 앞)의 마지막 단계에서 모서리 원호는 피치 이동의 일부라 슬롯 이송
            let flat_arcs: Vec<&ToolPathSegment> = segs[..fin].iter().filter(|s| matches!(s, ToolPathSegment::ArcCCW { z, .. } | ToolPathSegment::ArcCW { z, .. } if (*z + plan.depth_mm).abs() < 1e-9)).collect();
            assert!(flat_arcs.iter().all(|s| s.feed() == Some(plan.feed_rough_arc) || s.feed() == Some(plan.feed_helix)), "{}: corner arcs of the stepovers at the compensated slot feed", preset.name);
            assert!(plan.rows >= 2 && plan.stepover_mm <= pattern_stepover(&p, 0.2, 0.6) + 1e-9, "{}", preset.name);
            assert!(((ry1 - ry0) - plan.stepover_mm * (plan.rows - 1) as f64).abs() < 1e-9, "{}: the last row lands on the rough boundary", preset.name);
            let a = plan.finish_allow_mm;
            assert!(a > 0.0 && (rx0 - fx0 - a).abs() < 1e-9 && (fx1 - rx1 - a).abs() < 1e-9 && (ry0 - fy0 - a).abs() < 1e-9 && (fy1 - ry1 - a).abs() < 1e-9);
            assert!((fx0 - plan.wall_mm - p.endmill_setting.diameter_mm / 2.0).abs() < 1e-9);
            // 이송: 슬롯형 이동(첫 줄 · y 방향 피치 이동) = 슬롯 이송, 측면 절삭 줄 = 프리셋 이송 (마지막 단계의 지그재그를 본다)
            let mut prev = (0.0, 0.0, 0.0);
            let (mut side, mut slot_like, mut first) = (0usize, 0usize, 0usize);
            for s in segs[..fin].iter() {
                let e = s.end_point();
                if let ToolPathSegment::Linear { feed, .. } = s {
                    let flat = (e.2 - prev.2).abs() < 1e-12 && (e.2 + plan.depth_mm).abs() < 1e-9;
                    if flat && e.1 > prev.1 + 1e-9 && (e.0 - prev.0).abs() < 1e-9 && (e.0 - rx0).abs().min((e.0 - rx1).abs()) < 1e-9 {
                        assert!((feed - plan.feed_slot).abs() < 1e-9, "{}: stepover at slot feed", preset.name);
                        slot_like += 1;
                    } else if flat && (e.1 - prev.1).abs() < 1e-12 && e.1 > ry0 + 1e-9 && e.1 <= ry1 + 1e-9 && (e.0 - prev.0).abs() > 0.5 * (rx1 - rx0) {
                        assert!((feed - plan.feed_side).abs() < 1e-9, "{}: side rows at the preset feed", preset.name);
                        side += 1;
                    } else if flat && (e.1 - ry0).abs() < 1e-12 && (prev.1 - ry0).abs() < 1e-12 && (e.0 - (rx1 - rcr)).abs() < 1e-9 {
                        // 첫 줄은 둥근 모서리 사이 (rx0 + rcr → rx1 − rcr) 폭 전체 슬롯
                        assert!((feed - plan.feed_slot).abs() < 1e-9 && (prev.0 - (rx0 + rcr)).abs() < 1e-9, "{}: the first full-width row is a slot", preset.name);
                        first += 1;
                    }
                }
                prev = e;
            }
            assert_eq!(first, 1, "{}: one first row at the last level", preset.name);
            assert_eq!(slot_like, plan.rows - 1, "{}", preset.name);
            assert_eq!(side, plan.rows - 1, "{}", preset.name);
        }
    }

    /// 정삭 윤곽 구간: 정삭 하강(마지막 플런지) 뒤 급이송 전까지 — (세그먼트, 시작점)
    fn finish_moves(segs: &[ToolPathSegment]) -> Vec<(ToolPathSegment, (f64, f64, f64))> {
        let fin = segs.iter().rposition(|s| matches!(s, ToolPathSegment::Plunge { .. })).expect("finishing descent");
        let mut start = segs[fin].end_point();
        let mut out = Vec::new();
        for s in segs[fin + 1..].iter().take_while(|s| !matches!(s, ToolPathSegment::Rapid { .. })) {
            out.push((s.clone(), start));
            start = s.end_point();
        }
        out
    }

    /// 정삭 윤곽: 원호로 들어가(이송 절반) 둥근 모서리 사각형을 한 바퀴 — 직선은 프리셋 이송, 모서리는 반경 r/3 원호 (벽 모서리 반경 4/3·r) 를
    /// 안쪽 원호 이송 보정 F·rc/(rc + r) 로, 하향 = 반시계 (CFRP 는 상향 = 시계) — 원호로 나온다
    #[test]
    fn a_climb_contour_with_dfm_corners_finishes_the_walls() {
        for preset in MachiningPreset::builtin() {
            let (p, plan, segs) = plan_of(&preset);
            let r = p.endmill_setting.diameter_mm / 2.0;
            let (fx0, fy0, fx1, fy1) = plan.finish_rect;
            let rc = plan.finish_corner_mm;
            assert!((plan.corner_radius_mm - (r + rc)).abs() < 1e-9 && plan.corner_radius_mm >= CORNER_RADIUS_PER_TOOL_RADIUS * r - 1e-9, "{}", preset.name);
            assert!((plan.feed_finish_arc - plan.feed_finish * rc / (rc + r)).abs() < 1e-9);
            let moves = finish_moves(&segs);
            assert_eq!(moves.len(), 11, "{}: lead-in, 4 × (side, corner), closing side, lead-out", preset.name);
            assert!(moves.iter().all(|(s, _)| (s.end_point().2 + plan.depth_mm).abs() < 1e-9), "{}: full depth", preset.name);
            assert_eq!(moves[0].0.feed(), Some(plan.feed_lead_in));
            assert_eq!(moves[10].0.feed(), Some(plan.feed_lead_out));
            let centers = [(fx1 - rc, fy0 + rc), (fx1 - rc, fy1 - rc), (fx0 + rc, fy1 - rc), (fx0 + rc, fy0 + rc)];
            let mut ring: Vec<(f64, f64)> = Vec::new();
            for (k, (s, start)) in moves[1..10].iter().enumerate() {
                if k % 2 == 0 {
                    assert!(matches!(s, ToolPathSegment::Linear { .. }) && s.feed() == Some(plan.feed_finish), "{}: side {}", preset.name, k);
                } else {
                    let (cx, cy, ra, _) = s.arc_sweep(*start).expect("corner arc");
                    assert!((ra - rc).abs() < 1e-9 && s.feed() == Some(plan.feed_finish_arc), "{}: corner arc radius {:.3} vs {:.3}", preset.name, ra, rc);
                    assert!(centers.iter().any(|c| (c.0 - cx).abs() < 1e-9 && (c.1 - cy).abs() < 1e-9), "{}: corner centre ({:.3}, {:.3})", preset.name, cx, cy);
                }
                ring.extend(s.polyline_from(*start, 0.2).iter().map(|q| (q.0, q.1)));
            }
            // 네 변을 모두 지난다 (공구 중심이 정삭 사각형의 각 변 위)
            for (on, label) in [
                (ring.iter().any(|q| (q.0 - fx1).abs() < 1e-9), "+X"),
                (ring.iter().any(|q| (q.1 - fy1).abs() < 1e-9), "+Y"),
                (ring.iter().any(|q| (q.0 - fx0).abs() < 1e-9), "−X"),
                (ring.iter().any(|q| (q.1 - fy0).abs() < 1e-9), "−Y"),
            ] {
                assert!(on, "{}: the contour reaches the {} wall", preset.name, label);
            }
            let area: f64 = ring.iter().zip(ring.iter().cycle().skip(1)).map(|(a, b)| a.0 * b.1 - b.0 * a.1).sum::<f64>() / 2.0;
            // 하향 = 반시계 (넓이 +), 상향을 지시하는 소재(CFRP — Sandvik)는 시계
            assert!(if plan.up_milling { area < 0.0 } else { area > 0.0 }, "{}: contour direction (up milling {})", preset.name, plan.up_milling);
            assert!(matches!(moves[10].0, ToolPathSegment::ArcCCW { .. } | ToolPathSegment::ArcCW { .. }), "{}: arc lead-out", preset.name);
        }
    }

    /// 모든 안쪽 원호는 공구 중심 이송을 ρ/(ρ + r) 로 줄여 날 끝 이송이 그 이동의 몫과 같다: 헬리컬 = 진입(슬롯) 이송, 황삭 모서리 = 슬롯 이송,
    /// 정삭 원호 진입 = 정삭 이송의 절반, 정삭 모서리·원호 이탈 = 정삭 이송 (반경 0.5 mm 모서리를 보정 없이 돌면 Ø6 날 끝 이송이 7배)
    #[test]
    fn every_inner_arc_runs_at_its_edge_feed() {
        for preset in MachiningPreset::builtin() {
            let (p, plan, segs) = plan_of(&preset);
            let r = p.endmill_setting.diameter_mm / 2.0;
            let edge = [plan.feed_entry, plan.feed_slot, 0.5 * plan.feed_finish, plan.feed_finish];
            let mut start = (0.0, 0.0, 0.0);
            let mut arcs = 0;
            for s in segs.iter() {
                if let (Some((_, _, rho, _)), Some(f)) = (s.arc_sweep(start), s.feed()) {
                    let at_edge = f * (rho + r) / rho;
                    assert!(edge.iter().any(|e| (e - at_edge).abs() < 1e-6 * e.max(1.0)), "{}: arc ρ {:.3} mm at {:.2} mm/min = edge {:.2} mm/min, not one of {:?}", preset.name, rho, f, at_edge, edge);
                    arcs += 1;
                }
                start = s.end_point();
            }
            assert!(arcs > 0);
        }
        assert!((inner_arc_feed(840.0, 0.5, 3.0) - 120.0).abs() < 1e-9, "Ø6 tool on a 0.5 mm arc: edge 7× the centre feed");
        assert_eq!(inner_arc_feed(840.0, 0.0, 3.0), 840.0);
    }

    /// 황삭 공구 중심은 둥근 황삭 경계(모서리 반경 rcr) 밖으로 나가지 않고, 정삭 모서리 원호는 같은 중심에서 정삭 여유만큼 바깥 — 모서리에도 여유가 고르다
    #[test]
    fn roughing_keeps_a_uniform_allowance_around_the_rounded_corners() {
        for preset in MachiningPreset::builtin() {
            let (_, plan, segs) = plan_of(&preset);
            let (rx0, ry0, rx1, ry1) = plan.rough_rect;
            let rcr = plan.rough_corner_mm;
            assert!(rcr > 0.0 && (plan.finish_corner_mm - rcr - plan.finish_allow_mm).abs() < 1e-9, "{}: corner allowance", preset.name);
            let fin = segs.iter().rposition(|s| matches!(s, ToolPathSegment::Plunge { .. })).unwrap();
            let mut start = (0.0, 0.0, 0.0);
            let mut worst = 0.0f64;
            for s in segs[..fin].iter() {
                if s.feed().is_some() && s.end_point().2 < 0.0 {
                    for q in s.polyline_from(start, 0.05) {
                        let dx = (rx0 + rcr - q.0).max(q.0 - (rx1 - rcr)).max(0.0);
                        let dy = (ry0 + rcr - q.1).max(q.1 - (ry1 - rcr)).max(0.0);
                        worst = worst.max(dx.hypot(dy) - rcr);
                    }
                }
                start = s.end_point();
            }
            assert!(worst < 1e-6, "{}: the roughing tool centre leaves the rounded boundary by {:.4} mm", preset.name, worst);
        }
    }

    /// 작은 스톡·넓은 공구에서도 경로가 망가지지 않는다: 헬리컬 구멍이 안 들어가면 직선 램프, 램프 길이도 없으면 직선 플런지
    #[test]
    fn narrow_pockets_fall_back_to_ramp_or_plunge_entry() {
        let mut p = MachiningProfile::from_preset(&MachiningPreset::default_steel_general(), "t");
        let d = p.endmill_setting.diameter_mm;
        // 황삭 공구 중심 영역이 (폭, 높이) 가 되는 스톡: 높이 0.1·D 면 헬리컬 구멍(1.2·D 이상)이 안 들어가고, 폭도 D 보다 좁으면 램프할 길이가 없다
        let edge = 2.0 * (pocket_wall_mm(&p) + d / 2.0 + p.workpiece_setup.stock_allowance_mm.min(d / 2.0));
        for (w, h, entry) in [(edge + 3.0 * d, edge + 0.1 * d, "ramp"), (edge + 0.1 * d, edge + 0.1 * d, "plunge"), (0.6 * d, 0.6 * d, "plunge")] {
            p.workpiece_setup.width_mm = w;
            p.workpiece_setup.height_mm = h;
            let plan = pocket_plan(&p);
            assert_eq!(plan.entry, entry, "{}x{}: {:?}", w, h, plan.rough_rect);
            let segs = GCodeGenerator::generate_synthetic_segments_with_pattern(&p, &ToolPathPattern::PocketZigZag);
            assert!(segs.len() > 4 && segs.iter().all(|s| {
                let e = s.end_point();
                e.0.is_finite() && e.1.is_finite() && e.2.is_finite() && e.2 >= -plan.depth_mm - 1e-9
            }));
        }
    }
}
