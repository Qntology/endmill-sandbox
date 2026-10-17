# 가짜 앤드밀 샌드박스 오픈소스 프로젝트

## 프로젝트 전제

오프라인에서 필요한 것은 다음과 같습니다:

- 이벤트 카메라
- 엔드밀, 절삭공구, 공작물
- 전문성

해당 항목은 없기 때문에 가상 시뮬레이션으로 하되, 가능하면 각 환경적인 부분을 조사하여
절삭공구 테스트가 가능하다면 첫번째로 휴대폰 카메라로 낮은 RPM 상황에서 스캔된 영상을
이벤트 카메라 포맷의 데이터로 전처리하여 마모 체크를 진행하거나, 근본적으로 가장 확실한
휴대폰 카메라나 이벤트 카메라로 해당 값을 얻는 게 가능한 희망적인 상황을 고려해서 진행하고자 합니다.

### Plan A — 카메라와 시뮬레이션 연동

**카메라 촬영 기준:**

- 앤드밀이 한 바퀴 돌면 파노라마 사진과 같이 3D로 접어서 마모 편차를 계산합니다.
- 피삭재의 경우 촬영하는 부분이 아니기 때문에 피지컬 예측으로 진행합니다.

### Plan B (선택) — 휴대폰 카메라와 이벤트 카메라 포맷 전처리 후 시뮬레이션 연동, 또는 가상 조건으로 진행

**가상환경:**

- 온도, 습도
- 앤드밀 형태, 앤드밀 코팅 소재
- 피삭재 형태와 피삭재 소재
- 냉각 방식 (절삭유 등)

**가상상호작용:**

- 앤드밀과 피삭재 간의 스트레스 증가
- 냉각으로 인한 앤드밀의 스트레스 감소
- 냉각으로 인한 피삭재의 스트레스 감소

**휴대폰 시나리오 가설:**

휴대폰 촬영 가능한 조건은 1000 RPM 근방에서 가능하며:

0. 최초에 앤드밀의 위치를 포커싱합니다.
1. 가공 중에 G-코드로 RPM과 이송 속도를 변경하는 형태로, 원하는 시점에 RPM과 이송 속도 값만 바꿔주면 스핀들이 즉시 속도를 줄여서 그 시점에만 촬영을 진행하여 앤드밀 상태를 체크하고 랩탑으로 전송합니다.
2. 랩탑에서는 Depth Estimation으로 깊이를 잡고, mask-generation 모델로 형태를 정확하게 선택하여 두 모델 결과 값을 기준으로 기준 값을 잡습니다.
3. 이후 촬영에서 최초 원본 기준 값에서 변화되는 특징을 잡습니다.
   - 특징 값을 쌓은 뒤 시계열 모델로 현재 시간 싱크가 맞지 않는 부분을 잡아보고 싶습니다.

> ⚠️ 해당 가설은 절대 정확하지 않으며, 절삭공구가 없어서 가설에서 끝날 가능성이 높습니다.

### 공통

- 앤드밀 브랜드는 저작권 이슈가 있어서 가져올 수 없으며, 가능하면 해당 앤드밀 형태와 앤드밀 코팅 소재를 프리셋으로 가져오는 방향으로 하고자 합니다.

---

## End Mill Sandbox — Project Premise (English)

### What This Project Assumes Offline

The following are required for real-world testing:

- Event camera
- End mills, cutting tools, workpieces
- Domain expertise

None of these are available. Therefore, this project proceeds as a **virtual simulation**,
researching each environmental factor so that — should cutting-tool testing become possible —
the first step would be to use a phone camera at low RPM to scan the tool, preprocess the
video into event-camera-format data for wear checking, or ultimately obtain the values
directly from a phone camera or event camera in an aspirational future scenario.

### Plan A — Camera ↔ Simulation Integration

**Capture criteria:**

- One full rotation of the end mill is stitched panoramatically into a 3D surface to compute wear deviation.
- The workpiece is not imaged; its state is handled by physics prediction.

### Plan B (Optional) — Phone Camera + Event-Camera-Format Preprocessing → Simulation, or Pure Virtual Conditions

**Virtual environment:**

- Temperature, humidity
- End mill geometry, coating material
- Workpiece geometry, workpiece material
- Cooling method (cutting fluid, etc.)

**Virtual interaction:**

- Stress increase between end mill and workpiece
- Stress reduction on end mill via cooling
- Stress reduction on workpiece via cooling

**Phone-camera scenario hypothesis:**

Feasible capture conditions are around 1 000 RPM:

0. Focus on the end mill's initial position.
1. During machining, modify RPM and feed rate via G-code so the spindle decelerates at a chosen instant; capture the tool state at that moment and transmit to a laptop.
2. On the laptop, run Depth Estimation for geometry and a mask-generation model for precise segmentation; use both outputs to establish the baseline.
3. In subsequent captures, extract feature deltas from the original baseline.
   - Accumulate feature values and use a time-series model to reconcile any temporal misalignment.

> ⚠️ This hypothesis is not guaranteed to be correct and may remain purely theoretical
> due to the absence of physical cutting tools.

### Common Note

- End mill brand names cannot be used due to copyright concerns. Instead, tool geometry
  and coating material are provided as presets.

---

# End Mill Sandbox

