//! 2D 평면변형 직교절삭 MLS-MPM(APIC) 해석기.
//!
//! 앱의 해석 모델(Kienzle·Boothroyd 계열)을 **가상으로 교차검증**하기 위한 연구용 연속체 모델이다.
//! 실험으로 검증된 모델이 아니며, 결과는 "다른 가정으로 푼 두 번째 계산"으로만 읽어야 한다.
//!
//! 모델
//! - 재료: Hencky 탄소성 + von Mises 반환사상(평면변형, 면외 탄성 로그변형률 `e3` 를 입자 상태로 저장),
//!   Johnson–Cook 유동응력(변형률·변형률속도·열연화), 소성일의 `beta` 비율이 열로 변환(단열 가열).
//! - 열: 격자 확산(열전도율 `k`). 공구는 단열(열전달 없음)로 둔다.
//! - 공구: 강체 쐐기(경사각·여유각) + 날 끝 둥글기(곡률 중심 오프셋 SDF). 격자 노드 기반 쿨롱 마찰 접촉.
//! - 시간 간격: 탄성계수를 낮추지 않고 **질량 스케일링**(밀도 × `mscale`)으로 키운다. 준정적 여부는
//!   `ke_ratio_max`(운동에너지/누적 절삭일)로 확인한다.
//! - 한계: 2D 평면변형 · 균열/톱니형 칩 없음 · 공구 열전도/마모 없음 · 냉각유 없음 · 격자 4 µm 근방 해상도.
//!   티타늄·인코넬처럼 톱니형 칩이 생기는 재료는 신뢰도가 낮다.

use rayon::prelude::*;
use std::time::Instant;

pub const PPC: usize = 2;

#[derive(Debug, Clone)]
pub struct Material {
    pub name: String,
    pub rho: f64,
    pub e_pa: f64,
    pub nu: f64,
    pub cp: f64,
    pub k: f64,
    pub jc_a: f64,
    pub jc_b: f64,
    pub jc_n: f64,
    pub jc_c: f64,
    pub jc_m: f64,
    pub rate_ref: f64,
    pub t_room_c: f64,
    pub t_melt_c: f64,
    pub beta: f64,
}

impl Material {
    /// 앱의 SUS304 상수(JC 310/1000/0.65/0.07/1.0, Tm 1400 °C)와 같은 값.
    pub fn sus304() -> Self {
        Self {
            name: "SUS304".into(),
            rho: 8000.0,
            e_pa: 193e9,
            nu: 0.30,
            cp: 500.0,
            k: 16.2,
            jc_a: 310e6,
            jc_b: 1000e6,
            jc_n: 0.65,
            jc_c: 0.07,
            jc_m: 1.0,
            rate_ref: 1.0,
            t_room_c: 20.0,
            t_melt_c: 1400.0,
            beta: 0.9,
        }
    }
}

/// 해석 품질 단계. 빠름은 표준보다 격자가 거칠다(칩 두께 10칸 대 12.5칸, 질량 스케일 2배).
/// 기준 조건(SUS304, h=50 µm, 기본 이동거리)에서 빠름은 표준 대비 절삭력 -3%, 접촉면 온도 +10%, 칩 두께비 +6% 차이
/// (표준 194 N/mm · 782 °C · r 0.35 / 빠름 187 N/mm · 860 °C · r 0.37). 추력은 날 둥글기 해상도에 따라 더 흔들린다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    Quick,
    Standard,
}

