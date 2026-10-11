# endmill_mpm

2D plane-strain orthogonal-cutting solver (MLS-MPM / APIC) used by the End Mill Sandbox physics cross-check
(`src/physcal.rs` in the parent project). It is a research model that gives the app's analytic cutting model
a *second calculation with different assumptions*. **It is not validated against experiments.**

앤드밀 샌드박스의 물리 교차검증이 쓰는 2차원 평면변형 직교절삭 해석기입니다. 앱의 해석식(Kienzle·Boothroyd 계열)을
다른 가정으로 한 번 더 풀어 비교하는 연구용 모델이며, 실험으로 검증되지 않았습니다.

## Model

| Part | Choice |
| --- | --- |
| Material | Hencky elasto-plasticity, von Mises return mapping (plane strain, out-of-plane elastic log-strain `e3` kept per particle) |
| Flow stress | Johnson–Cook (strain, strain rate, thermal softening), Taylor–Quinney β = 0.9 adiabatic heating |
| Heat | conduction on the grid with the real ρ·cp; the tool is adiabatic |
| Tool | rigid wedge (rake / clearance) with a rounded edge (offset-centre SDF), Coulomb friction on grid nodes |
| Time step | explicit; **mass scaling** (ρ × `mscale`, `mscale` ∝ 1/Vc²) instead of softening E; `ke_ratio_max` = kinetic / cumulative cutting work checks the quasi-static assumption |
| Parallelism | rayon thread pool (`threads`) |

Presets: `Preset::Quick` (chip thickness ≈ 10 cells, coarser) and `Preset::Standard` (≈ 12.5 cells).
`plan()` returns particle / step counts and `estimate_seconds()` a rough wall time before anything is run.

## Limits

2D plane strain · no cracks or segmented (saw-tooth) chips (titanium, Inconel, hardened steel are unreliable) ·
no tool heat conduction or wear · no coolant · no flank rubbing · grid ≈ 3–4 µm so thrust force depends on the
edge-radius resolution. Chip diagnostics (thickness ratio, shear angle) are heuristic and can fail when the
edge radius is large compared with the chip thickness.

## Use

```bash
# solver only (SUS304 constants by default; --jc A,B,n,C,m,Tm  --rho --cp --k --e --nu to change)
cargo run --release --bin mpmcut -- --h 50 --vc 2.2 --rake 12 --edge 8 --threads 4 --json out.json --particles p.csv
cargo run --release --bin mpmcut -- --quick --h 50 --vc 2.2        # coarser, ~3x faster

cargo test --release                                               # unit tests (a few seconds)
```

Reference run (SUS304, h = 50 µm, Vc = 2.2 m/s, rake 12°, edge 8 µm, standard preset): Fc ≈ 157 N/mm,
interface temperature ≈ 820 °C, chip ratio ≈ 0.61, shear angle ≈ 34°. An independent Taichi prototype of the same
equations gave 156 N/mm — an implementation check, not a physical validation.

Library entry points: `CutParams::{new, quick, for_conditions}`, `plan(&params)`, `run(&params, &progress)` where
`progress(fraction, message)` returns `false` to cancel, and `CutResult::to_json()`.
