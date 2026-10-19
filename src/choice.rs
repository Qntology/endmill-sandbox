//! 사람이 고르는 결정 (선택 카드).
//!
//! 결정적 안전 게이트가 허용한 선택지만 버튼으로 보여 주고(권장을 맨 앞에), laya-typed-decisions 의 확률은 고르는 근거로만 붙인다.
//! laya 혼자서는 게이트 판단을 '공구 점검'까지만 올릴 수 있어 판단을 거의 바꾸지 못했다 — 대신 작업자가 laya 순위를 보며 고르고,
//! 그 선택을 SDS 원장에 작업자–laya · 작업자–규칙 일치로 남겨 laya 가 얻지 못하던 사람 판단 이력을 쌓는다.
//! 게이트가 막은 선택지는 막은 게이트 이름과 함께 비활성으로 남는다 (게이트가 결정권자, 모델은 조언자)
use serde::{Deserialize, Serialize};

/// 선택지 하나
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ChoiceOption {
    pub key: String,
    pub label: String,
    /// 이 선택이 실제로 하는 일 (한 줄)
    pub detail: String,
    /// 시스템 권장 (게이트 + laya 결합 판단)
    pub recommended: bool,
    /// 결정적 안전 게이트의 판단
    pub rule: bool,
    /// laya 확률 (모델을 썼을 때)
    pub advisor_probability: Option<f32>,
    pub advisor_top: bool,
    /// 고를 수 있는가 (게이트가 막지 않음)
    pub enabled: bool,
    /// 막은 게이트 (비활성일 때)
    pub blocked_by: Vec<String>,
    /// 작업자가 고른 선택지
    pub chosen: bool,
}

/// 선택 카드 하나 — 질문 · 선택지 · 사람이 봐야 하는 이유
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ChoicePrompt {
    /// "comp" (2차 보정) · "process" (공정 판단)
    pub kind: String,
    /// 짧은 머리표 (12자 안팎)
    pub header: String,
    pub question: String,
    /// 권장 → 고를 수 있는 나머지(laya 확률 순, 없으면 보수성 순) → 막힌 선택지
    pub options: Vec<ChoiceOption>,
    pub recommended: String,
    pub decided_by: String,
    /// 알림을 띄울지 — 작업자가 아직 고르지 않았고, 아래 이유가 하나라도 있으며, 고를 수 있는 선택지가 둘 이상
    pub needs_attention: bool,
    pub attention: Vec<String>,
    pub operator_choice: Option<String>,
    /// laya 상태 (사용 · 미사용 이유)
    pub advisor_note: String,
    /// (이름, 일치율, 건수) — 규칙–laya, 작업자–laya, 작업자–규칙
    pub agreement: Vec<(String, f64, u64)>,
}

impl ChoicePrompt {
    /// 선택지 순서: 권장 → 고를 수 있는 것(laya 확률 내림차순, 같으면 `rank` 오름차순) → 막힌 것
    pub fn sort_options(&mut self, rank: impl Fn(&str) -> u8) {
        self.options.sort_by(|a, b| {
            let tier = |o: &ChoiceOption| if o.recommended { 0 } else if o.enabled { 1 } else { 2 };
            tier(a)
                .cmp(&tier(b))
                .then_with(|| b.advisor_probability.unwrap_or(-1.0).partial_cmp(&a.advisor_probability.unwrap_or(-1.0)).unwrap_or(std::cmp::Ordering::Equal))
                .then_with(|| rank(&a.key).cmp(&rank(&b.key)))
        });
    }

    pub fn enabled_count(&self) -> usize {
        self.options.iter().filter(|o| o.enabled).count()
    }

    /// 사람이 봐야 하는 이유가 있고 실제로 고를 것이 있을 때만 알림
    pub fn settle_attention(&mut self) {
        self.needs_attention = self.operator_choice.is_none() && !self.attention.is_empty() && self.enabled_count() >= 2;
    }
}

/// 일치율 목록에 이력이 있는 축만 더한다
pub fn push_rate(v: &mut Vec<(String, f64, u64)>, label: &str, rate: Option<(f64, u64)>) {
    if let Some((r, n)) = rate {
        if n > 0 {
            v.push((label.to_string(), r, n));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opt(key: &str, rec: bool, enabled: bool, p: Option<f32>) -> ChoiceOption {
        ChoiceOption {
            key: key.into(),
            recommended: rec,
            enabled,
            advisor_probability: p,
            ..Default::default()
        }
    }

    #[test]
    fn recommended_comes_first_then_advisor_order_then_blocked() {
        let mut c = ChoicePrompt {
            options: vec![opt("a", false, false, Some(0.9)), opt("b", false, true, Some(0.1)), opt("c", true, true, Some(0.2)), opt("d", false, true, Some(0.6))],
            attention: vec!["x".into()],
            ..Default::default()
        };
        c.sort_options(|_| 0);
        let keys: Vec<&str> = c.options.iter().map(|o| o.key.as_str()).collect();
        assert_eq!(keys, ["c", "d", "b", "a"]);
        c.settle_attention();
        assert!(c.needs_attention);
        c.operator_choice = Some("d".into());
        c.settle_attention();
        assert!(!c.needs_attention, "once the operator picked, no more notification");
    }

    #[test]
    fn a_single_enabled_option_never_asks() {
        let mut c = ChoicePrompt {
            options: vec![opt("a", true, true, None), opt("b", false, false, None)],
            attention: vec!["x".into()],
            ..Default::default()
        };
        c.settle_attention();
        assert!(!c.needs_attention);
    }
}