A physics-driven CNC end mill machining simulator, tool wear analysis platform, and
cutting-condition curation engine — built in Rust with a Tauri desktop shell.

> **Spec · Coat · Cut · Measure · Decide**

---

## What This Project Does

End Mill Sandbox models the full lifecycle of an end mill in a machining operation:

| Stage | Capability |
| --- | --- |
| **Tool Selection** | Curate candidate end mills from a SQLite + LanceDB library using physics scoring, direct history, and relay evidence from similar workpieces/tools |
| **Cutting Condition Calculation** | Mechanistic force model, Kienzle power law, chip-thinning correction, Johnson-Cook flow stress |
| **Process Simulation** | Grid-based material removal along 6 toolpath patterns (pocket zigzag, rectangular profile, circular, helical, contour multi-pass, slot) with per-step force / power / temperature / wear tracking |
| **Thermal & Tribology** | Interface temperature (Boothroyd partition), coating × workpiece × coolant interaction matrix, boiling-regime heat transfer, thermal shock / crack risk, BUE risk |
| **Wear Prediction** | 4-mechanism decomposition (abrasion · adhesion · diffusion · oxidation), 3-phase trajectory (break-in → steady → accelerated), calibration against measured logs |
| **Chatter & Dynamics** | Regenerative chatter stability lobes (Altintas–Budak), forced vibration at tooth-passing frequency, process damping, variable-pitch gain, measured-FRF override with stickout scaling |
| **Tool Image Analysis** | SigLIP2 (large patch16 512) zero-shot wear-type classification, patch-vector deviation z-score, silhouette-based radial loss measurement (µm resolution) |
| **Mold / Workpiece Metrology** | Heightmap deviation fit (Z offset · radial wear · deflection · thermal · tilt decomposition), wall CMM gain calibration, mold geometry vector comparison |
| **Time-Series Forecasting** | Granite TTM-R3 (safetensors) quantile forecasting with physics-residual mode; Holt-damped fallback |
| **Decision Engine** | Deterministic safety gates (power · torque · deflection · temperature · chatter · wear · forecast · mold residual · tilt) + optional laya-typed-decisions advisor (escalation-only) |
| **Adaptive Feed G-code** | Episode-segmented load timeline → per-segment feed override → exportable NC program |
| **Statistical Ledger (SDS)** | Welford + ring-buffer hierarchical write across 4 ledgers (tool · mold · process · decision), drift detection, calibration shrinkage, confusion tracking |

---

## Architecture Overview

```text
┌─────────────────────────────────────────────────────────────┐
│  Tauri Shell (src-tauri/)                                   │
│  ├─ commands.rs   – 50+ IPC commands                        │
│  └─ main.rs       – window lifecycle, state flush           │
├─────────────────────────────────────────────────────────────┤
│  Frontend (index.html, single-file)                         │
│  ├─ 3D toolpath viewport (canvas, drag/scroll rotation)     │
│  ├─ Coolant particle visualisation                          │
│  ├─ Precision Analysis (7-tab overlay)                      │
│  └─ Library / Curation panel                                │
├─────────────────────────────────────────────────────────────┤
│  Rust Library  (src/)                                       │
│  ├─ physics.rs        – core cutting analysis               │
│  ├─ tribology.rs      – friction / coating / coolant chem   │
│  ├─ dynamics.rs       – chatter lobes, forced vibration     │
│  ├─ loadsim.rs        – grid-based path simulation          │
│  ├─ toolimage.rs      – silhouette / edge extraction        │
│  ├─ wear.rs           – SigLIP2 comparison engine           │
│  ├─ mold.rs           – heightmap fit / wall CMM            │
│  ├─ sds.rs            – statistical dynamics store          │
│  ├─ decision.rs       – gate evaluation + advisor           │
│  ├─ ingest.rs         – document parser (spec/NC/CSV/img)   │
│  ├─ pipeline.rs       – workspace orchestrator              │
│  ├─ timeseries.rs     – forecast (TTM / Holt)               │
│  ├─ gcode.rs          – G-code generation                   │
│  ├─ store/            – SQLite (rusqlite) + LanceDB         │
│  │  ├─ rdb.rs         – relational schema                   │
│  │  ├─ lance.rs       – LanceDB connection wrapper          │
│  │  ├─ vector.rs      – ANN vector collections              │
│  │  ├─ series.rs      – time-series point/window store      │
│  │  ├─ attr.rs        – attribute vector definitions        │
│  │  └─ record.rs      – run/ingest/decision recording       │
│  ├─ curation.rs       – candidate scoring & relay           │
│  ├─ ml/               – model loading                       │
│  │  ├─ siglip.rs      – SigLIP2 vision + text               │
│  │  ├─ ttm.rs         – TTM-R3 forecaster                   │
│  │  ├─ laya.rs        – laya decision advisor               │
│  │  └─ modernbert.rs  – ModernBERT encoder                  │
│  └─ profile.rs / cutting.rs / spec.rs / coating.rs / …      │
└─────────────────────────────────────────────────────────────┘

```

---

## Key Dependencies

