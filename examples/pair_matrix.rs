//! 공구 × 피삭재 N:N 행렬 (내장 공구 8 × 사용 사례 8) — 공칭 해석 + 경로 시뮬레이션(공구 수명 감시 포함)을 칸마다 돌려
//! `[MATRIX]` 줄·판정 격자를 찍고 CSV/JSON 으로 저장한다.
//!
//!   cargo run --features cuda --example pair_matrix -- [출력 폴더 | -] [경로 패턴[,패턴…] | none] [--check]
//!
//! 출력 폴더 기본값(`-` 포함)은 `ENDMILL_MATRIX_DIR` 또는 임시 폴더, 패턴 기본값은 rect_profile (none 이면 공칭 해석만).
//! `--check` 는 칸마다 물리 타당성·판정 규칙(`pairmatrix::check`)을 보고 `[MATRIX-CHECK]` 줄을 찍는다 —
//! 계산 결함(유한하지 않은 값, mm/min 단위 마모, 온도·VB 상한, 탄소계 코팅 × 철·Ti·Ni 가 '부적합'이 아님 …)이 하나라도 있으면 종료 코드 1.
use endmill_model::pairmatrix;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let check = args.iter().any(|a| a == "--check");
    let pos: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();
    let dir = pos
        .first()
        .filter(|d| d.as_str() != "-")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var("ENDMILL_MATRIX_DIR").ok().map(std::path::PathBuf::from))
        .unwrap_or_else(|| std::env::temp_dir().join("endmill-matrix"));
    let patterns: Vec<String> = pos.get(1).map(|p| p.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()).unwrap_or_else(|| vec!["rect_profile".into()]);
    let _ = std::fs::create_dir_all(&dir);
    let mut hard_total = 0usize;
    for pattern in patterns.iter() {
        let pat = if pattern == "none" { None } else { Some(pattern.as_str()) };
        let started = std::time::Instant::now();
        let cells = pairmatrix::run(pat);
        for l in pairmatrix::lines(&cells) {
            println!("{}", l);
        }
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
        if check {
            let issues = pairmatrix::check(&cells);
            for l in pairmatrix::check_lines(tag, &cells, &issues) {
                println!("{}", l);
            }
            let path = dir.join(format!("pair_matrix_{}_check.json", tag));
            std::fs::write(&path, serde_json::to_string_pretty(&issues).unwrap_or_default()).expect("점검 JSON 저장");
            hard_total += issues.iter().filter(|i| i.severity >= 2).count();
        }
    }
    if check && hard_total > 0 {
        eprintln!("[MATRIX-CHECK] 계산 결함 {} 건 — 위 줄과 pair_matrix_*_check.json 참고", hard_total);
        std::process::exit(1);
    }
}
