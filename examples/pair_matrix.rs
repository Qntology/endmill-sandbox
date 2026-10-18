//! 공구 × 피삭재 N:N 행렬 (내장 공구 8 × 사용 사례 8) — 공칭 해석 + 경로 시뮬레이션(공구 수명 감시 포함)을 칸마다 돌려
//! `[MATRIX]` 줄·판정 격자를 찍고 CSV/JSON 으로 저장한다.
//!
//!   cargo run --features cuda --example pair_matrix -- [출력 폴더] [경로 패턴 | none]
//!
//! 출력 폴더 기본값은 `ENDMILL_MATRIX_DIR` 또는 임시 폴더, 패턴 기본값은 rect_profile (none 이면 공칭 해석만).
use endmill_model::pairmatrix;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let dir = args
        .get(1)
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var("ENDMILL_MATRIX_DIR").ok().map(std::path::PathBuf::from))
        .unwrap_or_else(|| std::env::temp_dir().join("endmill-matrix"));
    let pattern = args.get(2).cloned().unwrap_or_else(|| "rect_profile".into());
    let pat = if pattern == "none" { None } else { Some(pattern.as_str()) };
    let started = std::time::Instant::now();
    let cells = pairmatrix::run(pat);
    for l in pairmatrix::lines(&cells) {
        println!("{}", l);
    }
    let _ = std::fs::create_dir_all(&dir);
    let tag = pat.unwrap_or("nominal");
    let csv = dir.join(format!("pair_matrix_{}.csv", tag));
    let json = dir.join(format!("pair_matrix_{}.json", tag));
    std::fs::write(&csv, pairmatrix::csv(&cells)).expect("CSV 저장");
    std::fs::write(&json, serde_json::to_string_pretty(&cells).unwrap_or_default()).expect("JSON 저장");
    let bad = cells.iter().filter(|c| c.verdict >= 2).count();
    let warn = cells.iter().filter(|c| c.verdict == 1).count();
    println!(
        "[MATRIX] {} 칸 · 적합 {} · 주의 {} · 부적합 {} · 경로 {} · {:.1} s · 저장 {} · {}",
        cells.len(),
        cells.len() - bad - warn,
        warn,
        bad,
        tag,
        started.elapsed().as_secs_f64(),
        csv.display(),
        json.display()
    );
}