| Crate | Role |
| --- | --- |
| `candle-core` / `candle-nn` | Tensor inference (CPU / CUDA / ROCm / Vulkan via feature flag) |
| `tokenizers` | SigLIP2 / laya text encoding |
| `image` | PNG / JPEG / TIFF / BMP decode for tool & mold images |
| `rusqlite` (bundled) | Relational library (projects, presets, profiles, runs, metrics) |
| `lancedb` | Vector search (endmill/workpiece attributes, cut regime, time-series windows) |
| `tokio` / `futures` | Async runtime for LanceDB |
| `serde` / `serde_json` | Serialisation throughout |
| `thiserror` | Error types |
| `regex` | Tool-spec text parsing |
| `base64` | Image transfer over IPC |
| `tauri` | Desktop shell |

---

## Getting Started

### Prerequisites

* Rust ≥ 1.75 (edition 2021)
* Node.js ≥ 18 + npm
* Tauri CLI v1 (`@tauri-apps/cli ^1.5`)
* *(Optional)* CUDA toolkit + cuDNN for GPU inference → `--features cuda`
* *(Optional)* AMD ROCm/HIP SDK (Windows: `C:\Program Files\AMD\ROCm\<ver>`) → `--features rocm`
* *(Optional)* Any Vulkan 1.1+ GPU driver (no SDK needed) → `--features vulkan`
* *(Optional)* Model weights in safetensors format (SigLIP2, TTM-R3, laya)

### Build & Run

```bash
# Install frontend deps
npm install

# Development (CPU)
cargo tauri dev

# Development (CUDA)
cargo tauri dev -- --features cuda

# Release build
cargo tauri build -- --features cuda

# ROCm / Vulkan (candle-rocm-vulkan fork)
cargo tauri dev   -- --no-default-features --features rocm
cargo tauri dev   -- --no-default-features --features vulkan
cargo tauri build -- --no-default-features --features rocm

```

> On Windows, see `run.bat` / `build.bat` for pre-configured MSVC + NVCC environment.
> ROCm / Vulkan: `run_rocm.bat`, `run_vulkan.bat`, `build_rocm.bat`, `build_vulkan.bat`, `check_*.bat`, `test_*.bat`
> (env: `ROCM_VER` (default 7.2), `VCVARS_VER`, `HIP_ARCH`, `CANDLE_VULKAN_DEVICE`, ...).

### Tests

```bash
cargo test                                   # CPU
./test_linux.sh vulkan                       # Linux: full suite + Vulkan parity tests (lavapipe if no GPU)
./test_linux.sh vulkan --test vulkan_backend # Vulkan parity tests only
./test_linux.sh rocm                         # needs /opt/rocm
```

On Windows use `test_vulkan.bat` / `test_rocm.bat` (extra arguments are passed to `cargo test`).

```bash
cargo test --test physcal_flow              # MPM cross-check → calibration channel (does not run the solver)
cargo test --lib physcal                    # record / envelope / reliability unit tests
(cd crates/endmill_mpm && cargo test --release)   # solver unit tests
```
`tests/vulkan_backend.rs` (compiled only with `--features vulkan`) runs every op pattern the models use
— masked attention with `-inf` sliding-window masks, LayerNorm (with/without bias), GeGLU/`gelu_erf`,
RoPE, `index_select`, SigLIP `conv2d` patch embedding and `interpolate2d`, a tiny ModernBERT forward pass,
and the memory-pool flush — on the Vulkan device and compares it with the CPU result, both with the
fork's default path and with native GPU shaders forced on.

### Model Weights

Place safetensors model folders under a root directory (default: `<app_data>/models/`),
then specify the root in **Precision Analysis → ① Input/Model → Folder**.

Expected folder naming (keyword search):

| Keyword | Example folder | Purpose |
| --- | --- | --- |
| `siglip` | `siglip2-large-patch16-512/` | Tool & mold image embedding |
| `ttm` | `granite-timeseries-ttm-r3/52-16-dec-52-r3/` | Wear time-series forecast |
| `laya` | `laya-typed-decisions/` | Decision advisor (ModernBERT encoder) |

> **ONNX is not supported.** Only `.safetensors` / `.gguf` weights are accepted.

---

## Simulation Physics — Current Coverage

### Force Model

* Mechanistic oblique-cutting force integration over helix angle + axial slices
* Kienzle power-law specific energy with `mc` exponent
* Flank-wear land force (σ_flank · VB · db)
* Edge-force terms (Kte, Kre) with minimum-chip-thickness ploughing ramp
* Runout per-tooth chip-thickness offset. `runout_um` is the dial-indicator **TIR** (the "3 µm" of a holder datasheet):
  eccentricity e = TIR/2, the chip-thickness difference between two flutes is 2e = TIR (Kline & DeVor 1983). A slot with
  a 10 µm TIR therefore peaks at fz + 10 µm. Earlier versions read the number as e (offset ±2·TIR), which inflated the
  aluminium roughing preset's mean chip thickness to 28.6 µm and the peak/mean force to 4.3.
* Axial (tool-lifting) force from the oblique cut: `dFa = Kac·shear + Kae·db`, `Kac/Ktc = 0.35·tan(helix)`
  (measured 0.2–0.4 on aluminium end mills), `Kae = 0.3·Kte`. Mean and peak are reported in the audit; the axial force does
  not bend the tool, so it does not enter the wall error.
