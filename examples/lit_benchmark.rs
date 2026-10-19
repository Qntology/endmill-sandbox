//! 문헌 실측 벤치마크 — 논문 공구 + 내장 공구 × 논문 소재·조건 N:N 을 공칭 해석으로 돌리고 논문 실측(수명·체적·온도·힘·조도,
//! 코팅 비, 속도·반경 절입 민감도)과 대 본다. `[BENCH]` 비교 줄 · `[BENCH-SUM]` 양별 요약 · `[BENCH-GRID]` 수명 격자를 찍고 CSV/JSON 으로 저장한다.
//!
//!   cargo run --example lit_benchmark -- [출력 폴더 | -]
//!
//! 출력 폴더 기본값(`-` 포함)은 `ENDMILL_MATRIX_DIR` 또는 임시 폴더. 데이터와 근거는 `src/benchmark.rs`, 결과 해설은 `docs/literature-benchmark.md`
use endmill_model::benchmark;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dir = args
        .first()
        .filter(|d| d.as_str() != "-")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var("ENDMILL_MATRIX_DIR").ok().map(std::path::PathBuf::from))
        .unwrap_or_else(|| std::env::temp_dir().join("endmill-benchmark"));
    let _ = std::fs::create_dir_all(&dir);
    let started = std::time::Instant::now();
    let rep = benchmark::run();
    for l in benchmark::lines(&rep) {
        println!("{}", l);
    }
    let cmp = dir.join("literature_benchmark.csv");
    let cells = dir.join("literature_benchmark_cells.csv");
    let json = dir.join("literature_benchmark.json");
    std::fs::write(&cmp, benchmark::csv(&rep)).expect("CSV 저장");
    std::fs::write(&cells, benchmark::cells_csv(&rep)).expect("칸 CSV 저장");
    std::fs::write(&json, serde_json::to_string_pretty(&rep).unwrap_or_default()).expect("JSON 저장");
    let inb = rep.comparisons.iter().filter(|c| c.in_band).count();
    println!(
        "[BENCH] 비교 {} 건 · 대역 안 {} · 칸 {} · 점검 {} · {:.1} s · 저장 {} · {} · {}",
        rep.comparisons.len(),
        inb,
        rep.cells.len(),
        rep.issues.len(),
        started.elapsed().as_secs_f64(),
        cmp.display(),
        cells.display(),
        json.display()
    );
}
