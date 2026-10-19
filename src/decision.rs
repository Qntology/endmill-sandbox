use crate::loadsim::LoadSimReport;
use crate::ml::laya::{AdvisorAnswer, LayaAdvisor};
use crate::mold::DeviationFit;
use crate::physics::CutAnalysis;
use crate::profile::MachineLimits;
use crate::timeseries::ForecastResult;
use crate::wear::{WearComparison, WearState};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Hash)]
pub enum Action {
    Continue,
    ReduceFeed,
    ReduceSpeed,
    ChangeCoolant,
    InspectTool,
    ReplaceTool,
    Stop,
}

impl Action {
    pub fn all() -> [Action; 7] {
        [
            Action::Continue,
            Action::ReduceFeed,
            Action::ReduceSpeed,
            Action::ChangeCoolant,
            Action::InspectTool,
            Action::ReplaceTool,
            Action::Stop,
        ]
    }

    pub fn key(&self) -> &'static str {
        match self {
            Action::Continue => "continue",
            Action::ReduceFeed => "reduce_feed",
            Action::ReduceSpeed => "reduce_speed",
            Action::ChangeCoolant => "change_coolant",
            Action::InspectTool => "inspect_tool",
            Action::ReplaceTool => "replace_tool",
            Action::Stop => "stop",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Action::Continue => "현 조건 유지",
            Action::ReduceFeed => "이송 감속",
            Action::ReduceSpeed => "회전수 감속",
            Action::ChangeCoolant => "냉각 방식 변경",
            Action::InspectTool => "공구 점검",
            Action::ReplaceTool => "공구 교체",
            Action::Stop => "즉시 정지",
        }
    }

    pub fn english(&self) -> &'static str {
        match self {
            Action::Continue => "keep the current cutting conditions",
            Action::ReduceFeed => "reduce the feed rate to lower cutting force and tool deflection",
            Action::ReduceSpeed => "reduce the spindle speed to lower the cutting temperature",
            Action::ChangeCoolant => "change the coolant method to control heat, friction or thermal cracking",
            Action::InspectTool => "pause and inspect the tool edge for wear or chipping",
            Action::ReplaceTool => "replace the end mill now",
            Action::Stop => "stop machining immediately because a hard limit is exceeded",
        }
    }

    pub fn from_key(k: &str) -> Option<Action> {
        Action::all().into_iter().find(|a| a.key() == k)
    }

    pub fn conservativeness(&self) -> u8 {
        match self {
            Action::Continue => 0,
            Action::ReduceFeed | Action::ReduceSpeed | Action::ChangeCoolant => 1,
            Action::InspectTool => 2,
            Action::ReplaceTool => 3,
            Action::Stop => 4,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Gate {
    pub id: String,
    pub label: String,
    pub value: f64,
    pub limit: f64,
    pub unit: String,
    pub severity: u8,
    pub passed: bool,
    pub action: Action,
    pub message: String,
}

pub struct DecisionInput<'a> {
    pub cut: Option<&'a CutAnalysis>,
    pub sim: Option<&'a LoadSimReport>,
    pub wear: Option<&'a WearComparison>,
    pub mold_surface: Option<&'a WearComparison>,
    pub forecast: Option<&'a ForecastResult>,
    pub mold_fit: Option<&'a DeviationFit>,
    pub machine: &'a MachineLimits,
    pub rpm: f64,
    pub tolerance_mm: f64,
    pub allowance_mm: f64,
    pub coating_max_temp_c: f64,
    pub vb_limit_mm: f64,
    pub mc: f64,
    pub workpiece_temp_limit_c: Option<f64>,
    pub material_label: String,
    pub tool_label: String,
    pub coolant_label: String,
    pub finishing: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionReport {
    pub gates: Vec<Gate>,
    pub rule_action: Action,
    pub rule_action_label: String,
    pub rule_reasons: Vec<String>,
    pub feed_scale: Option<f64>,
    pub speed_scale: Option<f64>,
    pub coolant_suggestion: Option<String>,
    pub state_text: String,
    pub advisor: Option<AdvisorAnswer>,
    pub advisor_action: Option<Action>,
    pub advisor_error: Option<String>,
    pub advisor_allowed: bool,
    pub final_action: Action,
    pub final_action_label: String,
    pub escalated_by_advisor: bool,
    pub agreement: Option<bool>,
    pub transition_to_replace: Option<f64>,
    pub notes: Vec<String>,
    /// 작업자가 선택 카드에서 고른 조치 (게이트 판단보다 덜 보수적일 수 없음)
    #[serde(default)]
    pub operator_choice: Option<Action>,
    /// 작업자가 고르기 전 시스템 판단 (게이트 + 조언 상향)
    #[serde(default)]
    pub system_action: Option<Action>,
    /// 선택 카드 (작업자에게 보여 줄 선택지 · 알림 이유)
    #[serde(default)]
    pub choice: Option<crate::choice::ChoicePrompt>,
}

fn gate(id: &str, label: &str, value: f64, limit: f64, unit: &str, severity: u8, action: Action, message: String) -> Gate {
    Gate {
        id: id.into(),
        label: label.into(),
        value,
        limit,
        unit: unit.into(),
        severity,
        passed: severity == 0,
        action,
        message,
    }
}

pub fn evaluate(inp: &DecisionInput) -> DecisionReport {
    let mut gates: Vec<Gate> = Vec::new();
    let mut feed_scale: Option<f64> = None;
    let mut speed_scale: Option<f64> = None;
    let mut coolant: Option<String> = None;
    let expo = 1.0 / (1.0 - inp.mc).max(0.5);
    let avail = inp.machine.available_power_kw(inp.rpm).max(1e-6);
    let power = inp
        .sim
        .map(|s| s.max_power_kw)
        .into_iter()
        .chain(inp.cut.map(|c| c.spindle_power_kw))
        .fold(0.0, f64::max);
    if power > 0.0 {
        let ratio = power / avail;
        let (sev, act) = if ratio > 1.15 {
            (3, Action::Stop)
        } else if ratio > 1.0 {
            (3, Action::ReduceFeed)
        } else if ratio > 0.9 {
            (2, Action::ReduceFeed)
        } else {
            (0, Action::Continue)
        };
        if sev > 0 {
            feed_scale = Some(feed_scale.unwrap_or(1.0).min((0.9 / ratio).powf(expo).clamp(0.3, 1.0)));
        }
        gates.push(gate(
            "power",
            "스핀들 동력",
            power,
            avail,
            "kW",
            sev,
            act,
            format!("필요 {:.2} kW / 가용 {:.2} kW ({:.0}%)", power, avail, ratio * 100.0),
        ));
    }
    if let Some(c) = inp.cut {
        let tmax = inp.machine.max_torque_nm();
        let ratio = c.forces.torque_peak_nm / tmax.max(1e-6);
        let sev = if ratio > 1.0 { 3 } else if ratio > 0.9 { 2 } else { 0 };
        gates.push(gate(
            "torque",
            "스핀들 토크",
            c.forces.torque_peak_nm,
            tmax,
            "N·m",
            sev,
            if sev > 0 { Action::ReduceFeed } else { Action::Continue },
            format!("피크 {:.2} N·m / 한계 {:.2} N·m", c.forces.torque_peak_nm, tmax),
        ));
    }
    let defl = inp
        .sim
        .map(|s| s.max_wall_defl_um)
        .into_iter()
        .chain(inp.cut.map(|c| c.wall.deflection_um.abs()))
        .fold(0.0, f64::max);
    if defl > 0.0 {
        let budget = if inp.finishing {
            (0.3 * inp.tolerance_mm * 1000.0).max(2.0)
        } else {
            (0.5 * inp.allowance_mm * 1000.0).max(20.0)
        };
        let ratio = defl / budget;
        let sev = if ratio > 2.0 { 3 } else if ratio > 1.0 { 2 } else if ratio > 0.8 { 1 } else { 0 };
        if sev >= 2 {
            feed_scale = Some(feed_scale.unwrap_or(1.0).min((1.0 / ratio).powf(expo).clamp(0.3, 1.0)));
        }
        gates.push(gate(
            "deflection",
            if inp.finishing { "벽면 휨 (정삭 예산)" } else { "벽면 휨 (황삭 예산)" },
            defl,
            budget,
            "µm",
            sev,
            if sev >= 2 { Action::ReduceFeed } else { Action::Continue },
            format!("예측 휨 {:.1} µm / 예산 {:.1} µm", defl, budget),
        ));
    }
    let temp = inp
        .sim
        .map(|s| s.max_temp_c)
        .into_iter()
        .chain(inp.cut.map(|c| c.thermal.interface_c))
        .fold(0.0, f64::max);
    if temp > 0.0 {
        let lim = 0.85 * inp.coating_max_temp_c;
        let sev = if temp > inp.coating_max_temp_c { 3 } else if temp > lim { 2 } else { 0 };
        if sev > 0 {
            speed_scale = Some(((lim - 22.0) / (temp - 22.0).max(1.0)).powi(3).clamp(0.5, 1.0));
        }
        gates.push(gate(
            "temperature",
            "날끝 온도",
            temp,
            lim,
            "°C",
            sev,
            if sev > 0 { Action::ReduceSpeed } else { Action::Continue },
            format!("날끝 {:.0}°C / 코팅 허용 85% {:.0}°C", temp, lim),
        ));
    }
    let margin = inp
        .sim
        .map(|s| s.min_chatter_margin)
        .filter(|m| *m > 0.0)
        .into_iter()
        .chain(inp.cut.map(|c| c.stability.margin).filter(|m| m.is_finite() && *m > 0.0))
        .fold(f64::INFINITY, f64::min);
    if margin.is_finite() {
        let measured = inp.cut.map(|c| c.stability.measured).unwrap_or(false);
        let sev = if margin < 0.7 && measured {
            2
        } else if margin < 1.0 {
            1
        } else {
            0
        };
        let suggested: Vec<f64> = inp.cut.map(|c| c.stability.suggested_rpm.clone()).unwrap_or_default();
        let lower = suggested.iter().cloned().filter(|n| *n < inp.rpm).fold(None, |acc: Option<f64>, n| Some(acc.map_or(n, |a| a.max(n))));
        let higher = suggested.iter().cloned().find(|n| *n > inp.rpm);
        let mut act = Action::Continue;
        let mut msg = format!(
            "재생 채터 여유 {:.2}배 (1 미만이면 채터, {})",
            margin,
            if measured { "실측 FRF" } else { "모델 추정 FRF · 탭 테스트 입력 시 확정" }
        );
        if sev > 0 {
            match (lower, higher) {
                (Some(n), _) if measured && n >= 0.7 * inp.rpm => {
                    speed_scale = Some(speed_scale.unwrap_or(1.0).min(n / inp.rpm.max(1.0)));
                    act = Action::ReduceSpeed;
                    msg.push_str(&format!(" → 안정 로브 S{:.0} 로 감속", n));
                }
                (Some(n), _) if n >= 0.7 * inp.rpm => msg.push_str(&format!(" → 안정 로브 후보 S{:.0} (탭 테스트로 확인 후 적용)", n)),
                (_, Some(n)) => msg.push_str(&format!(" → 안정 로브 S{:.0} 로 증속하거나 ap 축소 권장", n)),
                _ => msg.push_str(" → ap 축소 권장"),
            }
        }
        gates.push(gate("chatter", "채터 안정성", margin, 1.0, "×", sev, act, msg));
    }
    if let (Some(lim), Some(c)) = (inp.workpiece_temp_limit_c, inp.cut) {
        let t = c.thermal.interface_c;
        let sev = if t > lim { 3 } else { 0 };
        if sev > 0 {
            coolant = Some("air_blast".into());
        }
        gates.push(gate(
            "workpiece_temp",
            "피삭재 허용 온도",
            t,
            lim,
            "°C",
            sev,
            if sev > 0 { Action::ReduceSpeed } else { Action::Continue },
            format!("가공 온도 {:.0}°C / 소재 한계 {:.0}°C", t, lim),
        ));
    }
    if let Some(c) = inp.cut {
        let crack = c.thermal.thermal_crack_risk;
        let sev = if crack > 0.7 { 2 } else if crack > 0.45 { 1 } else { 0 };
        if sev > 0 {
            coolant = Some("air_blast".into());
        }
        gates.push(gate(
            "thermal_crack",
            "열균열 위험",
            crack,
            0.45,
            "",
            sev,
            if sev > 0 { Action::ChangeCoolant } else { Action::Continue },
            format!("단속 절삭 급랭 지수 {:.2}", crack),
        ));
        let bue = c.thermal.bue_risk;
        let sev_b = if bue > 0.3 { 1 } else { 0 };
        if sev_b > 0 && coolant.is_none() {
            coolant = Some("mist".into());
        }
        gates.push(gate(
            "bue",
            "구성인선 위험",
            bue,
            0.3,
            "",
            sev_b,
            if sev_b > 0 { Action::ChangeCoolant } else { Action::Continue },
            format!("BUE 지수 {:.2}", bue),
        ));
        let compat = &c.tribology.compat;
        if compat.severity > 0 {
            let sev_c = compat.severity.min(2);
            let act = if let Some(h) = compat.coolant_hint.as_ref().filter(|_| sev_c >= 2) {
                if coolant.is_none() {
                    coolant = Some(h.clone());
                }
                Action::ChangeCoolant
            } else if compat.chemical && sev_c >= 2 {
                speed_scale = Some(speed_scale.unwrap_or(1.0).min(0.85));
                Action::ReduceSpeed
            } else {
                Action::Continue
            };
            gates.push(gate(
                "pair_compat",
                "코팅·피삭재·냉각 궁합",
                compat.severity as f64,
                1.0,
                "",
                sev_c,
                act,
                compat.messages.join(" / "),
            ));
        }
        let ratio = c.error_budget_um.utilization;
        if inp.finishing {
            let sev_e = if ratio > 1.0 { 2 } else if ratio > 0.8 { 1 } else { 0 };
            gates.push(gate(
                "error_budget",
                "치수 오차 예산",
                c.error_budget_um.total_um,
                c.error_budget_um.tolerance_um,
                "µm",
                sev_e,
                if sev_e >= 2 { Action::ReduceFeed } else { Action::Continue },
                format!(
                    "휨 {:.1} + 마모(10분) {:.1} + 공구열 {:.1} + 소재열 {:.1} = {:.1} µm / 공차 {:.0} µm",
                    c.error_budget_um.deflection_um.abs(),
                    c.error_budget_um.wear_10min_um,
                    c.error_budget_um.thermal_tool_um.abs(),
                    c.error_budget_um.thermal_workpiece_um,
                    c.error_budget_um.total_um,
                    c.error_budget_um.tolerance_um
                ),
            ));
        }
    }
    if let Some(s) = inp.sim {
        let ratio = s.vb_end_mm / inp.vb_limit_mm.max(1e-6);
        let sev = if ratio >= 1.0 { 3 } else if ratio >= 0.8 { 2 } else if ratio >= 0.5 { 1 } else { 0 };
        gates.push(gate(
            "vb_sim",
            "경로 종료 시 플랭크 마모",
            s.vb_end_mm,
            inp.vb_limit_mm,
            "mm",
            sev,
            match sev {
                3 => Action::ReplaceTool,
                2 => Action::InspectTool,
                _ => Action::Continue,
            },
            format!("시뮬레이션 VB {:.3} mm / 한계 {:.2} mm", s.vb_end_mm, inp.vb_limit_mm),
        ));
    }
    if let Some(w) = inp.wear {
        let (sev, act) = match w.state {
            WearState::Replace => (3, Action::ReplaceTool),
            WearState::Alert => (2, Action::InspectTool),
            WearState::Watch => (1, Action::Continue),
            WearState::Normal => (0, Action::Continue),
        };
        gates.push(gate(
            "image_wear",
            "이미지 마모 벡터 (교체 한계 대비)",
            w.severity_index as f64,
            1.0,
            "×",
            sev,
            act,
            format!("{} / 유형: {} / {}", w.state.label(), w.wear_type_label, w.state_reasons.join("; ")),
        ));
    }
    if let Some(w) = inp.mold_surface {
        let sev = match w.state {
            WearState::Replace => 3,
            WearState::Alert => 2,
            WearState::Watch => 1,
            WearState::Normal => 0,
        };
        let act = if sev == 0 {
            Action::Continue
        } else {
            match w.wear_type.as_str() {
                "chatter" => Action::ReduceSpeed,
                "burn" | "tearing" => Action::ChangeCoolant,
                "scallop" => Action::ReduceFeed,
                "burr" => Action::InspectTool,
                _ => Action::InspectTool,
            }
        };
        if sev > 0 && w.wear_type == "chatter" {
            speed_scale = Some(speed_scale.unwrap_or(1.0).min(0.9));
        }
        if sev > 0 && (w.wear_type == "burn" || w.wear_type == "tearing") && coolant.is_none() {
            coolant = Some(if w.wear_type == "burn" { "flood".into() } else { "mist".into() });
        }
        gates.push(gate(
            "mold_surface",
            "금형 표면 이미지 벡터 (경고 한계 대비)",
            w.severity_index as f64,
            1.0,
            "×",
            sev,
            act,
            format!("{} / 유형: {} / {}", w.state.label(), w.wear_type_label, w.state_reasons.join("; ")),
        ));
    }
    if let Some(f) = inp.forecast {
        if let Some(th) = &f.threshold {
            let unit = match f.t_unit.as_str() {
                "s" => "초",
                "min" => "분",
                "h" => "시간",
                "part" => "개",
                _ => "스텝",
            };
            let t_last = f.history_t.last().cloned().unwrap_or(0.0);
            let span = |t: Option<f64>, i: usize| -> String {
                match (t, f.t_unit.as_str()) {
                    (Some(tt), "s") => format!("{:.1}분", (tt - t_last) / 60.0),
                    (Some(tt), "min") | (Some(tt), "h") | (Some(tt), "part") => format!("{:.1}{}", tt - t_last, unit),
                    _ => format!("{}{}", i + 1, unit),
                }
            };
            let (sev, act, msg) = if th.already_exceeded {
                (3, Action::ReplaceTool, format!("마지막 측정값이 이미 한계 {:.3} 을 넘었습니다", th.limit))
            } else if let Some(i) = th.p50_index {
                (2, Action::ReplaceTool, format!("중앙값 예측으로 {} 후 한계 {:.3} 도달", span(th.p50_t, i), th.limit))
            } else if let Some(i) = th.p90_index {
                (1, Action::InspectTool, format!("상위 90% 예측으로 {} 후 한계 도달 가능", span(th.p90_t, i)))
            } else {
                (0, Action::Continue, "예측 구간 내 한계 미도달".to_string())
            };
            let to_unit = |t: f64| -> f64 {
                match f.t_unit.as_str() {
                    "s" => (t - t_last) / 60.0,
                    "h" => (t - t_last) * 60.0,
                    _ => t - t_last,
                }
            };
            let (gate_unit, horizon) = match f.t_unit.as_str() {
                "s" | "min" | "h" => ("분", f.t_future.last().map(|t| to_unit(*t)).unwrap_or(0.0)),
                "part" => ("개", f.t_future.last().map(|t| to_unit(*t)).unwrap_or(0.0)),
                _ => ("스텝", f.point.len() as f64),
            };
            let value = match (th.already_exceeded, th.p50_t, th.p50_index) {
                (true, _, _) => 0.0,
                (false, Some(t), _) if gate_unit != "스텝" => to_unit(t),
                (false, _, Some(i)) => (i + 1) as f64,
                _ => horizon + if gate_unit == "스텝" { 1.0 } else { to_unit(t_last + f.step.max(0.0)) },
            };
            gates.push(gate(
                "forecast",
                &format!("시계열 예측 한계 도달까지 ({})", f.method),
                value,
                horizon,
                gate_unit,
                sev,
                act,
                msg,
            ));
        }
    }
    if let Some(m) = inp.mold_fit {
        let tol_um = inp.tolerance_mm * 1000.0;
        let resid = m.rms_residual_um;
        let sev = if resid > 0.5 * tol_um { 2 } else if resid > 0.3 * tol_um { 1 } else { 0 };
        gates.push(gate(
            "mold_residual",
            "금형 편차 미설명 잔차",
            resid,
            0.5 * tol_um,
            "µm",
            sev,
            if sev >= 2 { Action::InspectTool } else { Action::Continue },
            if sev > 0 {
                format!("RMS 잔차 {:.1} µm (R²={:.2}) — 마모·휨·열·기울기로 설명되지 않는 편차: 채터·측정·셋업 재확인", resid, m.r2)
            } else {
                format!("RMS 잔차 {:.1} µm (R²={:.2}) — 물리 성분으로 설명됨", resid, m.r2)
            },
        ));
        let tilt = |name: &str| m.coefficients.iter().find(|c| c.name == name && c.identifiable).map(|c| c.value).unwrap_or(0.0);
        let (tx, ty) = (tilt("tilt_x_um_per_mm"), tilt("tilt_y_um_per_mm"));
        let tilt_um = (tx * tx + ty * ty).sqrt() * m.span_mm;
        if tilt_um > 0.0 {
            let sev_t = if tilt_um > tol_um { 2 } else if tilt_um > 0.5 * tol_um { 1 } else { 0 };
            gates.push(gate(
                "mold_tilt",
                "금형 셋업 기울기",
                tilt_um,
                0.5 * tol_um,
                "µm",
                sev_t,
                if sev_t >= 2 { Action::InspectTool } else { Action::Continue },
                format!("X {:+.2} µm/mm · Y {:+.2} µm/mm → 측정 폭 {:.0} mm 에서 {:.1} µm (고정구·바이스 평행 확인)", tx, ty, m.span_mm, tilt_um),
            ));
        }
        if let Some(c) = m.coefficients.iter().find(|c| c.name == "axial_offset_um" && c.identifiable) {
            let sev2 = if c.value.abs() > 0.5 * tol_um { 1 } else { 0 };
            gates.push(gate(
                "mold_axial",
                "금형 Z 오프셋",
                c.value,
                0.5 * tol_um,
                "µm",
                sev2,
                Action::Continue,
                format!("공구 길이 보정값 제안: {:+.1} µm", -c.value),
            ));
        }
    }
    let worst = gates.iter().map(|g| g.severity).max().unwrap_or(0);
    let rule_action = gates
        .iter()
        .filter(|g| g.severity == worst && worst > 0)
        .map(|g| g.action)
        .max_by_key(|a| (a.conservativeness(), *a))
        .unwrap_or(Action::Continue);
    let rule_reasons: Vec<String> = gates
        .iter()
        .filter(|g| g.severity >= 2)
        .map(|g| format!("[{}] {}", g.label, g.message))
        .collect();
    let state_text = state_text(inp, &gates);
    DecisionReport {
        rule_action_label: rule_action.label().into(),
        final_action_label: rule_action.label().into(),
        final_action: rule_action,
        rule_action,
        rule_reasons,
        feed_scale,
        speed_scale,
        coolant_suggestion: coolant,
        state_text,
        gates,
        advisor: None,
        advisor_action: None,
        advisor_error: None,
        advisor_allowed: false,
        escalated_by_advisor: false,
        agreement: None,
        transition_to_replace: None,
        notes: Vec::new(),
        operator_choice: None,
        system_action: None,
        choice: None,
    }
}

impl Action {
    /// 선택 카드에 보이는 한 줄 설명
    pub fn detail(&self) -> &'static str {
        match self {
            Action::Continue => "지금 절삭 조건 그대로 계속",
            Action::ReduceFeed => "이송을 낮춰 절삭력·휨을 줄임",
            Action::ReduceSpeed => "회전수를 낮춰 날끝 온도를 줄임 (fz 유지)",
            Action::ChangeCoolant => "냉각 방식을 바꿔 열·마찰·열균열을 조절",
            Action::InspectTool => "멈추고 날 마모·치핑을 확인 (조건 변경 없음)",
            Action::ReplaceTool => "공구를 바꿈 (공구 패널에서 교체·리셋)",
            Action::Stop => "즉시 정지 — 한계를 넘은 항목이 있음",
        }
    }
}

/// 작업자가 고를 수 있는가 — 결정적 게이트 판단보다 덜 보수적인 조치는 고를 수 없다 (게이트가 결정권자).
/// 같은 보수성 단계(이송·회전수·냉각)끼리는 서로 바꿀 수 있다
pub fn operator_can_choose(rep: &DecisionReport, a: Action) -> bool {
    a.conservativeness() >= rep.rule_action.conservativeness()
}

/// 공정 판단 선택 카드. `operator_rate` 는 이 스코프의 작업자–조언 일치율 (SDS)
pub fn choice_prompt(rep: &DecisionReport, operator_rate: Option<(f64, u64)>, advisor_rate: Option<(f64, u64)>) -> crate::choice::ChoicePrompt {
    use crate::choice::{push_rate, ChoiceOption, ChoicePrompt};
    let system = rep.system_action.unwrap_or(rep.final_action);
    let prob = |a: Action| rep.advisor.as_ref().and_then(|ans| ans.labels.iter().position(|k| k == a.key()).and_then(|i| ans.probabilities.get(i).copied()));
    let floor_gates: Vec<String> = rep
        .gates
        .iter()
        .filter(|g| g.severity > 0 && g.action == rep.rule_action)
        .map(|g| format!("{} ({})", g.label, ["통과", "관찰", "초과", "위험"][g.severity.min(3) as usize]))
        .collect();
    let detail = |a: Action| -> String {
        match a {
            Action::ReduceFeed => format!("{} — 이송 ×{:.2}", a.detail(), rep.feed_scale.unwrap_or(DEFAULT_OPERATOR_SCALE)),
            Action::ReduceSpeed => format!("{} — 회전수 ×{:.2}", a.detail(), rep.speed_scale.unwrap_or(DEFAULT_OPERATOR_SCALE)),
            Action::ChangeCoolant => match rep.coolant_suggestion.as_ref() {
                Some(c) => format!("{} — 제안 {}", a.detail(), c),
                None => format!("{} — 제안된 냉각 방식 없음 (기록만)", a.detail()),
            },
            _ => a.detail().to_string(),
        }
    };
    let options: Vec<ChoiceOption> = Action::all()
        .into_iter()
        .map(|a| {
            let enabled = operator_can_choose(rep, a);
            ChoiceOption {
                key: a.key().into(),
                label: a.label().into(),
                detail: detail(a),
                recommended: a == system,
                rule: a == rep.rule_action,
                advisor_probability: prob(a),
                advisor_top: rep.advisor_action == Some(a),
                enabled,
                blocked_by: if enabled { Vec::new() } else { floor_gates.clone() },
                chosen: rep.operator_choice == Some(a),
            }
        })
        .collect();
    let mut attention = Vec::new();
    if system.conservativeness() >= Action::InspectTool.conservativeness() {
        attention.push(format!("{} 이상 조치가 필요합니다", Action::InspectTool.label()));
    } else if system.conservativeness() >= 1 {
        attention.push(format!("조건 조정이 필요합니다 ({})", system.label()));
    }
    if let (Some(false), Some(aa)) = (rep.agreement, rep.advisor_action) {
        attention.push(format!("laya 1위({})와 규칙 판단({})이 다릅니다", aa.label(), rep.rule_action.label()));
    }
    if rep.escalated_by_advisor {
        attention.push("laya 조언으로 공구 점검까지 상향됐습니다".into());
    }
    let mut agreement = Vec::new();
    push_rate(&mut agreement, "규칙–laya", advisor_rate);
    push_rate(&mut agreement, "작업자–laya", operator_rate);
    let mut c = ChoicePrompt {
        kind: "process".into(),
        header: "공정 판단".into(),
        question: "다음 조치를 무엇으로 할까요?".into(),
        options,
        recommended: system.key().into(),
        decided_by: if rep.operator_choice.is_some() {
            "operator".into()
        } else if rep.escalated_by_advisor {
            "rule+laya".into()
        } else {
            "rule".into()
        },
        needs_attention: false,
        attention,
        operator_choice: rep.operator_choice.map(|a| a.key().to_string()),
        advisor_note: match (rep.advisor.as_ref(), rep.advisor_error.as_ref()) {
            (Some(ans), _) => format!(
                "laya 1위 {} {:.0}% · 행동 확률 {:.2}",
                Action::from_key(&ans.top).map(|a| a.label()).unwrap_or(ans.top.as_str()),
                ans.top_probability * 100.0,
                ans.act_probability
            ),
            (None, Some(e)) => e.clone(),
            (None, None) => "laya 조언 미사용".into(),
        },
        agreement,
    };
    c.sort_options(|k| Action::from_key(k).map(|a| a.conservativeness()).unwrap_or(9));
    c.settle_attention();
    c
}

/// 작업자가 고른 조치를 판단에 반영한다 (절삭 조건 변경은 `pipeline::Workspace::choose_decision`)
pub fn apply_operator_choice(rep: &mut DecisionReport, a: Action) -> Result<(), String> {
    if !operator_can_choose(rep, a) {
        return Err(format!(
            "안전 게이트 판단 '{}' 보다 덜 보수적인 '{}' 은 고를 수 없습니다",
            rep.rule_action.label(),
            a.label()
        ));
    }
    if rep.system_action.is_none() {
        rep.system_action = Some(rep.final_action);
    }
    rep.operator_choice = Some(a);
    rep.final_action = a;
    rep.final_action_label = a.label().into();
    Ok(())
}

/// 작업자가 이송·회전수 감속을 골랐는데 게이트가 배율을 정하지 않았을 때 쓰는 배율
pub const DEFAULT_OPERATOR_SCALE: f64 = 0.9;

fn state_text(inp: &DecisionInput, gates: &[Gate]) -> String {
    let mut s = format!(
        "End mill machining state. Workpiece: {}. Tool: {}. Coolant: {}. Operation: {}. ",
        inp.material_label,
        inp.tool_label,
        inp.coolant_label,
        if inp.finishing { "finishing pass" } else { "roughing pass" }
    );
    for g in gates.iter() {
        let status = match g.severity {
            0 => "ok",
            1 => "watch",
            2 => "over limit",
            _ => "critical",
        };
        let name = match g.id.as_str() {
            "power" => "spindle power",
            "torque" => "spindle torque",
            "deflection" => "tool deflection at the wall",
            "temperature" => "cutting edge temperature",
            "workpiece_temp" => "workpiece temperature",
            "thermal_crack" => "thermal crack risk index",
            "bue" => "built-up edge risk index",
            "error_budget" => "dimensional error budget",
            "vb_sim" => "simulated flank wear at end of path",
            "image_wear" => "image-based wear deviation of the cutting edge",
            "forecast" => "time-series wear forecast",
            "mold_residual" => "unexplained mold deviation",
            "mold_axial" => "mold axial offset",
            "mold_tilt" => "mold setup tilt",
            "mold_surface" => "image-based defect deviation of the machined mold surface",
            "chatter" => "regenerative chatter stability margin (limit depth divided by actual depth, below 1 means chatter)",
            "pair_compat" => "coating, workpiece and coolant chemical compatibility severity",
            other => other,
        };
        s.push_str(&format!(
            "{}: {:.3} {} against limit {:.3} ({}). ",
            name,
            g.value,
            g.unit.replace('µ', "u").replace('°', " deg ").replace('×', "x"),
            g.limit,
            status
        ));
    }
    if let Some(c) = inp.cut {
        if let Some(m) = c.wear.mechanisms.iter().find(|m| m.key == c.wear.dominant) {
            s.push_str(&format!(
                "Dominant predicted wear mechanism: {} ({:.0} percent). Effective friction coefficient {:.2}, lubricant access {:.2}. ",
                m.key,
                m.share * 100.0,
                c.tribology.mu_eff,
                c.tribology.lubricant_access
            ));
        }
    }
    if let Some(w) = inp.wear {
        s.push_str(&format!(
            "Visual wear state: {}. Dominant visual change: {}. ",
            w.state.key(),
            w.wear_type
        ));
    }
    if let Some(w) = inp.mold_surface {
        s.push_str(&format!(
            "Mold surface state: {}. Dominant surface defect: {}. ",
            w.state.key(),
            w.wear_type
        ));
    }
    s
}

pub fn apply_advisor(rep: &mut DecisionReport, adv: &LayaAdvisor, allow_escalation: bool) {
    rep.advisor_allowed = allow_escalation;
    let options: Vec<(String, String)> = Action::all().iter().map(|a| (a.key().to_string(), a.english().to_string())).collect();
    match adv.ask_choice(
        "Which action should the machine operator take next to keep the end mill and the mold within tolerance and avoid tool failure?",
        &options,
        &rep.state_text,
    ) {
        Ok(ans) => {
            let act = Action::from_key(&ans.top);
            rep.agreement = act.map(|a| a == rep.rule_action);
            if let Some(a) = act {
                if allow_escalation
                    && a.conservativeness() > rep.rule_action.conservativeness()
                    && ans.top_probability >= 0.6
                    && ans.act_probability >= 0.5
                {
                    let escalated = if rep.rule_action.conservativeness() >= Action::InspectTool.conservativeness() {
                        rep.rule_action
                    } else {
                        Action::InspectTool
                    };
                    if escalated != rep.final_action {
                        rep.final_action = escalated;
                        rep.final_action_label = escalated.label().into();
                        rep.escalated_by_advisor = true;
                    }
                }
            }
            rep.advisor_action = act;
            rep.advisor = Some(ans);
        }
        Err(e) => rep.advisor_error = Some(e),
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn report(rule: Action) -> DecisionReport {
        DecisionReport {
            gates: vec![Gate {
                id: "vb_sim".into(),
                label: "시뮬레이션 마모".into(),
                value: 0.25,
                limit: 0.2,
                unit: "mm".into(),
                severity: 2,
                passed: false,
                action: rule,
                message: "VB 한계 초과".into(),
            }],
            rule_action: rule,
            rule_action_label: rule.label().into(),
            rule_reasons: Vec::new(),
            feed_scale: None,
            speed_scale: None,
            coolant_suggestion: None,
            state_text: String::new(),
            advisor: None,
            advisor_action: None,
            advisor_error: None,
            advisor_allowed: false,
            final_action: rule,
            final_action_label: rule.label().into(),
            escalated_by_advisor: false,
            agreement: None,
            transition_to_replace: None,
            notes: Vec::new(),
            operator_choice: None,
            system_action: None,
            choice: None,
        }
    }

    #[test]
    fn the_operator_cannot_pick_below_the_gate() {
        let mut r = report(Action::InspectTool);
        let c = choice_prompt(&r, None, None);
        assert_eq!(c.options[0].key, "inspect_tool", "the gate decision is offered first");
        let cont = c.options.iter().find(|o| o.key == "continue").unwrap();
        assert!(!cont.enabled && cont.blocked_by.iter().any(|b| b.contains("시뮬레이션 마모")), "{:?}", cont);
        assert!(c.options.iter().filter(|o| o.enabled).all(|o| Action::from_key(&o.key).unwrap().conservativeness() >= 2));
        assert!(c.needs_attention, "an inspect-or-worse decision asks the operator");
        assert!(apply_operator_choice(&mut r, Action::Continue).is_err());
        apply_operator_choice(&mut r, Action::ReplaceTool).unwrap();
        assert_eq!((r.final_action, r.system_action, r.operator_choice), (Action::ReplaceTool, Some(Action::InspectTool), Some(Action::ReplaceTool)));
        let c2 = choice_prompt(&r, None, None);
        assert!(!c2.needs_attention && c2.options[0].key == "inspect_tool" && c2.options.iter().any(|o| o.key == "replace_tool" && o.chosen));
    }

    #[test]
    fn a_calm_cut_does_not_raise_a_notification() {
        let mut r = report(Action::Continue);
        r.gates[0].severity = 0;
        let c = choice_prompt(&r, None, None);
        assert!(!c.needs_attention && c.recommended == "continue");
        assert_eq!(c.enabled_count(), Action::all().len(), "every action is at least as conservative as continuing");
    }
}
