//! 측벽(주변 밀링) 표면조도 — 날마다 다른 유효 반경으로 그린 궤적의 하한 포락선.
//!
//! 예전 식 `Ra = (fz²/8R + h_min/2)/4 + TIR/4` 는 정삭 측벽에서 실측보다 한 자릿수 작았다: 55 HRC D12 6날 fz 0.1 mm 에서 실측 조도는
//! 기하 이송 자국(Rt 0.21 µm)의 19~38배였고, Rz 는 반경 변위 폭(4~6 µm)과 같았다 (Wojciechowski 2011/2016). 런아웃이 몇 µm 만
//! 되어도 한 날이 표면을 만들어 자국 간격이 회전당 이송이 된다 (Schmitz 외 2007). 그래서 날 j 의 유효 반경 r_j = R + Δ_j 로 각 날의
//! 원호를 그리고, 그 하한 포락선에서 Rt·Ra 를 수치로 구한다. 소재 바닥값·최소 칩 밀림·플랭크 마모·구성인선·채터는 실측 경향을 따른다.

use crate::pairref::WpGroup;
use serde::{Deserialize, Serialize};

/// 측정된 런아웃 중 표면을 만드는 날 반경 차로 남는 몫 — 굵은 칩을 받는 긴 날이 더 많이 휘어 정지 런아웃의 일부를 상쇄한다
/// (긴 날의 과삭은 e·cos(π/N) ~ e, 휨으로 조금 줄어 0.7~1.0 e — Schmitz 외 2007). 치수(과삭)와 조도에 같은 값을 쓴다
pub const RUNOUT_EFFECTIVE: f64 = 0.85;

/// 최소 칩 밀림(탄성 복원·쟁기질) 조도 항 Ra += k·h_min 의 k. Nowakowski 2023 (H11) 은 k = 0.37~0.4 이지만 그 h_min 은 조도에서 역산한
/// 0.3~1.7 µm 로, 이 모델의 역학 h_min (최소 칩 비 × 날끝 반경, 1~4 µm) 의 약 1/4 이다 — 그래서 0.4 × 0.25 = 0.1.
/// 소재 바닥값(최저 실측 정삭 Ra)에 보통 날끝의 밀림 몫이 이미 들어 있어 이 항은 날끝이 둥근(호닝 큰) 공구의 증가분에 가깝다. 근거 약함
pub const HMIN_RA_COEF: f64 = 0.1;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct WallRoughness {
    /// 기대 Ra (채터 확률로 가중) (µm)
    pub ra_um: f64,
    /// 안정 절삭일 때 Ra / 채터일 때 Ra (µm)
    pub ra_stable_um: f64,
    pub ra_chatter_um: f64,
    /// Rz 추정 (µm)
    pub rz_um: f64,
    /// 날 궤적 포락선의 Rt (런아웃 포함, 마모·소재 항 전) (µm)
    pub rt_envelope_um: f64,
    /// 런아웃이 없을 때의 이송 자국 Rt = fz²/8R (µm)
    pub rt_feed_um: f64,
    /// 표면을 실제로 만드는 날 수 (런아웃으로 짧은 날이 포락선 밖이거나 최소 칩 미만이면 빠짐)
    pub forming_teeth: u32,
    pub single_tooth: bool,
    /// 채터 파상도 (µm, 산-골) — 여유 < 1 일 때 h_max·(ap/ap_lim), 상한 3·h_max
    pub chatter_wpv_um: f64,
    pub chatter_probability: f64,
    pub floor_um: f64,
    pub hmin_term_um: f64,
    pub wear_factor: f64,
    pub bue_factor: f64,
}

/// 소재별 측벽 정삭 Ra 바닥값 (µm) — 최저 실측 정삭 Ra 에서 정한 값 (맞춤값 아님): 7075 0.10~0.15, 강·고경도 0.15~0.2,
/// Ti-6Al-4V ≈0.2, Inconel 718 ≈0.25, 316L ≥0.3. CFRP 는 섬유 뽑힘으로 1 µm 안팎 (자료 없음, 가정)
pub fn ra_floor_um(g: WpGroup) -> f64 {
    match g {
        WpGroup::Aluminum => 0.12,
        WpGroup::CarbonSteel | WpGroup::AlloySteel | WpGroup::Hardened => 0.18,
        WpGroup::Titanium => 0.20,
        WpGroup::Nickel => 0.25,
        WpGroup::Stainless => 0.30,
        WpGroup::Cfrp => 1.0,
    }
}