* Chip thinning factor for radial immersion < 50 %
* Work-hardening multiplier on specific energy
* Aluminium coefficients follow the literature instead of the generic table: `kc11` 600 N/mm² (1.4 J/mm³ at the
  roughing preset's 33 µm mean chip; AA2024 orthogonal cuts measure 1.4–1.9 J/mm³ at 29 µm and 1.0–1.3 at 100 µm, the
  MPM cross-check gives 0.9–1.3), radial ratio floor `kr ≥ 0.40` (six end-mill coefficient sets: 0.33–0.52;
  with a small `kr` the tangential and radial components cancel at the instant the wall is cut and the deflection is
  under-read), reference life 60 min (Ø6 carbide end mills reach VB 0.2–0.4 mm in 59–342 min and 119–684 cm³; the old
  240 min anchor gave the roughing preset a path life of 841 min / 2,100 cm³; 60 min gives about 260 min — still above the
  band, so the audit flags it yellow (`wear_band_life_time`) until a real VB series is entered).

### Tool–Workpiece Pair Reference Values (`src/pairref.rs` + the material table)

Several numbers used to be the same for every tool and workpiece. They now come from per-workpiece reference values
(`WorkpieceProps`, sources in the code comments) times small tool-side corrections (coating, edge, holder, stickout).
The audit prints the values that were used for each job (`[AUDIT] 조합 기준값`, JSON `audit.pair`) and lists the entries
that have no direct measurement behind them (`pair_reference_low_confidence`).

| Value | Before (all pairs) | Now | Main evidence |
| --- | --- | --- | --- |
| Tool–chip friction μ | coating pin-on-disc μ × adhesion (DLC on Al 0.10, diamond on CFRP 0.05) | workpiece cutting μ (Al 0.78, C45 0.50, alloy 0.48, hardened 0.42, SS 0.55, Ti 0.45, Ni 0.55, CFRP 0.30) × coating factor 0.88–1.0 | forces measured with coated vs uncoated tools change 0–6 % (C45 AlTiN −0.4…−4 %, Al-Si DLC −4…−6 %); μ back-calculated from cutting forces |
| Friction → kc factor range | 0.6–1.6 | 0.7–1.3 | same; the new factors match Sandvik's rake correction (1 − γ/100) within a few % |
| Krc/Ktc floor | 0.15 (Al 0.40) | Al 0.40, C45/alloy 0.33, Ti 0.37, SS/Ni/hardened 0.30, CFRP 0.25 | end-mill coefficient sets (Al ×4, C45 ×3, Ti ×3) |
| Kre/Kte | 1.4 | Al 0.9, C45/alloy 1.5, Ti 1.9, SS/hardened 1.0, Ni/CFRP 1.4 | Al end-mill sets 0.83–1.0, 1045 orthogonal 1.5–2, Ti end mill 1.9 |
| Kte | 0.011·kc1.1·√(r/10) | × material factor (Ti 1.7, SS 1.3) | Ti end mill Kte 41 N/mm, orthogonal 24; 304 turning edge intercept 51 N/mm |
| kc1.1 at 55–65 HRC | 3800 (60), 4200 (65) | Sandvik MC: 3690 (55), 4330 (60), 4750 (63), 4950 (65) | Sandvik tables |
| h_min / r_edge | hardened 0.20, SS 0.35 | hardened 0.35, SS 0.28 | review of 20 studies (0.08–0.63), 304 FE/tests 0.25–0.30 |
| Break-in VB_b | 15 % of the VB limit | per pair: Al 15, C45 30, SS 20, hardened 45, Ti 60, Ni 50, CFRP 20 µm × (10 µm/r)^0.25 × (Vc/Vc_ref)^0.5 for Ti/Ni, ≤ 40 % of the limit | HRC 52 sharp ball 31–63 µm in the first 4 s; honed coated 316L no visible wear for 30 min; Ti AlCrN 115 µm in 5 m |
| Break-in τ | 3 % of the linear life (12–22 min for long-life pairs) | 0.5–2 min of cutting per workpiece | break-in ends within a few metres of feed path |
| VB limit | 0.2 if tolerance ≤ 20 µm else 0.3 | min(tolerance rule, workpiece criterion: Ti/SS/CFRP 0.2, hardened 0.15, others 0.3) | ISO 8688-2 (0.3 uniform), Ti 0.2 avg, hardened finishing 0.1–0.2, CFRP delamination/uncut fibres |
| Default clearance | 10° | Al 12, C45/alloy 8, hardened 7, SS 9, Ti 10, Ni 9, CFRP 15° | manufacturer/patent consensus; it changes the radial loss per VB (ΔR = VB·tan α) |
| Runout TIR | 3 µm premium / 10 µm | by holder: shrink-fit/hydraulic/polygon 3, ER UP 5, ER precision 10, ER standard 15, milling chuck 8, side-lock 15 µm (unknown holder keeps the old rule) | holder catalogues, ISO 15488 collet classes |
| Tool-tip damping ζ | 0.03 | 0.28·(L/D)^−1.38, 0.010–0.045 (+0.005 hydraulic) unless entered by a tap test | measured 0.05 (L/D 3.75), 0.026 (5.8), 0.009 (12); holder mode 0.02–0.025 |
| Process damping factor | Al 0.4, steel 1.0, SS 1.1, Ti 1.3 | Al 1.0, steel 1.0, SS 1.8, Ti 1.0, × (tan 10°/tan α)^0.4 | Tyler/Karandikar/Schmitz 2014: C 304 ≈ 3.2× 1018, Ti ≈ 1.06×; C +23–42 % from 15° to 11° relief |
| Thermal factor | C45 1.0, hardened 1 − 0.015·(HRC−25) | C45 0.88, hardened 0.40 at 58 HRC, CFRP 0.42; Kienzle size effect not extrapolated below a 20 µm chip in the temperature formula | measured interface bands per preset (see below) |

