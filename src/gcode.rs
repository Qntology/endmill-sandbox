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

/// 합성 경로의 슬롯형 이동(지그재그 포켓의 직선 플런지·첫 패스 슬롯·패스 끝 피치 이동, 슬롯 패턴)이 프리셋의 측면 절삭 ap 를 그대로 쓸 때
/// 알리는 문구. 측면 절삭용 ap(인코넬 프리셋 1·D)는 제조사의 슬롯 권장(`WorkpieceProps::slot_ap_ratio`, 확인된 소재만)보다 깊고,
/// 작은 ae 용 칩 얇아짐 보정이 든 프리셋 fz 는 슬롯에서 그대로 칩 두께가 된다 — 2026-10-19 인코넬 포켓의 최종 벽 오차 약 1 mm 중
/// 공구 휨 0.5 mm 가 이 피치 이동(법선 힘 약 7 kN, 몸체 응력이 정적 강도에 닿음)에서 나왔다
pub fn slot_note(profile: &MachiningProfile, pattern: &ToolPathPattern) -> Option<String> {
    let moves = match pattern {
        ToolPathPattern::PocketZigZag => "직선 플런지·첫 패스 슬롯·패스 끝 피치 이동",
        ToolPathPattern::Slot { .. } => "슬롯",
        _ => return None,
    };
    let wp = crate::physics::WorkpieceProps::of(&profile.workpiece_setup.effective_material());
    let lim = wp.slot_ap_ratio?;
    let d = profile.endmill_setting.diameter_mm.max(0.1);
    let ap = profile.conditions.axial_doc_mm.min(profile.endmill_setting.loc_mm);
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
                let step = pattern_stepover(profile, 0.2, 0.6);
                let margin = dia / 2.0 + 1.0;
                let inner_w = w - margin * 2.0;
                let inner_h = h - margin * 2.0;

                segments.push(ToolPathSegment::Rapid { x: margin, y: margin, z: safe_z });
                segments.push(ToolPathSegment::Plunge {
                    x: margin, y: margin,
                    z_start: safe_z, z_end: cut_z, feed: plunge_feed,
                });

                let mut y_pos = margin;
                let mut direction = 1.0_f64;
                while y_pos < margin + inner_h {
                    let x_end = if direction > 0.0 { margin + inner_w } else { margin };
                    segments.push(ToolPathSegment::Linear { x: x_end, y: y_pos, z: cut_z, feed });
                    y_pos += step;
                    if y_pos < margin + inner_h {
                        segments.push(ToolPathSegment::Linear { x: x_end, y: y_pos, z: cut_z, feed: feed * 0.7 });
                    }
                    direction *= -1.0;
                }
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
        // 인코넬 프리셋: 측면 절삭 ap 1·D 를 슬롯형 이동에 그대로 → 제조사 슬롯 상한 0.75·D 초과, fz 0.06 = 목표 칩 0.036 의 1.7배
        let mut p = MachiningProfile::from_preset(&MachiningPreset::default_superalloy_roughing(), "t");
        let n = slot_note(&p, &ToolPathPattern::PocketZigZag).expect("note");
        assert!(n.contains("1.00·D") && n.contains("0.75·D") && n.contains("1.7배"), "{}", n);
        assert!(slot_note(&p, &ToolPathPattern::RectangularProfile).is_none(), "side milling at 1·D is within the vendor envelope");
        p.conditions.axial_doc_mm = 0.5 * p.endmill_setting.diameter_mm;
        assert!(slot_note(&p, &ToolPathPattern::PocketZigZag).is_none());
        // 자료를 확인하지 못한 소재는 판정하지 않는다 (추정값으로 경고하지 않음)
        let al = MachiningProfile::from_preset(&MachiningPreset::default_aluminum_roughing(), "a");
        assert!(slot_note(&al, &ToolPathPattern::PocketZigZag).is_none());
    }
}