impl Preset {
    pub fn key(self) -> &'static str {
        match self {
            Preset::Quick => "quick",
            Preset::Standard => "standard",
        }
    }

    pub fn from_key(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "quick" | "fast" | "빠름" => Some(Preset::Quick),
            "standard" | "std" | "표준" => Some(Preset::Standard),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CutParams {
    pub material: Material,
    pub h_um: f64,
    pub vc_m_s: f64,
    pub rake_deg: f64,
    pub clear_deg: f64,
    pub mu: f64,
    pub edge_um: f64,
    pub dx_um: f64,
    pub mscale: f64,
    /// 0 이면 자동: max(300, 4.5·h + 75)
    pub travel_um: f64,
    /// 0 이면 자동: travel + 150
    pub wp_len_um: f64,
    /// 0 이면 자동: max(200, 3·h + 50)
    pub wp_thick_um: f64,
    pub tool_x0_um: f64,
    pub cfl: f64,
    /// 0 이면 끔. 이 소성변형률을 넘으면 편차응력을 98% 잃는다(유동화).
    pub eps_fail: f64,
    pub conduction: bool,
    pub threads: usize,
    pub max_steps: usize,
    pub keep_particles: bool,
}

impl CutParams {
    pub fn new(material: Material) -> Self {
        Self {
            material,
            h_um: 50.0,
            vc_m_s: 2.2,
            rake_deg: 12.0,
            clear_deg: 7.0,
            mu: 0.4,
            edge_um: 8.0,
            dx_um: 4.0,
            mscale: 200.0,
            travel_um: 0.0,
            wp_len_um: 0.0,
            wp_thick_um: 0.0,
            tool_x0_um: 92.0,
            cfl: 0.3,
            eps_fail: 0.0,
            conduction: true,
            threads: 1,
            max_steps: 0,
            keep_particles: false,
        }
    }

    /// 빠른 설정: 격자 5 µm, 질량 ×400. 표준(4 µm, ×200)보다 대략 3배 빠르다.
    pub fn quick(material: Material) -> Self {
        let mut p = Self::new(material);
        p.dx_um = 5.0;
        p.mscale = 400.0;
        p
    }

    /// 절삭 조건에서 격자와 질량 스케일을 정한다.
    /// - 격자: 칩 두께 h 를 표준은 12.5칸, 빠름은 10칸으로 나누도록 잡되 표준 2.5~6 µm, 빠름 3~8 µm 로 제한한다.
    /// - 질량 스케일: 운동에너지 비(∝ 질량배율 × 속도²)를 기준 조건(2.2 m/s)과 같게 두도록 속도의 제곱에 반비례시킨다.
    ///   이 비례에서는 속도와 관계없이 시간 단계 수가 거의 같다. 5~2000 으로 제한한다.
    pub fn for_conditions(material: Material, preset: Preset, h_um: f64, vc_m_s: f64, rake_deg: f64, clear_deg: f64, mu: f64, edge_um: f64) -> Self {
        let mut p = Self::new(material);
        p.h_um = h_um;
        p.vc_m_s = vc_m_s;
        p.rake_deg = rake_deg;
        p.clear_deg = clear_deg;
        p.mu = mu;
        p.edge_um = edge_um;
        let (cells, lo, hi, ms0) = match preset {
            Preset::Standard => (12.5, 2.5, 6.0, 200.0),
            Preset::Quick => (10.0, 3.0, 8.0, 400.0),
        };
        p.dx_um = (h_um / cells).clamp(lo, hi);
        let v = vc_m_s.max(0.05);
        p.mscale = (ms0 * (2.2 / v) * (2.2 / v)).clamp(5.0, 2000.0);
        p
    }

    fn validate(&self) -> Result<(), String> {
        let bad = |name: &str, v: f64, lo: f64, hi: f64| -> Result<(), String> {
            if !(v.is_finite() && v >= lo && v <= hi) {
                Err(format!("{} = {} 는 허용 범위 [{}, {}] 밖입니다", name, v, lo, hi))
            } else {
                Ok(())
            }
        };
        bad("h_um", self.h_um, 5.0, 400.0)?;
        bad("vc_m_s", self.vc_m_s, 0.05, 30.0)?;
        bad("rake_deg", self.rake_deg, -20.0, 35.0)?;
        bad("clear_deg", self.clear_deg, 2.0, 20.0)?;
        bad("mu", self.mu, 0.0, 1.2)?;
        bad("edge_um", self.edge_um, 0.0, 60.0)?;
        bad("dx_um", self.dx_um, 1.5, 10.0)?;
        bad("mscale", self.mscale, 1.0, 5000.0)?;
        bad("cfl", self.cfl, 0.05, 0.6)?;
        let m = &self.material;
        bad("rho", m.rho, 500.0, 25000.0)?;
        bad("e_pa", m.e_pa, 1e9, 1e12)?;
        bad("nu", m.nu, 0.1, 0.45)?;
        bad("cp", m.cp, 100.0, 5000.0)?;
        bad("k", m.k, 0.1, 500.0)?;
        bad("jc_a", m.jc_a, 1e6, 5e9)?;
        bad("jc_b", m.jc_b, 0.0, 5e9)?;
        bad("jc_n", m.jc_n, 0.01, 1.5)?;
        bad("jc_c", m.jc_c, 0.0, 0.5)?;
        bad("jc_m", m.jc_m, 0.1, 3.0)?;
        bad("t_melt_c", m.t_melt_c, 300.0, 3500.0)?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
pub struct HistRow {
    pub travel_um: f64,
    pub fc: f64,
    pub ft: f64,
    pub tmax_c: f64,
}

#[derive(Debug, Clone, Default)]
pub struct CutResult {
    pub steps: usize,
    pub particles: usize,
    pub wall_s: f64,
    pub dt_s: f64,
    pub fc_n_per_mm: f64,
    pub fc_sd: f64,
    pub ft_n_per_mm: f64,
    pub tmax_c: f64,
    pub chip_t_mean_c: f64,
    pub chip_t_p95_c: f64,
    pub interface_t_mean_c: f64,
    pub interface_t_p95_c: f64,
    pub chip_ep_mean: f64,
    /// 칩 두께 (µm) — `chip_ratio` 와 같은 기준 (h / chip_ratio)
    pub chip_thickness_um: f64,
    /// 칩 두께비 r = h/h_c. 정상 구간의 칩 속도(공구 기준, 경사면 방향)로 질량 보존 r = V_chip / V_c 에서 구한다.
    /// 속도 표본이 부족하면 단면 두께 측정값을 쓴다.
    pub chip_ratio: f64,
    /// 칩 단면 두께를 직접 재서 구한 두께비 (칩이 말리거나 공구에 붙으면 흔들린다) — 비교용
    pub chip_ratio_geom: f64,
    /// 공구 기준 칩 속도 (경사면 방향, m/s, 정상 구간 중앙값)
    pub chip_velocity_m_s: f64,
    /// 칩 속도 표본(기록 시점) 수
    pub chip_samples: usize,
    pub shear_angle_deg: f64,
    pub mu_apparent: f64,
    pub tau_shear_mpa: f64,
    pub wp_over_wtool: f64,
    pub ke_ratio_max: f64,
    /// 꼬리 구간 뒤 절반 평균 / 앞 절반 평균 - 1 (절삭력 추세)
    pub fc_trend: f64,
    /// 칩이 계산 영역 천장 근처까지 올라갔는지 (그러면 칩이 쌓여 힘이 커진다)
    pub chip_at_ceiling: bool,
    /// 힘이 안정 구간에 들어갔는지(꼬리 구간 표준편차/평균 < 12%, 앞뒤 절반 차이 < 8%)
    pub steady: bool,
    pub cancelled: bool,
    pub history: Vec<HistRow>,
    /// keep_particles 일 때만: [x_um, y_um, ep, T_C, p_GPa, vx_m_s, vy_m_s, x0_um, y0_um]
    pub particles_dump: Vec<[f32; 9]>,
    /// 해석이 끝난 시점의 날 끝 위치 (µm) — 입자 덤프를 공구 기준으로 볼 때 쓴다
    pub tool_tip_x_um: f64,
    pub tool_tip_y_um: f64,
    pub notes: Vec<String>,
}

fn jnum(v: f64) -> String {
    if v.is_finite() {
        format!("{:.6}", v)
    } else {
        "null".into()
    }
}

impl CutResult {
    pub fn to_json(&self) -> String {
        let mut s = String::from("{");
        let f = |k: &str, v: f64| format!("\"{}\":{}", k, jnum(v));
        let parts = [
            format!("\"steps\":{}", self.steps),
            format!("\"particles\":{}", self.particles),
            f("wall_s", self.wall_s),
            f("dt_ns", self.dt_s * 1e9),
            f("fc_n_per_mm", self.fc_n_per_mm),
            f("fc_sd", self.fc_sd),
            f("ft_n_per_mm", self.ft_n_per_mm),
            f("tmax_c", self.tmax_c),
            f("chip_t_mean_c", self.chip_t_mean_c),
            f("chip_t_p95_c", self.chip_t_p95_c),
            f("interface_t_mean_c", self.interface_t_mean_c),
            f("interface_t_p95_c", self.interface_t_p95_c),
            f("chip_ep_mean", self.chip_ep_mean),
            f("chip_thickness_um", self.chip_thickness_um),
            f("chip_ratio", self.chip_ratio),
            f("chip_ratio_geom", self.chip_ratio_geom),
            f("tool_tip_x_um", self.tool_tip_x_um),
            f("tool_tip_y_um", self.tool_tip_y_um),
            f("chip_velocity_m_s", self.chip_velocity_m_s),
            format!("\"chip_samples\":{}", self.chip_samples),
            f("shear_angle_deg", self.shear_angle_deg),
            f("mu_apparent", self.mu_apparent),
            f("tau_shear_mpa", self.tau_shear_mpa),
            f("wp_over_wtool", self.wp_over_wtool),
            f("ke_ratio_max", self.ke_ratio_max),
            f("fc_trend", self.fc_trend),
            format!("\"chip_at_ceiling\":{}", self.chip_at_ceiling),
            format!("\"steady\":{}", self.steady),
            format!("\"cancelled\":{}", self.cancelled),
        ];
        s.push_str(&parts.join(","));
        s.push_str(",\"history\":[");
        for (i, h) in self.history.iter().enumerate() {
            if i > 0 {
                s.push(',');
            }
            s.push_str(&format!("[{},{},{},{}]", jnum(h.travel_um), jnum(h.fc), jnum(h.ft), jnum(h.tmax_c)));
        }
        s.push_str("],\"notes\":[");
        for (i, n) in self.notes.iter().enumerate() {
            if i > 0 {
                s.push(',');
            }
            s.push_str(&format!("\"{}\"", n.replace('\\', "\\\\").replace('"', "\\\"")));
        }
        s.push_str("]}");
        s
    }
}

// ---------------------------------------------------------------- 2x2 선형대수

type M2 = [f64; 4];

#[inline]
fn rot(a: f64) -> M2 {
    let (s, c) = a.sin_cos();
    [c, -s, s, c]
}

#[inline]
fn mul2(a: M2, b: M2) -> M2 {
    [a[0] * b[0] + a[1] * b[2], a[0] * b[1] + a[1] * b[3], a[2] * b[0] + a[3] * b[2], a[2] * b[1] + a[3] * b[3]]
}

/// F = U · diag(s) · Vt (닫힌 형식). U, Vt 는 회전행렬이고 s[1] 은 det<0 이면 음수가 된다.
#[inline]
pub fn svd2(m: [f64; 4]) -> ([f64; 4], [f64; 2], [f64; 4]) {
    let e = (m[0] + m[3]) * 0.5;
    let f = (m[0] - m[3]) * 0.5;
    let g = (m[2] + m[1]) * 0.5;
    let h = (m[2] - m[1]) * 0.5;
    let q = (e * e + h * h).sqrt();
    let r = (f * f + g * g).sqrt();
    let a1 = g.atan2(f);
    let a2 = h.atan2(e);
    let theta = (a2 - a1) * 0.5;
    let phi = (a2 + a1) * 0.5;
    (rot(phi), [q + r, q - r], rot(theta))
}

// ---------------------------------------------------------------- 상태

#[derive(Clone, Copy, Default)]
struct P {
    x: [f64; 2],
    v: [f64; 2],
    c: M2,
    f: M2,
    e3: f64,
    ep: f64,
    epd: f64,
    t: f64,
}

struct K {
    dx: f64,
    inv_dx: f64,
    nx: usize,
    ny: usize,
    mu_l: f64,
    lam_l: f64,
    rho: f64,
    cp: f64,
    beta: f64,
    p_vol: f64,
    p_mass: f64,
    jc_a: f64,
    jc_b: f64,
    jc_n: f64,
    jc_c: f64,
    jc_m: f64,
    rate_ref: f64,
    t0: f64,
    tm: f64,
    eps_fail: f64,
    v_tool: f64,
    mu_fric: f64,
    cr: f64,
    sr: f64,
    ca: f64,
    sa: f64,
    r_edge: f64,
    clamp_y: f64,
    alpha: f64,
    cond: bool,
}

#[inline]
fn yield_stress(k: &K, ep: f64, e_dot: f64, t: f64) -> f64 {
    let ed = e_dot.max(k.rate_ref);
    let ts = ((t - k.t0) / (k.tm - k.t0)).clamp(0.0, 0.999);
    (k.jc_a + k.jc_b * ep.max(0.0).powf(k.jc_n)) * (1.0 + k.jc_c * (ed / k.rate_ref).ln()) * (1.0 - ts.powf(k.jc_m))
}

/// 강체 쐐기(꼭짓점 = 곡률 중심)의 부호 거리(밖이 +)와 바깥 법선.
#[inline]
fn tool_phi_n(k: &K, px: f64, py: f64, tx: f64, ty: f64) -> (f64, [f64; 2]) {
    let rx = px - tx;
    let ry = py - ty;
    let d1 = -(rx * k.sa + ry * k.ca);
    let d2 = rx * k.cr + ry * k.sr;
    let s1 = rx * (-k.ca) + ry * k.sa;
    let s2 = rx * (-k.sr) + ry * k.cr;
    if d1 <= 0.0 && d2 <= 0.0 {
        if d1 > d2 {
            (d1, [-k.sa, -k.ca])
        } else {
            (d2, [k.cr, k.sr])
        }
    } else {
        let r = (rx * rx + ry * ry).sqrt() + 1e-30;
        let mut phi = r;
        let mut n = [rx / r, ry / r];
        if s1 >= 0.0 && d1 > 0.0 && d1 < phi {
            phi = d1;
            n = [-k.sa, -k.ca];
        }
        if s2 >= 0.0 && d2 > 0.0 && d2 < phi {
            phi = d2;
            n = [k.cr, k.sr];
        }
        (phi, n)
    }
}

#[inline]
fn weights(k: &K, x: [f64; 2]) -> (usize, usize, [f64; 2], [f64; 3], [f64; 3]) {
    let gx = x[0] * k.inv_dx - 0.5;
    let gy = x[1] * k.inv_dx - 0.5;
    let bx = (gx.floor() as isize).clamp(0, k.nx as isize - 3) as usize;
    let by = (gy.floor() as isize).clamp(0, k.ny as isize - 3) as usize;
    let fx = x[0] * k.inv_dx - bx as f64;
    let fy = x[1] * k.inv_dx - by as f64;
    let wx = [0.5 * (1.5 - fx) * (1.5 - fx), 0.75 - (fx - 1.0) * (fx - 1.0), 0.5 * (fx - 0.5) * (fx - 0.5)];
    let wy = [0.5 * (1.5 - fy) * (1.5 - fy), 0.75 - (fy - 1.0) * (fy - 1.0), 0.5 * (fy - 0.5) * (fy - 0.5)];
    (bx, by, [fx, fy], wx, wy)
}

/// 평면변형 Hencky 탄소성 + von Mises 반환사상. (Kirchhoff 응력 τ 의 2×2 행렬, 소성일 증가량)을 돌려준다.
#[inline]
fn stress_update(k: &K, dt: f64, p: &mut P) -> (M2, f64) {
    let (u, s, vt) = svd2(p.f);
    let mut e1 = s[0].max(1e-6).ln();
    let mut e2 = s[1].max(1e-6).ln();
    let mut e3 = p.e3;
    let mean = (e1 + e2 + e3) / 3.0;
    let mut d1 = e1 - mean;
    let mut d2 = e2 - mean;
    let mut d3 = e3 - mean;
    let dn = (d1 * d1 + d2 * d2 + d3 * d3).sqrt();
    let q = 6f64.sqrt() * k.mu_l * dn;
    let mut sy = yield_stress(k, p.ep, p.epd, p.t);
    if k.eps_fail > 0.0 && p.ep > k.eps_fail {
        sy *= 0.02;
    }
    let mut wp = 0.0;
    if q > sy && dn > 1e-30 {
        let de = (q - sy) / (3.0 * k.mu_l);
        let sc = sy / q;
        d1 *= sc;
        d2 *= sc;
        d3 *= sc;
        p.ep += de;
        p.epd = 0.8 * p.epd + 0.2 * (de / dt);
        p.t += k.beta * sy * de / (k.rho * k.cp);
        wp = sy * de * k.p_vol;
        e1 = d1 + mean;
        e2 = d2 + mean;
        e3 = d3 + mean;
        p.e3 = e3;
        p.f = mul2(mul2(u, [e1.exp(), 0.0, 0.0, e2.exp()]), vt);
    } else {
        p.epd *= 0.8;
    }
    let tr = e1 + e2 + e3;
    let t1 = k.lam_l * tr + 2.0 * k.mu_l * e1;
    let t2 = k.lam_l * tr + 2.0 * k.mu_l * e2;
    let (c, sn) = (u[0], u[2]);
    let tau = [t1 * c * c + t2 * sn * sn, (t1 - t2) * c * sn, (t1 - t2) * c * sn, t1 * sn * sn + t2 * c * c];
    (tau, wp)
}

fn p2g_chunk(k: &K, dt: f64, parts: &mut [P], buf: &mut [[f64; 4]]) -> f64 {
    for b in buf.iter_mut() {
        *b = [0.0; 4];
    }
    let mut wp_sum = 0.0;
    let ny = k.ny;
    let coef = -dt * k.p_vol * 4.0 * k.inv_dx * k.inv_dx;
    for p in parts.iter_mut() {
        let (tau, wp) = stress_update(k, dt, p);
        wp_sum += wp;
        let (bx, by, fxy, wx, wy) = weights(k, p.x);
        let aff = [coef * tau[0] + k.p_mass * p.c[0], coef * tau[1] + k.p_mass * p.c[1], coef * tau[2] + k.p_mass * p.c[2], coef * tau[3] + k.p_mass * p.c[3]];
        let (mvx0, mvy0) = (k.p_mass * p.v[0], k.p_mass * p.v[1]);
        let mt = k.p_mass * p.t;
        for a in 0..3 {
            let dpx = (a as f64 - fxy[0]) * k.dx;
            for b in 0..3 {
                let dpy = (b as f64 - fxy[1]) * k.dx;
                let wt = wx[a] * wy[b];
                let e = &mut buf[(bx + a) * ny + by + b];
                e[0] += wt * k.p_mass;
                e[1] += wt * (mvx0 + aff[0] * dpx + aff[1] * dpy);
                e[2] += wt * (mvy0 + aff[2] * dpx + aff[3] * dpy);
                e[3] += wt * mt;
            }
        }
    }
    wp_sum
}

fn g2p_chunk(k: &K, dt: f64, parts: &mut [P], out: &[[f64; 3]]) {
    let ny = k.ny;
    for p in parts.iter_mut() {
        let (bx, by, fxy, wx, wy) = weights(k, p.x);
        let mut nv = [0.0; 2];
        let mut nc = [0.0; 4];
        let mut dtp = 0.0;
        for a in 0..3 {
            let dposx = a as f64 - fxy[0];
            for b in 0..3 {
                let dposy = b as f64 - fxy[1];
                let wt = wx[a] * wy[b];
                let o = out[(bx + a) * ny + by + b];
                nv[0] += wt * o[0];
                nv[1] += wt * o[1];
                let sc = 4.0 * k.inv_dx * wt;
                nc[0] += sc * o[0] * dposx;
                nc[1] += sc * o[0] * dposy;
                nc[2] += sc * o[1] * dposx;
                nc[3] += sc * o[1] * dposy;
                dtp += wt * o[2];
            }
        }
        p.v = nv;
        p.c = nc;
        p.x[0] += dt * nv[0];
        p.x[1] += dt * nv[1];
        p.f = mul2([1.0 + dt * nc[0], dt * nc[1], dt * nc[2], 1.0 + dt * nc[3]], p.f);
        p.t += dtp;
    }
}

struct Sim {
    k: K,
    parts: Vec<P>,
    x0: Vec<[f64; 2]>,
    bufs: Vec<Vec<[f64; 4]>>,
    chunk: usize,
    nodes: Vec<[f64; 4]>,
    out: Vec<[f64; 3]>,
    node_chunk: usize,
}

struct StepOut {
    wp: f64,
    fx: f64,
    fy: f64,
    nc: usize,
}

impl Sim {
    fn step(&mut self, dt: f64, tx: f64, ty: f64) -> StepOut {
        let k = &self.k;
        let chunk = self.chunk;
        // 1) P2G (입자별 응력 갱신 포함)
        let wps: Vec<f64> = self
            .parts
            .par_chunks_mut(chunk)
            .zip(self.bufs.par_iter_mut())
            .map(|(pc, buf)| p2g_chunk(k, dt, pc, buf))
            .collect();
        // 2) 청크별 버퍼 합산 + 정규화
        let bufs = &self.bufs;
        let ncb = self.node_chunk;
        self.nodes.par_chunks_mut(ncb).enumerate().for_each(|(ci, nc)| {
            let base = ci * ncb;
            for (kk, nd) in nc.iter_mut().enumerate() {
                let idx = base + kk;
                let mut s = [0.0f64; 4];
                for b in bufs.iter() {
                    let e = b[idx];
                    s[0] += e[0];
                    s[1] += e[1];
                    s[2] += e[2];
                    s[3] += e[3];
                }
                if s[0] > 0.0 {
                    *nd = [s[0], s[1] / s[0], s[2] / s[0], s[3] / s[0]];
                } else {
                    *nd = [0.0; 4];
                }
            }
        });
        // 3) 격자 연산: 경계 + 공구 접촉 + 열전도
        let nodes = &self.nodes;
        let (nx, ny) = (k.nx, k.ny);
        let reds: Vec<(f64, f64, usize)> = self
            .out
            .par_chunks_mut(ncb)
            .enumerate()
            .map(|(ci, oc)| {
                let base = ci * ncb;
                let (mut fx, mut fy, mut cnt) = (0.0, 0.0, 0usize);
                for (kk, o) in oc.iter_mut().enumerate() {
                    let idx = base + kk;
                    let nd = nodes[idx];
                    if nd[0] <= 0.0 {
                        *o = [0.0; 3];
                        continue;
                    }
                    let (i, j) = (idx / ny, idx % ny);
                    let (mut vx, mut vy) = (nd[1], nd[2]);
                    let px = i as f64 * k.dx;
                    let py = j as f64 * k.dx;
                    if py < k.clamp_y || i < 3 || i + 4 > nx || j < 3 || j + 4 > ny {
                        vx = 0.0;
                        vy = 0.0;
                    } else {
                        let (mut phi, n) = tool_phi_n(k, px, py, tx, ty);
                        phi -= k.r_edge;
                        if phi < 0.0 {
                            let (rvx, rvy) = (vx - k.v_tool, vy);
                            let vn = rvx * n[0] + rvy * n[1];
                            if vn < 0.0 {
                                let (tvx, tvy) = (rvx - vn * n[0], rvy - vn * n[1]);
                                let nt = (tvx * tvx + tvy * tvy).sqrt() + 1e-30;
                                let fac = (1.0 + k.mu_fric * vn / nt).max(0.0);
                                let (nvx, nvy) = (k.v_tool + tvx * fac, tvy * fac);
                                let (dpx, dpy) = (nd[0] * (nvx - vx), nd[0] * (nvy - vy));
                                fx += -dpx / dt;
                                fy += -dpy / dt;
                                cnt += 1;
                                vx = nvx;
                                vy = nvy;
                            }
                        }
                    }
                    let mut dtg = 0.0;
                    if k.cond && i > 0 && i + 1 < nx && j > 0 && j + 1 < ny {
                        let mut lap = 0.0;
                        for nb in [idx + ny, idx - ny, idx + 1, idx - 1] {
                            let q = nodes[nb];
                            if q[0] > 0.0 {
                                lap += q[3] - nd[3];
                            }
                        }
                        dtg = k.alpha * dt * lap * k.inv_dx * k.inv_dx;
                    }
                    *o = [vx, vy, dtg];
                }
                (fx, fy, cnt)
            })
            .collect();
        // 4) G2P
        let out = &self.out;
        self.parts.par_chunks_mut(chunk).for_each(|pc| g2p_chunk(k, dt, pc, out));
        let mut so = StepOut { wp: wps.iter().sum(), fx: 0.0, fy: 0.0, nc: 0 };
        for r in reds {
            so.fx += r.0;
            so.fy += r.1;
            so.nc += r.2;
        }
        so
    }
}

fn percentile(v: &mut Vec<f64>, p: f64) -> f64 {
    if v.is_empty() {
        return f64::NAN;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let pos = (p * (v.len() - 1) as f64).clamp(0.0, (v.len() - 1) as f64);
    let (lo, hi) = (pos.floor() as usize, pos.ceil() as usize);
    v[lo] + (v[hi] - v[lo]) * (pos - lo as f64)
}

/// 격자·시간 단계 계산 결과. `plan` 과 `run` 이 같은 값을 쓴다.
struct Geo {
    rho_m: f64,
    mu_l: f64,
    lam_l: f64,
    dx: f64,
    h_m: f64,
    travel_um: f64,
    wp_len_um: f64,
    wp_thick_um: f64,
    x_wp0: f64,
    y_wp0: f64,
    y_top: f64,
    nx: usize,
    ny: usize,
    nxp: usize,
    nyp: usize,
    n_p: usize,
    p_vol: f64,
    dt: f64,
    n_steps: usize,
}

fn geometry(prm: &CutParams) -> Result<Geo, String> {
    prm.validate()?;
    let m = &prm.material;
    let rho_m = m.rho * prm.mscale;
    let mu_l = m.e_pa / (2.0 * (1.0 + m.nu));
    let lam_l = m.e_pa * m.nu / ((1.0 + m.nu) * (1.0 - 2.0 * m.nu));
    let c_p = ((lam_l + 2.0 * mu_l) / rho_m).sqrt();
    let dx = prm.dx_um * 1e-6;
    let h_m = prm.h_um * 1e-6;
    // 이동거리·공작물 길이: 칩이 두껍게 나오는 재료(SUS304, 전단각 13~25°)는 전단면이 날 끝 앞쪽으로 h/tanφ ≈ 4h 까지
    // 뻗는다. 예전 기본값(이동 max(300, 4.5h+75) µm, 끝 여유 150 µm)은 h=50 µm 에서 힘이 아직 오르는 구간과
    // 공작물 끝 효과가 겹쳐 절삭력을 15~20% 낮게 냈다 (이동 1000 µm 해석에서 300~600 µm 구간 약 190 N/mm 로 수렴,
    // 예전 평균 157 N/mm). 이동은 8h+100 µm 이상, 끝 여유는 6h 이상으로 둔다.
    let travel_um = if prm.travel_um > 0.0 { prm.travel_um } else { (8.0 * prm.h_um + 100.0).max(300.0) };
    let wp_len_um = if prm.wp_len_um > 0.0 { prm.wp_len_um } else { travel_um + (6.0 * prm.h_um).max(150.0) };
    let wp_thick_um = if prm.wp_thick_um > 0.0 { prm.wp_thick_um } else { (3.0 * prm.h_um + 50.0).max(200.0) };
    if prm.edge_um + 8.0 > wp_thick_um {
        return Err("작업물 두께가 너무 얇습니다".into());
    }
    let x_wp0 = 100e-6;
    let y_wp0 = 40e-6;
    let x_wp1 = x_wp0 + wp_len_um * 1e-6;
    let y_top = y_wp0 + wp_thick_um * 1e-6;
    let l_dom = (wp_len_um + 300.0) * 1e-6;
    // 공작물 위 여유: 칩이 계산 영역 천장에 닿으면 빠져나가지 못하고 쌓여 힘이 계속 오른다 (이동 1000 µm 해석에서
    // 700 µm 이후 확인). 칩이 자라는 만큼(이동의 절반 + 4h) 이상 비워 둔다.
    let space_um = (0.5 * travel_um + 4.0 * prm.h_um).max(260.0);
    let h_dom = (wp_thick_um + 80.0 + space_um) * 1e-6;
    let nx = (l_dom / dx).round() as usize + 8;
    let ny = (h_dom / dx).round() as usize + 8;
    let nxp = ((x_wp1 - x_wp0) / dx).round() as usize * PPC;
    let nyp = ((y_top - y_wp0) / dx).round() as usize * PPC;
    let n_p = nxp * nyp;
    if n_p == 0 || n_p > 400_000 {
        return Err(format!("입자 수 {} 가 범위를 벗어났습니다", n_p));
    }
    let p_vol = (dx / PPC as f64).powi(2);
    let dt = prm.cfl * dx / (c_p + prm.vc_m_s);
    let travel = travel_um * 1e-6;
    let mut n_steps = (travel / (prm.vc_m_s * dt)) as usize;
    if prm.max_steps > 0 {
        n_steps = n_steps.min(prm.max_steps);
    }
    if n_steps < 50 {
        return Err("시간 단계가 너무 적습니다".into());
    }
    Ok(Geo { rho_m, mu_l, lam_l, dx, h_m, travel_um, wp_len_um, wp_thick_um, x_wp0, y_wp0, y_top, nx, ny, nxp, nyp, n_p, p_vol, dt, n_steps })
}

/// 실행 전에 알 수 있는 규모. 시간 추정에 쓴다.
#[derive(Debug, Clone, Copy)]
pub struct RunPlan {
    pub particles: usize,
    pub steps: usize,
    pub dt_s: f64,
    pub dx_um: f64,
    pub mscale: f64,
    pub travel_um: f64,
    pub wp_len_um: f64,
    pub wp_thick_um: f64,
    /// 입자 × 시간 단계 (계산량의 척도)
    pub work: f64,
}

impl RunPlan {
    /// 대략의 소요 시간(초). 측정: 35532 입자 × 40246 단계(빠름, 기본 스테인리스 조건)가 1 스레드에서 547 초
    /// → 스레드당 약 2.6e6 입자·단계/초. 이전 측정(22600 × 46041 이 2 스레드에서 470 초)은 다른 작업과 겹쳐 돌아 더 느렸다.
    /// 여유를 두어 스레드당 2.0e6 으로 잡고, 스레드 수에 대해 지수 0.85 로 늘어난다고 본다.
    /// 컴퓨터마다 다르므로 진행 중에는 실제 진행률로 남은 시간을 다시 계산하는 편이 낫다.
    pub fn estimate_seconds(&self, threads: usize) -> f64 {
        let t = threads.max(1) as f64;
        self.work / (2.0e6 * t.powf(0.85))
    }
}

/// 격자·시간 단계 수를 미리 계산한다 (해석은 하지 않는다).
pub fn plan(prm: &CutParams) -> Result<RunPlan, String> {
    let g = geometry(prm)?;
    Ok(RunPlan {
        particles: g.n_p,
        steps: g.n_steps,
        dt_s: g.dt,
        dx_um: prm.dx_um,
        mscale: prm.mscale,
        travel_um: g.travel_um,
        wp_len_um: g.wp_len_um,
        wp_thick_um: g.wp_thick_um,
        work: g.n_p as f64 * g.n_steps as f64,
    })
}

/// 경사면을 따라 올라가는 칩의 공구 기준 속도 (m/s, 중앙값).
/// 칩 층(처음에 절삭 두께 h 안에 있었고 이미 공구가 지나간 입자) 가운데 날 끝에서 경사면을 따라 1.2h~2.5h,
/// 경사면에서 0~4h 거리에 있는 입자만 쓴다. 정상 상태에서는 질량 보존으로 h·V_c = h_c·V_chip 이므로
/// 두께비 r = h/h_c = V_chip/V_c 가 된다 (칩 단면을 직접 재는 방법보다 칩이 말려도 덜 흔들린다).
#[allow(clippy::too_many_arguments)]
fn chip_speed(parts: &[P], x0: &[[f64; 2]], tx: f64, cen_y: f64, y_top: f64, h_m: f64, r_edge: f64, dx: f64, cr: f64, sr: f64, vc: f64) -> Option<f64> {
    let mut v: Vec<f64> = Vec::new();
    for (p, o) in parts.iter().zip(x0.iter()) {
        if !(o[1] > y_top - h_m && o[0] < tx - 60e-6) {
            continue;
        }
        let (rx, ry) = (p.x[0] - tx, p.x[1] - cen_y);
        let d2 = rx * cr + ry * sr;
        let s2 = -rx * sr + ry * cr;
        if s2 > 1.2 * h_m && s2 < 2.5 * h_m && d2 > r_edge - dx && d2 < r_edge + 4.0 * h_m {
            v.push(-(p.v[0] - vc) * sr + p.v[1] * cr);
        }
    }
    if v.len() < 20 {
        return None;
    }
    Some(percentile(&mut v, 0.5))
}

/// 해석 실행. `progress(진행률 0..1, 메시지)` 가 false 를 돌려주면 취소한다.
pub fn run(prm: &CutParams, progress: &(dyn Fn(f64, &str) -> bool + Sync)) -> Result<CutResult, String> {
    let g = geometry(prm)?;
    let m = &prm.material;
    let t_start = Instant::now();
    let Geo { rho_m, mu_l, lam_l, dx, h_m, travel_um: _, wp_len_um: _, wp_thick_um: _, x_wp0, y_wp0, y_top, nx, ny, nxp, nyp, n_p, p_vol, dt, n_steps } = g;
    let r_edge = prm.edge_um * 1e-6;
    let tip_y = y_top - h_m;
    let cen_y = tip_y + r_edge;
    let tool_x0 = prm.tool_x0_um * 1e-6;
    let (rk, cl) = (prm.rake_deg.to_radians(), prm.clear_deg.to_radians());
    let kc = K {
        dx,
        inv_dx: 1.0 / dx,
        nx,
        ny,
        mu_l,
        lam_l,
        rho: m.rho,
        cp: m.cp,
        beta: m.beta,
        p_vol,
        p_mass: rho_m * p_vol,
        jc_a: m.jc_a,
        jc_b: m.jc_b,
        jc_n: m.jc_n,
        jc_c: m.jc_c,
        jc_m: m.jc_m,
        rate_ref: m.rate_ref,
        t0: m.t_room_c + 273.15,
        tm: m.t_melt_c + 273.15,
        eps_fail: prm.eps_fail,
        v_tool: prm.vc_m_s,
        mu_fric: prm.mu,
        cr: rk.cos(),
        sr: rk.sin(),
        ca: cl.cos(),
        sa: cl.sin(),
        r_edge,
        clamp_y: y_wp0 + 20e-6,
        alpha: m.k / (m.rho * m.cp),
        cond: prm.conduction,
    };
    let mut parts = vec![P::default(); n_p];
    let mut x0 = vec![[0.0; 2]; n_p];
    for i in 0..nxp {
        for j in 0..nyp {
            let p = i * nyp + j;
            let pos = [x_wp0 + (i as f64 + 0.5) * dx / PPC as f64, y_wp0 + (j as f64 + 0.5) * dx / PPC as f64];
            parts[p] = P { x: pos, v: [0.0; 2], c: [0.0; 4], f: [1.0, 0.0, 0.0, 1.0], e3: 0.0, ep: 0.0, epd: 1.0, t: kc.t0 };
            x0[p] = pos;
        }
    }
    let threads = prm.threads.clamp(1, 32);
    let pool = rayon::ThreadPoolBuilder::new().num_threads(threads).build().map_err(|e| format!("스레드 풀 생성 실패: {}", e))?;
    let chunk = n_p.div_ceil(threads);
    let nch = n_p.div_ceil(chunk);
    let n_nodes = nx * ny;
    let bufs = vec![vec![[0.0f64; 4]; n_nodes]; nch];
    let node_chunk = n_nodes.div_ceil(threads * 4).max(256);
    let mut sim = Sim { k: kc, parts, x0, bufs, chunk, nodes: vec![[0.0; 4]; n_nodes], out: vec![[0.0; 3]; n_nodes], node_chunk };

    let log_every = (n_steps / 200).max(50);
    let mut hist: Vec<HistRow> = Vec::with_capacity(220);
    let (mut acc_fx, mut acc_fy, mut acc_n) = (0.0, 0.0, 0usize);
    let mut w_tool = 0.0f64;
    let mut wp_total = 0.0f64;
    let mut ke_max = 0.0f64;
    let mut cancelled = false;
    let mut last_f = 0.0f64;
    let mut last_tick = Instant::now();
    let p_mass = sim.k.p_mass;
    // 칩 속도 표본: 정상 구간(뒤 45%)의 기록 시점마다 경사면 창 안 칩 입자들의 공구 기준 속도 중앙값
    let tail_from = (n_steps as f64 * 0.55) as usize;
    let mut vchip: Vec<f64> = Vec::new();
    let (cr0, sr0) = (rk.cos(), rk.sin());
    pool.install(|| {
        for it in 0..n_steps {
            let tx = tool_x0 + prm.vc_m_s * dt * it as f64;
            let so = sim.step(dt, tx, cen_y);
            acc_fx += so.fx;
            acc_fy += so.fy;
            acc_n += 1;
            w_tool += -so.fx * prm.vc_m_s * dt;
            wp_total += so.wp;
            if (it + 1) % log_every == 0 {
                let (fa_x, fa_y) = (acc_fx / acc_n as f64, acc_fy / acc_n as f64);
                acc_fx = 0.0;
                acc_fy = 0.0;
                acc_n = 0;
                let tmax = sim.parts.iter().map(|p| p.t).fold(0.0f64, f64::max);
                let ke: f64 = 0.5 * p_mass * sim.parts.iter().map(|p| p.v[0] * p.v[0] + p.v[1] * p.v[1]).sum::<f64>();
                if hist.len() > 10 && w_tool > 1e-30 {
                    ke_max = ke_max.max(ke / w_tool);
                }
                hist.push(HistRow { travel_um: (it + 1) as f64 * dt * prm.vc_m_s * 1e6, fc: -fa_x * 1e-3, ft: fa_y * 1e-3, tmax_c: tmax - 273.15 });
                if it >= tail_from {
                    if let Some(v) = chip_speed(&sim.parts, &sim.x0, tx, cen_y, y_top, h_m, r_edge, dx, cr0, sr0, prm.vc_m_s) {
                        vchip.push(v);
                    }
                }
            }
            // 진행률은 1% 마다, 오래 걸리는 해석에서는 0.4초 마다 알린다 (취소가 늦지 않도록)
            if (it + 1) % 16 == 0 {
                let f = (it + 1) as f64 / n_steps as f64;
                if f - last_f >= 0.01 || last_tick.elapsed().as_millis() >= 400 {
                    last_f = f;
                    last_tick = Instant::now();
                    let msg = match hist.last() {
                        Some(h) => format!("이동 {:.0} µm · Fc {:.0} N/mm · Tmax {:.0} °C", h.travel_um, h.fc, h.tmax_c),
                        None => "시작".to_string(),
                    };
                    if !progress(f, &msg) {
                        cancelled = true;
                        break;
                    }
                }
            }
        }
    });

    let mut res = CutResult { steps: n_steps, particles: n_p, wall_s: t_start.elapsed().as_secs_f64(), dt_s: dt, cancelled, history: hist, ..Default::default() };
    if cancelled || res.history.len() < 20 {
        res.notes.push("취소되었거나 기록이 부족해 요약을 만들지 않았습니다".into());
        return Ok(res);
    }
    // 정상 구간(뒤 45%) 평균
    let from = (res.history.len() as f64 * 0.55) as usize;
    let tail = &res.history[from..];
    let nt = tail.len() as f64;
    let fc = tail.iter().map(|h| h.fc).sum::<f64>() / nt;
    res.fc_n_per_mm = fc;
    res.fc_sd = (tail.iter().map(|h| (h.fc - fc).powi(2)).sum::<f64>() / nt).sqrt();
    res.ft_n_per_mm = tail.iter().map(|h| h.ft).sum::<f64>() / nt;
    res.tmax_c = tail.iter().map(|h| h.tmax_c).fold(0.0, f64::max);
    // 정상 상태: 꼬리 구간의 흔들림이 작고(표준편차 12% 미만) 앞뒤 절반 평균 차이도 작아야 한다 (8% 미만).
    // 힘이 계속 오르는 구간(칩 형성 초기, 칩이 쌓이는 경우)을 정상으로 잘못 보지 않기 위해서다.
    let half = tail.len() / 2;
    let trend = if half >= 2 {
        let a = tail[..half].iter().map(|h| h.fc).sum::<f64>() / half as f64;
        let b = tail[half..].iter().map(|h| h.fc).sum::<f64>() / (tail.len() - half) as f64;
        (b - a) / fc.abs().max(1e-9)
    } else {
        0.0
    };
    res.fc_trend = trend;
    res.steady = res.fc_sd < 0.12 * fc.abs().max(1e-9) && trend.abs() < 0.08;
    res.wp_over_wtool = wp_total / w_tool.max(1e-30);
    res.ke_ratio_max = ke_max;

    // 칩 진단 — 초기 위치로 칩 입자를 식별(최종 가공면보다 위에 있던 두께 h 층 중 이미 절삭된 쪽)
    let tip_x = (tool_x0 + prm.vc_m_s * dt * n_steps as f64) * 1e6;
    res.tool_tip_x_um = tip_x;
    res.tool_tip_y_um = tip_y * 1e6;
    let (cr, sr) = (rk.cos(), rk.sin());
    let cen = [tip_x, cen_y * 1e6];
    let mut chip_t = Vec::new();
    let mut chip_ep = Vec::new();
    let mut face_t = Vec::new();
    let mut win_d2 = Vec::new();
    for (p, x0) in sim.parts.iter().zip(sim.x0.iter()) {
        let in_layer = x0[1] * 1e6 > (y_top - h_m) * 1e6 && x0[0] * 1e6 < tip_x - 60.0;
        if !in_layer {
            continue;
        }
        let (rx, ry) = (p.x[0] * 1e6 - cen[0], p.x[1] * 1e6 - cen[1]);
        let d2 = rx * cr + ry * sr;
        let s2 = -rx * sr + ry * cr;
        chip_t.push(p.t - 273.15);
        chip_ep.push(p.ep);
        if s2 > 1.2 * prm.h_um && s2 < 2.2 * prm.h_um {
            win_d2.push(d2);
        }
        if d2 - prm.edge_um < 10.0 && d2 > -5.0 && s2 > 0.0 {
            face_t.push(p.t - 273.15);
        }
    }
    if !chip_t.is_empty() {
        res.chip_t_mean_c = chip_t.iter().sum::<f64>() / chip_t.len() as f64;
        res.chip_ep_mean = chip_ep.iter().sum::<f64>() / chip_ep.len() as f64;
        res.chip_t_p95_c = percentile(&mut chip_t, 0.95);
    }
    if face_t.len() > 10 {
        res.interface_t_mean_c = face_t.iter().sum::<f64>() / face_t.len() as f64;
        res.interface_t_p95_c = percentile(&mut face_t, 0.95);
    } else {
        res.interface_t_mean_c = f64::NAN;
        res.interface_t_p95_c = f64::NAN;
    }
    res.chip_ratio_geom = if win_d2.len() > 30 {
        let mut a = win_d2.clone();
        let t_c = percentile(&mut a, 0.98) - percentile(&mut win_d2, 0.02);
        prm.h_um / t_c.max(1e-9)
    } else {
        f64::NAN
    };
    res.chip_samples = vchip.len();
    let r_flux = if vchip.len() >= 3 {
        let v = percentile(&mut vchip.clone(), 0.5);
        res.chip_velocity_m_s = v;
        v / prm.vc_m_s.max(1e-9)
    } else {
        res.chip_velocity_m_s = f64::NAN;
        f64::NAN
    };
    res.chip_ratio = if r_flux.is_finite() && r_flux > 0.02 && r_flux < 3.0 {
        r_flux
    } else if res.chip_ratio_geom.is_finite() {
        res.notes.push("칩 속도 표본이 부족해 칩 두께비는 단면 두께 측정값을 씁니다".into());
        res.chip_ratio_geom
    } else {
        f64::NAN
    };
    if res.chip_ratio.is_finite() {
        res.chip_thickness_um = prm.h_um / res.chip_ratio;
        res.shear_angle_deg = (res.chip_ratio * cr).atan2(1.0 - res.chip_ratio * sr).to_degrees();
        if r_flux.is_finite() && res.chip_ratio_geom.is_finite() && (res.chip_ratio_geom / r_flux - 1.0).abs() > 0.35 {
            res.notes.push(format!(
                "칩 단면 두께로 잰 두께비 {:.2} 와 칩 속도(질량 보존)로 구한 {:.2} 가 크게 다릅니다 — 칩이 말리거나 공구에 붙어 단면 측정이 흔들린 것으로 보고 속도 기준값을 씁니다",
                res.chip_ratio_geom, r_flux
            ));
        }
    } else {
        res.chip_thickness_um = f64::NAN;
        res.shear_angle_deg = f64::NAN;
        res.notes.push("연속 칩이 형성되지 않아 칩 두께비를 구하지 못했습니다".into());
    }
    let bg = res.ft_n_per_mm.atan2(res.fc_n_per_mm);
    res.mu_apparent = (bg + rk).tan();
    if res.shear_angle_deg.is_finite() {
        let ph = res.shear_angle_deg.to_radians();
        let fs = res.fc_n_per_mm * ph.cos() - res.ft_n_per_mm * ph.sin();
        res.tau_shear_mpa = fs * ph.sin() / (prm.h_um * 1e-3);
    } else {
        res.tau_shear_mpa = f64::NAN;
    }
    let ceiling = (ny as f64 - 6.0) * dx;
    res.chip_at_ceiling = sim.parts.iter().any(|p| p.x[1] > ceiling);
    if res.chip_at_ceiling {
        res.notes.push("칩이 계산 영역 천장 근처까지 올라가 빠져나가지 못하고 쌓였을 수 있습니다 — 절삭력이 커지는 쪽으로 틀어집니다".into());
    }
    if !res.steady {
        res.notes.push(format!(
            "힘이 아직 안정 구간이 아닙니다(표준편차 {:.0}% · 앞뒤 절반 차이 {:+.0}%) — 이동거리를 늘려 보세요",
            100.0 * res.fc_sd / fc.abs().max(1e-9),
            100.0 * res.fc_trend
        ));
    }
    if res.ke_ratio_max > 0.05 {
        res.notes.push(format!("운동에너지 비 {:.1}% — 질량 스케일링이 과해 준정적 가정이 흔들립니다", res.ke_ratio_max * 100.0));
    }
    if prm.keep_particles {
        // 압력(평균응력, 압축 +): 탄성 로그변형률 합으로 계산
        let kb = lam_l + 2.0 * mu_l / 3.0;
        res.particles_dump = sim
            .parts
            .iter()
            .zip(sim.x0.iter())
            .map(|(p, o)| {
                let (_, s, _) = svd2(p.f);
                let tr = s[0].max(1e-6).ln() + s[1].max(1e-6).ln() + p.e3;
                [
                    (p.x[0] * 1e6) as f32,
                    (p.x[1] * 1e6) as f32,
                    p.ep as f32,
                    (p.t - 273.15) as f32,
                    (-kb * tr * 1e-9) as f32,
                    p.v[0] as f32,
                    p.v[1] as f32,
                    (o[0] * 1e6) as f32,
                    (o[1] * 1e6) as f32,
                ]
            })
            .collect();
    }
    res.wall_s = t_start.elapsed().as_secs_f64();
    Ok(res)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lcg(seed: &mut u64) -> f64 {
        *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((*seed >> 11) as f64) / ((1u64 << 53) as f64)
    }

    #[test]
    fn svd2_reconstructs_random_matrices() {
        let mut seed = 12345u64;
        for _ in 0..2000 {
            let m = [lcg(&mut seed) * 4.0 - 2.0, lcg(&mut seed) * 4.0 - 2.0, lcg(&mut seed) * 4.0 - 2.0, lcg(&mut seed) * 4.0 - 2.0];
            let (u, s, vt) = svd2(m);
            let r = mul2(mul2(u, [s[0], 0.0, 0.0, s[1]]), vt);
            for k in 0..4 {
                assert!((r[k] - m[k]).abs() < 1e-9, "m={:?} r={:?}", m, r);
            }
            // U, Vt 는 회전행렬
            assert!((u[0] * u[3] - u[1] * u[2] - 1.0).abs() < 1e-12);
            assert!((vt[0] * vt[3] - vt[1] * vt[2] - 1.0).abs() < 1e-12);
            // 양의 행렬식이면 두 특이값이 모두 양수
            let det = m[0] * m[3] - m[1] * m[2];
            if det > 1e-6 {
                assert!(s[0] > 0.0 && s[1] > 0.0);
            }
        }
    }

    fn test_k() -> K {
        let m = Material::sus304();
        let mu_l = m.e_pa / (2.0 * (1.0 + m.nu));
        let lam_l = m.e_pa * m.nu / ((1.0 + m.nu) * (1.0 - 2.0 * m.nu));
        K {
            dx: 4e-6,
            inv_dx: 1.0 / 4e-6,
            nx: 100,
            ny: 100,
            mu_l,
            lam_l,
            rho: m.rho,
            cp: m.cp,
            beta: m.beta,
            p_vol: (2e-6f64).powi(2),
            p_mass: m.rho * (2e-6f64).powi(2),
            jc_a: m.jc_a,
            jc_b: m.jc_b,
            jc_n: m.jc_n,
            jc_c: m.jc_c,
            jc_m: m.jc_m,
            rate_ref: m.rate_ref,
            t0: 293.15,
            tm: 1673.15,
            eps_fail: 0.0,
            v_tool: 2.2,
            mu_fric: 0.4,
            cr: 12f64.to_radians().cos(),
            sr: 12f64.to_radians().sin(),
            ca: 7f64.to_radians().cos(),
            sa: 7f64.to_radians().sin(),
            r_edge: 0.0,
            clamp_y: 0.0,
            alpha: 4e-6,
            cond: false,
        }
    }

    #[test]
    fn return_mapping_lands_on_yield_surface_and_keeps_pressure() {
        let k = test_k();
        // 단순 전단 + 정수압(압축) 상태: 탄성 로그변형률 e1=0.02, e2=-0.012 → 편차 큼, 평균은 음(압축)
        let (e1, e2, e3) = (0.0030f64, -0.0120f64, -0.0005f64);
        let mut p = P { x: [1e-4, 1e-4], v: [0.0; 2], c: [0.0; 4], f: [e1.exp(), 0.0, 0.0, e2.exp()], e3, ep: 0.2, epd: 1e3, t: 293.15 };
        let mean_before = (e1 + e2 + e3) / 3.0;
        let (tau, wp) = stress_update(&k, 1e-9, &mut p);
        assert!(wp > 0.0, "항복을 넘었는데 소성일이 0");
        // 반환 뒤 von Mises 응력이 항복응력과 같아야 한다 (평면변형 3성분 모두 포함)
        let s = svd2(p.f).1;
        let (l1, l2, l3) = (s[0].ln(), s[1].ln(), p.e3);
        let mean_after = (l1 + l2 + l3) / 3.0;
        assert!((mean_after - mean_before).abs() < 1e-12, "정수압(체적 변형률)이 소성 중에 바뀌었다: {} -> {}", mean_before, mean_after);
        let dn = ((l1 - mean_after).powi(2) + (l2 - mean_after).powi(2) + (l3 - mean_after).powi(2)).sqrt();
        let q = 6f64.sqrt() * k.mu_l * dn;
        let sy = yield_stress(&k, p.ep, p.epd, 293.15);
        // ep 가 갱신된 뒤의 항복응력(경화)과 대략 맞아야 한다: 반환은 갱신 전 sy 로 했으므로 q <= sy_new
        assert!(q <= sy * 1.0001 + 1.0, "q={} sy={}", q, sy);
        assert!(tau.iter().all(|x| x.is_finite()));
    }

    #[test]
    fn elastic_state_has_no_plastic_work() {
        let k = test_k();
        let mut p = P { x: [1e-4, 1e-4], v: [0.0; 2], c: [0.0; 4], f: [1.0001, 0.0, 0.0, 0.9999], e3: 0.0, ep: 0.0, epd: 1.0, t: 293.15 };
        let (_, wp) = stress_update(&k, 1e-9, &mut p);
        assert_eq!(wp, 0.0);
        assert_eq!(p.ep, 0.0);
    }

    #[test]
    fn short_run_is_sane() {
        let mut prm = CutParams::quick(Material::sus304());
        prm.dx_um = 8.0;
        prm.travel_um = 120.0;
        prm.wp_len_um = 260.0;
        prm.wp_thick_um = 200.0;
        prm.threads = 2;
        let res = run(&prm, &|_, _| true).expect("run");
        assert!(res.steps > 100);
        assert!(res.fc_n_per_mm > 20.0 && res.fc_n_per_mm < 600.0, "fc={}", res.fc_n_per_mm);
        assert!(res.wp_over_wtool > 0.4 && res.wp_over_wtool < 1.05, "Wp/Wtool={}", res.wp_over_wtool);
        assert!(res.ke_ratio_max < 0.2, "ke={}", res.ke_ratio_max);
        assert!(res.history.iter().all(|h| h.fc.is_finite() && h.ft.is_finite()));
        let j = res.to_json();
        assert!(j.starts_with('{') && j.ends_with('}'));
    }

    #[test]
    fn chip_speed_recovers_mass_flow_ratio() {
        // 경사면을 따라 r·V_c 로 미끄러지는 강체 칩을 만들면 r 을 그대로 돌려줘야 한다
        let (vc, rake, h, r_edge, dx) = (2.0, 12f64.to_radians(), 50e-6, 8e-6, 4e-6);
        let (cr, sr) = (rake.cos(), rake.sin());
        let (tx, y_top) = (500e-6, 300e-6);
        let cen_y = y_top - h + r_edge;
        for r in [0.3, 0.6, 0.9] {
            let mut parts = Vec::new();
            let mut x0 = Vec::new();
            for i in 0..12 {
                for j in 0..12 {
                    let s2 = 1.3 * h + (i as f64) * 0.09 * h;
                    let d2 = r_edge + (j as f64 + 0.5) * (h / r) / 12.0;
                    // (d2, s2) → 실좌표: 법선 (cr, sr), 경사면 방향 (-sr, cr)
                    let x = tx + d2 * cr - s2 * sr;
                    let y = cen_y + d2 * sr + s2 * cr;
                    let v = [vc - r * vc * sr, r * vc * cr];
                    parts.push(P { x: [x, y], v, c: [0.0; 4], f: [1.0, 0.0, 0.0, 1.0], e3: 0.0, ep: 1.0, epd: 1.0, t: 300.0 });
                    x0.push([tx - 200e-6, y_top - 0.5 * h]);
                }
            }
            let got = chip_speed(&parts, &x0, tx, cen_y, y_top, h, r_edge, dx, cr, sr, vc).expect("samples");
            assert!((got / vc - r).abs() < 1e-9, "r={} got={}", r, got / vc);
        }
        // 아직 공구가 지나가지 않은 입자나 칩 층 밖 입자는 쓰지 않는다
        let p = P { x: [tx + 30e-6, cen_y + 80e-6], v: [0.0; 2], c: [0.0; 4], f: [1.0, 0.0, 0.0, 1.0], e3: 0.0, ep: 0.0, epd: 1.0, t: 300.0 };
        assert!(chip_speed(&vec![p; 40], &vec![[tx + 10e-6, y_top - 0.5 * h]; 40], tx, cen_y, y_top, h, r_edge, dx, cr, sr, vc).is_none());
    }

    #[test]
    fn plan_matches_reference_case() {
        // 기준 조건: 22600 입자, 약 46041 단계 (검증 실행과 같은 값)
        let mut prm = CutParams::new(Material::sus304());
        prm.travel_um = 300.0;
        prm.wp_len_um = 450.0;
        prm.wp_thick_um = 200.0;
        let pl = plan(&prm).expect("plan");
        assert_eq!(pl.particles, 22600);
        assert!((pl.steps as i64 - 46041).abs() <= 2, "steps={}", pl.steps);
        assert!(pl.estimate_seconds(2) > 200.0 && pl.estimate_seconds(2) < 900.0, "est={}", pl.estimate_seconds(2));
        assert!(pl.estimate_seconds(8) < pl.estimate_seconds(2));
    }

    #[test]
    fn for_conditions_scales_grid_and_mass() {
        let base = CutParams::for_conditions(Material::sus304(), Preset::Standard, 50.0, 2.2, 12.0, 7.0, 0.4, 8.0);
        assert!((base.dx_um - 4.0).abs() < 1e-9 && (base.mscale - 200.0).abs() < 1e-9);
        let q = CutParams::for_conditions(Material::sus304(), Preset::Quick, 50.0, 2.2, 12.0, 7.0, 0.4, 8.0);
        assert!((q.dx_um - 5.0).abs() < 1e-9 && (q.mscale - 400.0).abs() < 1e-9);
        // 느린 속도는 질량 스케일을 키우되 2000 에서 멈춘다
        let slow = CutParams::for_conditions(Material::sus304(), Preset::Standard, 50.0, 0.3, 12.0, 7.0, 0.4, 8.0);
        assert!((slow.mscale - 2000.0).abs() < 1e-9);
        // 빠른 속도는 줄이되 5 아래로 내려가지 않는다
        let fast = CutParams::for_conditions(Material::sus304(), Preset::Standard, 50.0, 12.0, 12.0, 7.0, 0.4, 8.0);
        assert!(fast.mscale >= 5.0 && fast.mscale < 10.0, "ms={}", fast.mscale);
        // 격자는 칩 두께에 따라 제한 범위 안에서 움직인다
        let thin = CutParams::for_conditions(Material::sus304(), Preset::Standard, 20.0, 2.2, 12.0, 7.0, 0.4, 8.0);
        assert!((thin.dx_um - 2.5).abs() < 1e-9);
        let thick = CutParams::for_conditions(Material::sus304(), Preset::Standard, 150.0, 2.2, 12.0, 7.0, 0.4, 8.0);
        assert!((thick.dx_um - 6.0).abs() < 1e-9);
        // 시간 단계 수는 속도와 거의 무관해야 한다 (질량 스케일이 1/v² 로 움직이는 구간)
        let s1 = plan(&CutParams::for_conditions(Material::sus304(), Preset::Standard, 50.0, 1.0, 12.0, 7.0, 0.4, 8.0)).unwrap().steps as f64;
        let s2 = plan(&CutParams::for_conditions(Material::sus304(), Preset::Standard, 50.0, 4.0, 12.0, 7.0, 0.4, 8.0)).unwrap().steps as f64;
        assert!((s1 / s2 - 1.0).abs() < 0.2, "s1={} s2={}", s1, s2);
    }

    #[test]
    fn preset_keys_round_trip() {
        assert_eq!(Preset::from_key("quick"), Some(Preset::Quick));
        assert_eq!(Preset::from_key(" Standard "), Some(Preset::Standard));
        assert_eq!(Preset::from_key("nope"), None);
        assert_eq!(Preset::from_key(Preset::Quick.key()), Some(Preset::Quick));
    }

    #[test]
    fn cancel_stops_early() {
        let mut prm = CutParams::quick(Material::sus304());
        prm.dx_um = 8.0;
        prm.travel_um = 200.0;
        prm.wp_len_um = 350.0;
        let res = run(&prm, &|f, _| f < 0.1).expect("run");
        assert!(res.cancelled);
    }

    #[test]
    fn rejects_bad_parameters() {
        let mut prm = CutParams::new(Material::sus304());
        prm.h_um = 1.0;
        assert!(run(&prm, &|_, _| true).is_err());
        let mut prm = CutParams::new(Material::sus304());
        prm.material.jc_n = f64::NAN;
        assert!(run(&prm, &|_, _| true).is_err());
    }
}