Interface temperatures of the built-in presets against bands built from published measurements (dry): Al roughing
262 °C (180–350), Al finishing Vc 300 281 °C (220–380; it used to be 202 °C, colder than Vc 165), Ti-6Al-4V 524 °C
(500–750), C45 555 °C (420–580, was 606), SUS304 585 °C (480–650), Inconel 718 611 °C (550–800), 58 HRC ball 542 °C
(350–600, was 745), CFRP 276 °C (150–300). The test `preset_interface_temperatures_sit_in_published_bands` keeps them there.

Still weak (logged as low confidence): nickel alloys and CFRP have no published end-mill coefficient set; break-in amounts
for Al, C45 and CFRP have no direct measurement; milling-chuck and polygon TIR and the hydraulic damping bonus are guesses.
A structural gap is logged as `wear_ae_scaling_gap`: at constant chip thickness measured life scales as ae^−1
(Ti-6Al-4V, R² 0.97) while this model scales with the engaged-time fraction (≈ ae^0.5), so it can under-read life at light
radial engagement on heat-dominated materials. Coolant cooling of steels (measured −10…−20 % flood, −5…−15 % MQL) is
also weaker in the model (≈ 0–6 %).

### Thermal Model

* Boothroyd–Scott partition → chip/workpiece heat split
* Loewen–Shaw convective rise with Péclet number
* Coolant film coefficient (jet velocity, boiling regime: nucleate / film / Leidenfrost)
* Coating thermal barrier (steady + transient contact resistance)
* Tool body temperature → radial/axial thermal growth (µm)
* Workpiece lumped-capacitance steady-state rise
* Workpiece-surface convection by coolant (dry 30 · air blast 250 · MQL/mist 500 · flood 2,500 × flow/12 l/min ·
  through-tool 3,000 W/m²K). It sets how fast the heat of earlier passes decays before the tool comes back
  (toolpath preheat), the bulk workpiece temperature and the cool-down dwell. Values follow published machining
  heat-transfer coefficients (dry 7–100, air jet ≈ 540, MQL ≈ 560–1,000, water-based flood 1,000–2,700, oil flood up
  to 10,000, liquid jets 10³–10⁴: Rai & Xirouchakis 2009, Brandão et al. 2010, Shokrani et al. 2021,
  Stegmann et al. 2024). The previous through-tool value (320) was air-jet level and let earlier passes pre-heat
  nickel and titanium alloys by 150–335 K.
* Thermal shock index (interrupted cutting + quench severity)

### Wear Model

* 4 mechanisms: abrasion (hard-phase indentation), adhesion (sticking × load), diffusion/ dissolution (Arrhenius, two-zone activation), oxidation (environment-dependent)
* Reference-state normalisation (AlTiN, flood, 10 mm, S45C baseline)
* 3-phase trajectory: break-in (exponential decay, pair-specific size and time constant) → steady → accelerated (force-temperature feedback loop, 1500-step ODE)
* Coating compatibility factor, runout factor, chip-evacuation factor
* Calibration via SDS ledger (measured/predicted ratio, shrinkage estimator)

### Dynamics

* Altintas–Budak analytic stability lobes (2-branch eigenvalue solution)
* FRF: cantilever beam + holder spring; measured tap-test override with stickout scaling
* Process damping (wavelength vs. flank wear land, material factor × clearance factor)
* Variable pitch gain
* Robust lobe search (±2 % RPM band)
* Forced vibration: harmonic decomposition of Fx/Fy at tooth-passing harmonics → SLE

### Toolpath Load Simulation

* Voxel grid (cell ≤ min(D/20, allowance/3), capped by the cell budget) with per-cell mean height. The tool disc covers a
  cell *fractionally* (box filter on the signed distance, nested coverage per cell, stock-edge cells keep their material
  fraction), so removed volume, radial engagement `ae`, engagement angle and cutting time do not depend on how the cell
  centres happen to fall against the tool edge. Earlier versions only tested cell centres: with the default step (cell/2)
  half of the cutting steps removed nothing and were booked as air — cutting time halved and `ae` doubled (aluminium
  roughing preset on `rect_profile`: ae 0.99 mm instead of 0.50 mm, 12.9 s instead of 25.7 s) — and finishing presets whose
  cell exceeded the allowance read `ae` at 0.5–0.63× the geometry. Through-cuts clamp the axial engagement to the stock
  thickness (CFRP 6 mm plate / ap 10 mm used to read ap = 10 mm and ae = 0.6×).
