//! 단독 실행용 CLI. 앱 없이 같은 해석기를 돌려 값을 확인한다.
//!
//!   cargo run --release --bin mpmcut -- --h 50 --vc 2.2 --rake 12 --edge 8 --threads 4 --json out.json --particles p.csv
//!
//! 재료는 기본 SUS304(앱과 같은 상수)이며 --jc A,B,n,C,m,Tm(MPa,MPa,-,-,-,°C), --rho, --cp, --k, --e(GPa), --nu 로 바꿀 수 있다.
use endmill_mpm::{run, CutParams, Material};
use std::io::Write;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut quick = false;
    let mut prm = CutParams::new(Material::sus304());
    let mut json_path: Option<String> = None;
    let mut dump_path: Option<String> = None;
    let mut i = 0;
    let next = |i: &mut usize, name: &str| -> String {
        *i += 1;
        args.get(*i).cloned().unwrap_or_else(|| {
            eprintln!("{} 값이 없습니다", name);
            std::process::exit(2)
        })
    };
    let num = |s: String, name: &str| -> f64 {
        s.parse::<f64>().unwrap_or_else(|_| {
            eprintln!("{} 값이 숫자가 아닙니다: {}", name, s);
            std::process::exit(2)
        })
    };
    // 먼저 --quick 여부를 보고 기본값을 정한다
    if args.iter().any(|a| a == "--quick") {
        quick = true;
        prm = CutParams::quick(Material::sus304());
    }
    while i < args.len() {
        match args[i].as_str() {
            "--quick" => {}
            "--h" => prm.h_um = num(next(&mut i, "--h"), "--h"),
            "--vc" => prm.vc_m_s = num(next(&mut i, "--vc"), "--vc"),
            "--rake" => prm.rake_deg = num(next(&mut i, "--rake"), "--rake"),
            "--clear" => prm.clear_deg = num(next(&mut i, "--clear"), "--clear"),
            "--mu" => prm.mu = num(next(&mut i, "--mu"), "--mu"),
            "--edge" => prm.edge_um = num(next(&mut i, "--edge"), "--edge"),
            "--dx" => prm.dx_um = num(next(&mut i, "--dx"), "--dx"),
            "--mscale" => prm.mscale = num(next(&mut i, "--mscale"), "--mscale"),
            "--travel" => prm.travel_um = num(next(&mut i, "--travel"), "--travel"),
            "--wp-len" => prm.wp_len_um = num(next(&mut i, "--wp-len"), "--wp-len"),
            "--wp-thick" => prm.wp_thick_um = num(next(&mut i, "--wp-thick"), "--wp-thick"),
            "--threads" => prm.threads = num(next(&mut i, "--threads"), "--threads") as usize,
            "--eps-fail" => prm.eps_fail = num(next(&mut i, "--eps-fail"), "--eps-fail"),
            "--max-steps" => prm.max_steps = num(next(&mut i, "--max-steps"), "--max-steps") as usize,
            "--no-cond" => prm.conduction = false,
            "--rho" => prm.material.rho = num(next(&mut i, "--rho"), "--rho"),
            "--cp" => prm.material.cp = num(next(&mut i, "--cp"), "--cp"),
            "--k" => prm.material.k = num(next(&mut i, "--k"), "--k"),
            "--e" => prm.material.e_pa = num(next(&mut i, "--e"), "--e") * 1e9,
            "--nu" => prm.material.nu = num(next(&mut i, "--nu"), "--nu"),
            "--jc" => {
                let s = next(&mut i, "--jc");
                let v: Vec<f64> = s.split(',').map(|x| x.trim().parse::<f64>().unwrap_or(f64::NAN)).collect();
                if v.len() != 6 || v.iter().any(|x| !x.is_finite()) {
                    eprintln!("--jc 는 A,B,n,C,m,Tm 6개 숫자여야 합니다");
                    std::process::exit(2);
                }
                prm.material.jc_a = v[0] * 1e6;
                prm.material.jc_b = v[1] * 1e6;
                prm.material.jc_n = v[2];
                prm.material.jc_c = v[3];
                prm.material.jc_m = v[4];
                prm.material.t_melt_c = v[5];
            }
            "--json" => json_path = Some(next(&mut i, "--json")),
            "--particles" => {
                dump_path = Some(next(&mut i, "--particles"));
                prm.keep_particles = true;
            }
            other => {
                eprintln!("알 수 없는 옵션: {}", other);
                std::process::exit(2);
            }
        }
        i += 1;
    }
    let _ = quick;
    let t0 = std::time::Instant::now();
    let last = std::sync::Mutex::new(-1.0f64);
    let res = run(&prm, &|f, msg| {
        let mut l = last.lock().unwrap();
        if f - *l >= 0.05 {
            *l = f;
            eprintln!("[MPM] {:>3.0}% {} (경과 {:.0}s)", f * 100.0, msg, t0.elapsed().as_secs_f64());
        }
        true
    });
    match res {
        Ok(r) => {
            eprintln!(
                "[MPM] DONE steps={} particles={} wall={:.1}s | Fc={:.1}±{:.1} N/mm Ft={:.1} N/mm chip r={:.2} φ={:.1}° μ_app={:.2} τ={:.0} MPa | T chip {:.0}/{:.0} °C (평균/95%) 접촉면 {:.0}/{:.0} °C | Wp/Wtool={:.2} KE/W={:.3} steady={}",
                r.steps, r.particles, r.wall_s, r.fc_n_per_mm, r.fc_sd, r.ft_n_per_mm, r.chip_ratio, r.shear_angle_deg, r.mu_apparent, r.tau_shear_mpa, r.chip_t_mean_c, r.chip_t_p95_c,
                r.interface_t_mean_c, r.interface_t_p95_c, r.wp_over_wtool, r.ke_ratio_max, r.steady
            );
            for n in &r.notes {
                eprintln!("[MPM] 참고: {}", n);
            }
            let js = r.to_json();
            if let Some(p) = json_path {
                std::fs::write(&p, &js).expect("json 저장 실패");
            } else {
                println!("{}", js);
            }
            if let Some(p) = dump_path {
                let mut f = std::io::BufWriter::new(std::fs::File::create(&p).expect("particles 저장 실패"));
                writeln!(f, "x_um,y_um,ep,T_C,p_GPa").unwrap();
                for q in &r.particles_dump {
                    writeln!(f, "{:.3},{:.3},{:.4},{:.1},{:.4}", q[0], q[1], q[2], q[3], q[4]).unwrap();
                }
            }
        }
        Err(e) => {
            eprintln!("[MPM] 실패: {}", e);
            std::process::exit(1);
        }
    }
}