/// 플랭크 마모에 따른 Ra 배수. 4340 (43 HRC): VB 0.1 mm 까지 1.0, 0.4 까지 0.1 mm 당 +18~25 % (0.4 에서 ×1.74), 그 뒤 급증
/// (0.5 에서 ×3.5). Inconel 718 은 VB 0.25 부근 최소에서 0.1 mm 당 +10~13 %
pub fn wear_factor(g: WpGroup, vb_mm: f64) -> f64 {
    let vb = vb_mm.max(0.0);
    match g {
        WpGroup::Nickel => 1.0 + 1.2 * (vb - 0.25).max(0.0),
        _ => {
            if vb <= 0.4 {
                1.0 + 2.2 * (vb - 0.1).max(0.0)
            } else {
                1.66 + 18.0 * (vb - 0.4)
            }
        }
    }
    .min(6.0)
}

/// 측벽 조도.
/// `ecc_mm` 편심(TIR/2), `h_min_mm` 최소 칩 두께, `h_max_mm` 최대 칩 두께, `bue_risk` 구성인선 위험(0~0.85),
/// `margin` 채터 여유 (∞ = 안정), `chatter_p` 채터 확률
#[allow(clippy::too_many_arguments)]
pub fn wall_roughness(radius_mm: f64, flutes: u32, fz_mm: f64, ecc_mm: f64, h_min_mm: f64, h_max_mm: f64, vb_mm: f64, g: WpGroup, bue_risk: f64, margin: f64, chatter_p: f64) -> WallRoughness {
    let r = radius_mm.max(1e-3);
    let nt = flutes.max(1) as usize;
    let fz = fz_mm.max(1e-6);
    let pitch = 2.0 * std::f64::consts::PI / nt as f64;
    // 날 j 의 반경 차 (mm, + = 길다). 가장 긴 날 기준으로 각 날이 실제로 칩을 받는지(최소 칩 이상) 본다
    let d: Vec<f64> = (0..nt).map(|j| RUNOUT_EFFECTIVE * ecc_mm.max(0.0) * (j as f64 * pitch).cos()).collect();
    let cuts: Vec<bool> = (0..nt)
        .map(|j| {
            let prev = d[(j + nt - 1) % nt];
            fz + d[j] - prev >= h_min_mm.max(0.0) || nt == 1
        })
        .collect();
    let longest = (0..nt).max_by(|a, b| d[*a].partial_cmp(&d[*b]).unwrap_or(std::cmp::Ordering::Equal)).unwrap_or(0);
    let period = nt as f64 * fz;
    let samples = 240usize;
    let mut z = Vec::with_capacity(samples);
    let mut forming = vec![false; nt];
    for i in 0..samples {
        let x = period * (i as f64 + 0.5) / samples as f64;
        let mut best = f64::INFINITY;
        let mut who = longest;
        for (j, dj) in d.iter().enumerate() {
            if !cuts[j] && j != longest {
                continue;
            }
            for k in -1..=1 {
                let xj = j as f64 * fz + k as f64 * period;
                let v = (x - xj) * (x - xj) / (2.0 * r) - dj;
                if v < best {
                    best = v;
                    who = j;
                }
            }
        }
        forming[who] = true;
        z.push(best);
    }
    let zmax = z.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let zmin = z.iter().cloned().fold(f64::INFINITY, f64::min);
    let mean = z.iter().sum::<f64>() / samples as f64;
    let rt_env = (zmax - zmin) * 1000.0;
    let ra_env_profile = z.iter().map(|v| (v - mean).abs()).sum::<f64>() / samples as f64 * 1000.0;
    let forming_teeth = forming.iter().filter(|f| **f).count() as u32;
    let rt_feed = fz * fz / (8.0 * r) * 1000.0;
    let floor = ra_floor_um(g);
    let hmin_term = HMIN_RA_COEF * h_min_mm.max(0.0) * 1000.0;
    let wf = wear_factor(g, vb_mm);
    let bue = 1.0 + 1.2 * bue_risk.clamp(0.0, 0.85);
    let ra_stable = (ra_env_profile + hmin_term + floor) * wf * bue;
    // 채터: 공구가 절삭에서 이탈할 때까지 커지는 떨림 — 파상도 ≈ h_max·ρ (ρ = ap/ap_lim), 3·h_max 에서 포화 (Honeycutt & Schmitz 2018,
    // 아임계 발생). 떨림이 지배하면 Ra ≈ 0.20·Rt. 마모·구성인선 배수는 미세 조도(바닥값·최소 칩 밀림)에만 걸고 떨림 파형에는 걸지 않는다
    let micro = (hmin_term + floor) * wf * bue;
    let (wpv, ra_chat, rz_chat) = if margin.is_finite() && margin > 0.0 && margin < 1.0 {
        let rho = 1.0 / margin;
        let w = (h_max_mm.max(0.0) * rho).min(3.0 * h_max_mm.max(0.0)) * 1000.0;
        let rt = rt_env.max(w);
        (w, 0.20 * rt + micro, rt + 4.0 * micro)
    } else {
        (0.0, ra_stable, 0.0)
    };
    let p = if wpv > 0.0 { chatter_p.clamp(0.0, 1.0) } else { 0.0 };
    let ra = (1.0 - p) * ra_stable + p * ra_chat;
    // Rz/Ra (안정 절삭): 깨끗한 원호 ≈ 4, 런아웃 지배 ≈ 5, 구성인선 뭉개짐 ≈ 7. 떨림이면 Rz ≈ 파상도 산-골 + 미세 조도
    let single = forming_teeth <= 1 && nt > 1;
    let rz_ratio = if bue > 1.3 { 7.0 } else if single { 5.0 } else { 4.0 };
    let rz = (1.0 - p) * ra_stable * rz_ratio + p * rz_chat;
    WallRoughness {
        ra_um: ra,
        ra_stable_um: ra_stable,
        ra_chatter_um: ra_chat,
        rz_um: rz,
        rt_envelope_um: rt_env,
        rt_feed_um: rt_feed,
        forming_teeth,
        single_tooth: single,
        chatter_wpv_um: wpv,
        chatter_probability: p,
        floor_um: floor,
        hmin_term_um: hmin_term,
        wear_factor: wf,
        bue_factor: bue,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2날 반경 차 Δ: Δ < fz²/2R 이면 Rt = (fz + 2RΔ/fz)²/8R, 넘으면 한 날만 표면을 만들어 Rt = (2fz)²/8R (Schmitz 2007 전환과 일치)
    #[test]
    fn two_flute_envelope_matches_the_closed_form_and_switches_to_one_tooth() {
        let (r, fz) = (5.0, 0.1);
        for delta_um in [0.0, 0.3, 0.8] {
            let ecc = delta_um / 1000.0 / (2.0 * RUNOUT_EFFECTIVE);
            let w = wall_roughness(r, 2, fz, ecc, 0.0, 0.05, 0.0, WpGroup::CarbonSteel, 0.0, f64::INFINITY, 0.0);
            let dlt = delta_um / 1000.0;
            let expect = if dlt < fz * fz / (2.0 * r) { (fz + 2.0 * r * dlt / fz).powi(2) / (8.0 * r) } else { (2.0 * fz).powi(2) / (8.0 * r) } * 1000.0;
            assert!((w.rt_envelope_um - expect).abs() < 0.03 * expect + 0.005, "Δ {} µm: Rt {} vs {}", delta_um, w.rt_envelope_um, expect);
            assert_eq!(w.single_tooth, dlt >= fz * fz / (2.0 * r));
        }
        let big = wall_roughness(r, 2, fz, 0.005, 0.0, 0.05, 0.0, WpGroup::CarbonSteel, 0.0, f64::INFINITY, 0.0);
        assert!(big.single_tooth && (big.rt_envelope_um - (2.0 * fz).powi(2) / (8.0 * r) * 1000.0).abs() < 0.02);
    }

    #[test]
    fn wear_bue_and_chatter_raise_roughness() {
        let base = wall_roughness(4.0, 3, 0.05, 0.0015, 0.002, 0.03, 0.0, WpGroup::CarbonSteel, 0.0, f64::INFINITY, 0.0);
        let worn = wall_roughness(4.0, 3, 0.05, 0.0015, 0.002, 0.03, 0.4, WpGroup::CarbonSteel, 0.0, f64::INFINITY, 0.0);
        assert!((worn.ra_um / base.ra_um - 1.66).abs() < 0.02, "{}", worn.ra_um / base.ra_um);
        let bue = wall_roughness(4.0, 3, 0.05, 0.0015, 0.002, 0.03, 0.0, WpGroup::Aluminum, 0.6, f64::INFINITY, 0.0);
        assert!(bue.bue_factor > 1.6);
        let chat = wall_roughness(4.0, 3, 0.05, 0.0015, 0.002, 0.03, 0.0, WpGroup::CarbonSteel, 0.0, 0.5, 1.0);
        assert!((chat.chatter_wpv_um - 60.0).abs() < 1e-6 && chat.ra_um > 5.0 * base.ra_um, "{:?}", chat);
        // 떨림이 지배하면 Rz 는 파상도 산-골(60 µm) + 미세 조도, 구성인선 배수는 떨림 파형에 걸리지 않는다
        assert!(chat.rz_um > 60.0 && chat.rz_um < 63.0, "{:?}", chat);
        let chat_bue = wall_roughness(4.0, 3, 0.05, 0.0015, 0.002, 0.03, 0.0, WpGroup::CarbonSteel, 0.6, 0.5, 1.0);
        assert!(chat_bue.ra_um - chat.ra_um < 1.0, "{} vs {}", chat_bue.ra_um, chat.ra_um);
        let deep = wall_roughness(4.0, 3, 0.05, 0.0015, 0.002, 0.03, 0.0, WpGroup::CarbonSteel, 0.0, 0.1, 1.0);
        assert!((deep.chatter_wpv_um - 90.0).abs() < 1e-6, "포화 3·h_max");
    }
}