* 6 patterns with arc interpolation, helical entry, multi-pass contour
* Per-step: engagement angle, mechanistic force, thermal, wear accumulation, wall/floor error
* Episode segmentation (k-distance on power/force/deflection/temp/engagement)
* Adaptive feed-scale per episode
* Thin-wall compliance (plate bending, thickness search)
* G00 collision & G01 LOC-exceed warnings
* The synthetic pocket pattern clamps its stepover to 0.2–0.6·D (multi-pass contour 0.1–0.4·D) and leaves a 1 mm wall
  to the stock edge. A finishing preset (ae ≈ 0.03–0.1·D) is therefore cut like pocket roughing at several times its
  radial engagement; the simulation now adds a warning when the pattern stepover differs from the preset ae by more
  than 1.5×. Use the rectangular profile pattern to judge finishing conditions.

### Simulation Audit — cross-checking the logs (`src/simaudit.rs`)

Every pipeline run builds a `SimAudit`. It is saved under the `audit` key of `wear_logs/*.json` (app and
`test_cuda.bat`), printed as `[AUDIT]` lines in the test logs, and can be produced from the CLI:
`cargo run --features cuda --example sim_audit -- <preset index> <pattern>` (no argument lists the presets).
It never changes a result. It re-reads the numbers of one job and states where they disagree, or where the model is
outside its validity range:

| Section | What it records |
| --- | --- |
| `inputs` | tool / workpiece / conditions / machine / coolant / calibration actually used (a JSON is self-contained) |
| `engagement` | planned `ae` vs. the `ae` the path really cuts (model, by volume, geometric expectation for `rect_profile`), grid cell / step / window, cutting length · time · volume |
| `force` | nominal (preset `ae`) vs. executed (path `ae`) force · power · specific energy · chip thickness, edge-force share, axial force mean / peak, cutting coefficients |
| `thermal` | interface dry / wet, cooling effectiveness, film and chip-conduction coefficients, tool-body rise, path temperatures |
| `wear` | job VB split into **steady** and **break-in**, break-in VB / τ and its progress, κ, path factor k, nominal vs. path life, chatter factors, how "parts until replacement" is built (physical path trajectory vs. the naïve job-average rate), the VB at which the radial wear uses up the tolerance (`vb_dim`) and the part count by that criterion, and — for aluminium Ø ≤ 8 mm — the steady rate / life / volume / specific wear against published bands (yellow outside the band; red only on the short-life side, because published tests are biased toward short lives) |
| `dynamics` | modal estimate, tooth-passing vs. natural frequency, margins (executed / nominal / path minimum), ZOA validity at low radial immersion (valid from ae/D ≈ 0.25), whether chatter reaches the wall error, and a reference band for the chatter marks that error leaves out (0.15·h_max at the limit … h_max when the tool leaves the cut) |
| `deflection` | tool-tip compliance split (beam vs. holder), wall deflection nominal / path, the normal force at the moment the wall is cut for **up vs. down milling** at the executed `ae` (and the share of the path cut in each mode), the runout over-cut the wall error leaves out, fixture and thermal terms |
| `flags[]` | machine-readable warnings: `id`, `severity` (0 info · 1 caution · 2 do not trust), `value`, `expected`, `message` |

Reading the wear numbers: the VB growth of a short job is dominated by the break-in term (VB_b = 15 % of the limit,
τ = 3 % of the linear life), so the CSV/JSON "steady" rate is *steady + break-in*; `wear_breakin_dominates` says so, and
`parts_left` comes from the physical path trajectory (steady wear only after break-in), not from that average.
τ = 3 % of the linear life is an assumption; where it leaves the published 0.5–5 min range (long-life finishing
conditions) `wear_breakin_tau_assumed` says so. The per-bin CSV gained
`ae_mm, ap_mm, fz_mm, h_max_um, vb_rate_um_min, breakin_um_min, chatter_wear, defl_um`.

Flags that are deliberately *not* alarms: a specific energy above 8 J/mm³ at a mean chip thickness under 15 µm is the
size effect (info); a ball-nose tool has no straight-wall `ae` expectation (`engage_geometry_ball_nose`, length and time
are still compared); chatter marks are never added to the wall error — when the margin is below 1 the audit prints a
reference band for them and marks the wall error as the stable-cut value (`dynamics_chatter_not_in_wall`).

### Physics Cross-Check (MPM) — optional, off by default, not experimentally validated

`crates/endmill_mpm` is a small 2D plane-strain orthogonal-cutting solver (MLS-MPM / APIC particles,
Hencky elasticity with von Mises return mapping, Johnson–Cook flow stress, Taylor–Quinney plastic heating,
heat conduction on the grid, a rigid wedge with a rounded edge and Coulomb friction, mass scaling, rayon threads).
`src/physcal.rs` runs it with the active profile's workpiece Johnson–Cook constants, tool rake / clearance /
edge radius, effective cutting speed, friction coefficient and a chip thickness taken from the job (clamped to
20–150 µm), then compares its cutting force `Fc` and tool–chip interface temperature rise with what the
analytic model above predicts for the *same* orthogonal cut (dry, new tool, no calibration).

* Run it from **정밀 분석 → ④ 마찰·냉각·마모 → 물리 교차검증 (MPM)**. It runs in the background (progress, ETA, cancel)
  and stores a record in `<app_data>/physcal/records.json`. Nothing changes until you opt in.
* Opt-in per record: **동력 보정 적용** (power) and, separately, **온도 보정도 적용** (temperature), with a weight
  (default 0.5, 0.25 when the record is flagged low reliability; the applied factor is clamped to 0.5–1.5).
* A record is applied only while the profile stays inside its envelope (same workpiece material and tool signature,
  Vc within ±35 %, rake within ±3°, edge radius within ±30 %) and only on axes where the SDS ledger has no real
  measured calibration — measurements always win.
* The applied factor goes through the same `Calibration` channel as measured calibrations
  (`pipeline::calibrated_context`), so the TTM wear forecast, the rule / LAYA decision and the second-pass G-code
  correction all see the changed force and temperature. It never writes to the SDS ledger or the replan anchor and
  never creates synthetic measurements; the pipeline divides the applied factor back out before it records real ones.
* Run length and diagnostics: the default run is long enough for the primary shear zone of thick-chip materials
  (travel ≥ 8h + 100 µm, workpiece margin ≥ 6h, free space above the stock ≥ travel/2 + 4h). A record is flagged low
  when the force is still drifting (`fc_trend` over 8 % between the two halves of the averaging window), when the chip
  reached the top of the grid, or when the chip ratio is outside 0.15–1.0 / shear angle outside 8–50°. The chip
  ratio comes from the chip speed (mass conservation, r = V_chip/V_c); for SUS304 it is compared with measured
  orthogonal-cutting values (r 0.23–0.51, Marcicano et al. 2005, Zhan et al. 2017) in the record's reasons.
* Reference (SUS304, h = 50 µm, Vc 132 m/min, rake 12°, edge 8 µm, μ 0.4): MPM Fc 194 N/mm (standard) / 187 N/mm
  (quick) vs. 186 N/mm from the analytic model, chip ratio 0.35 / 0.37, shear angle 20° / 21.5°. The default stainless
  profile (h = 32 µm, Vc 180 m/min, edge 12.4 µm, quick) gives Fc ×0.98 and an interface rise ×0.88 of the analytic
  values, chip ratio 0.27. An earlier, shorter default run (travel 300 µm, 150 µm margin) reported ~20 % lower forces
  and implausible chip ratios; records made before this change are
  ignored (record format 2).
* Limits: 2D (no helix or 3D engagement), no flank rubbing, coolant, tool heat conduction or wear, no segmented chips
  (titanium, Inconel and hardened steel are flagged low reliability), and thrust force is not compared. Only
  h = 32 and 50 µm on SUS304 have been checked against literature chip data. **There is no experimental validation** —
  treat it as a physics-based cross-check, not as a measurement.
* CLI without the app: `cargo run --release --example physcal_probe -- stainless quick [h_um] [--plan] [--threads N]`
  (same comparison, writes a JSON record) and
  `cargo run --release --manifest-path crates/endmill_mpm/Cargo.toml --bin mpmcut -- --h 50 --vc 2.2 --rake 12 --edge 8 --threads 4`
  (solver only).
* The solver is original code (same license as the project). It was cross-checked against an independent Taichi
  prototype of the same equations — an implementation check, not a physical validation.

---

## Project Structure

```text
endmill-sandbox/
├── Cargo.toml                  # library crate (endmill_model)
├── index.html                  # Tauri frontend (single-file SPA)
├── package.json / vite.config.js
├── src/
│   ├── lib.rs                  # public module re-exports
│   ├── physics.rs              # core analysis engine
│   ├── physcal.rs              # MPM physics cross-check (records · opt-in calibration)
│   ├── tribology.rs            # friction · coating · coolant chemistry
│   ├── dynamics.rs             # chatter lobes · forced vibration
│   ├── loadsim.rs              # toolpath load simulation (area-weighted stock grid)
│   ├── simaudit.rs             # simulation audit: log cross-checks · flags · [AUDIT] lines
│   ├── pairref.rs              # tool–workpiece pair reference values: friction, break-in, damping, holder runout
│   ├── toolimage.rs            # image processing (silhouette, edge)
│   ├── wear.rs                 # SigLIP2 wear comparison
│   ├── mold.rs                 # heightmap / wall CMM analysis
│   ├── sds.rs                  # statistical dynamics store
│   ├── decision.rs             # safety gates + advisor
│   ├── ingest.rs               # document parser
│   ├── pipeline.rs             # workspace orchestrator
│   ├── timeseries.rs           # forecast engine
│   ├── gcode.rs                # G-code generation
│   ├── profile.rs              # presets / profiles / machine limits
│   ├── cutting.rs              # speeds & feeds calculator
│   ├── spec.rs / coating.rs / material.rs / metadata.rs / application.rs
│   ├── workpiece_setup.rs
│   ├── ml/
│   │   ├── mod.rs / siglip.rs / ttm.rs / laya.rs / modernbert.rs
│   └── store/
│       ├── mod.rs / rdb.rs / lance.rs / vector.rs / series.rs
│       ├── attr.rs / record.rs
│   └── bin/main.rs             # CLI demo
├── crates/
│   └── endmill_mpm/            # 2D MPM orthogonal-cutting solver (library + `mpmcut` CLI)
├── src-tauri/
│   ├── Cargo.toml / build.rs / tauri.conf.json
│   └── src/
│       ├── main.rs             # Tauri entry
│       └── commands.rs         # 50+ IPC commands
├── tests/
│   ├── interaction_physics.rs / pipeline_flow.rs / store_flow.rs
│   ├── environment_tools.rs / tool_state.rs / wear_compensation.rs / physcal_flow.rs
│   └── vulkan_backend.rs       # Vulkan ↔ CPU parity (feature = "vulkan")
├── examples/physics_probe.rs / physcal_probe.rs / sim_audit.rs
├── run.bat / build.bat         # Windows CUDA helpers
├── _env_rocm.bat / _env_vulkan.bat  # shared MSVC / HIP SDK / protoc / Tauri CLI setup
├── {run,build,check,test}_{rocm,vulkan}.bat  # Windows ROCm / Vulkan helpers
├── test_linux.sh               # Linux test runner (cpu | vulkan | rocm | cuda)
└── .gitignore

```

---

## Configuration & Data Directories

| Path | Content |
| --- | --- |
| `<app_data>/profiles.json` | Legacy profile store (auto-migrated to SQLite) |
| `<app_data>/library/library.sqlite` | Projects, presets, profiles, end mills, workpieces, runs, metrics |
| `<app_data>/library/lancedb/` | LanceDB tables: `endmill_attr`, `workpiece_attr`, `cut_regime`, `tool_image_d*`, `mold_geom_d*`, `ts_points`, `ts_windows` |
| `<app_data>/sds/` | SDS ledgers: `tool.json`, `mold.json`, `process.json`, `decision.json`, `runs/` |
| `<app_data>/physcal/records.json` | MPM physics cross-check records (applied only when the user opts in) |
| `<app_data>/vectors/` | Saved SigLIP2 image records (`.json` + `.bin`) |
| `<app_data>/models/` | Default model root (override via `models_root.txt`) |

---

## Feature Flags

| Flag | Effect |
| --- | --- |
| `cuda` | Enables `candle-core/cuda` + `candle-nn/cuda` for GPU tensor ops |
| `rocm` | AMD ROCm/HIP backend via [`candle-rocm-vulkan`](https://github.com/Qntology/candle-rocm-vulkan) (`candle-rocm`, `candle-core/rocm`, `candle-nn/rocm`) |
| `vulkan` | Vulkan backend via the same fork (`candle-core/vulkan`, `candle-nn/vulkan`) |

`candle-core` / `candle-nn` are redirected to the fork with `[patch.crates-io]` (root and `src-tauri` `Cargo.toml`).
Set `ENDMILL_DEVICE=cpu` to force CPU, or `cuda` / `rocm` / `vulkan` to pick a specific backend among those compiled in.

---

## GPU Backends (ROCm / Vulkan)

The ROCm and Vulkan support follows the same layout as `logis-center/commerce` and uses the
[`candle-rocm-vulkan`](https://github.com/Qntology/candle-rocm-vulkan) fork.

**Device selection** (`src/ml/mod.rs`)
* `ml::device()` tries the compiled backends in order CUDA → ROCm → Vulkan and falls back to CPU.
  `ENDMILL_DEVICE` (`cpu` / `cuda` / `rocm` / `vulkan` / `gpu`) overrides the choice.
* The model status shows the device with its name, e.g. `Vulkan:0 (AMD Radeon RX 7900 XTX, discrete)` or `ROCm:0 (...)`.
* `ml::gpu_mem_info(&device)` returns `(free, total)` VRAM for ROCm (`hipMemGetInfo`) and Vulkan (heap budget) without NVML.

**Memory management**
* `ml::flush_device()` (called when a model is unloaded or a job finishes) no longer creates a device that was never used;
  it synchronizes CUDA/ROCm queues, then releases the ROCm cached resources / memory pool and trims the Vulkan buffer pool.
* After each inference call (SigLIP image/text embedding, laya `forward_ids`, TTM `forecast`) `ml::trim_idle_gpu_pool`
  returns pooled buffers: always on ROCm, and on Vulkan once the pool holds 64 MB or more.

**Precision**
* All models load their weights as F32 (`ml::load_varbuilder`), which is also the dtype the Vulkan backend runs natively,
  so no BF16/F16 switches are needed per backend.
* On ROCm, convolutions and `interpolate2d` (SigLIP patch embedding / position resize) run on the CPU and are copied back
  by the fork; everything else (matmul through rocBLAS, softmax, layer-norm, rope, gelu) runs on the GPU.
* Known numerical limit: candle-nn's `layer_norm` computes the variance in one pass (`E[x²]−E[x]²`), so rows with a very
  large mean lose precision on every backend (tracked as `BUG(LN-1)` in logis-center).

**Environment variables** (from the fork)

| Variable | Meaning |
| --- | --- |
| `ROCM_PATH` / `HIP_PATH`, `HIP_ARCH` / `CANDLE_ROCM_ARCHS` | ROCm SDK root and GPU targets (`gfx1100`, ...) |
| `CANDLE_ROCM_SKIP_KERNEL_BUILD=1` | type-check only, no HIP kernels |
| `CANDLE_VULKAN_DEVICE` | pick the Vulkan device whose name contains this substring |
| `CANDLE_VULKAN_ALLOW_CPU=1` | allow software Vulkan (lavapipe / SwiftShader), used by `test_linux.sh` |
| `CANDLE_VULKAN_QUEUE=graphics` | force the graphics queue instead of async compute |

The io_uring / DirectStorage SSD offloading in logis-center is for the LLM KV cache; this project only runs
encoder models without a KV cache, so that path is not used here.

---

## License

This project is licensed under the **Apache License, Version 2.0**.