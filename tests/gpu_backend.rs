//! GPU 백엔드 대조 (feature = "cuda" · "rocm" · "vulkan") — endmill 모델(SigLIP2 · ModernBERT/laya · TTM)이 쓰는 연산을
//! 이 빌드의 GPU 장치(`ml::new_gpu_device(0)`)에서 실행해 CPU 결과와 비교한다. test_cuda.bat · test_rocm.bat · test_vulkan.bat 의
//! 15단계가 같은 대조를 돌려 백엔드마다 `[GPU]` 줄(연산별 최대 오차 · 행렬곱 시간)을 남긴다.
//! Vulkan 은 셰이더 경로를 강제한 대조가 tests/vulkan_backend.rs 에 따로 있다 (16단계).
//!
//! 절삭 시뮬레이션(힘·열·마모·공구 수명·N:N 행렬)은 백엔드와 관계없이 CPU 에서 돈다 — 여기서는 ML 연산만 본다.
#![cfg(any(feature = "cuda", feature = "rocm", feature = "vulkan"))]

use candle_core::{DType, Device, IndexOp, Module, Tensor, D};
use candle_nn::{VarBuilder, VarMap};
use endmill_model::ml;
use endmill_model::ml::modernbert::{ModernBert, ModernBertConfig};
use std::collections::HashMap;

type R = anyhow::Result<()>;

const BACKEND: &str = if cfg!(feature = "cuda") {
    "cuda"
} else if cfg!(feature = "rocm") {
    "rocm"
} else {
    "vulkan"
};

fn gpu() -> Device {
    let d = ml::new_gpu_device(0).unwrap_or_else(|| {
        panic!(
            "[GPU] {BACKEND}: GPU 장치를 열지 못함 — 드라이버·런타임(CUDA toolkit / HIP SDK / vulkan-1.dll)과 ENDMILL_DEVICE 를 확인. \
             GPU 없는 Vulkan 은 CANDLE_VULKAN_ALLOW_CPU=1"
        )
    });
    assert!(!d.is_cpu(), "[GPU] {BACKEND}: expected a GPU device, got {d:?}");
    d
}

fn to_vec_f64(t: &Tensor) -> anyhow::Result<Vec<f64>> {
    Ok(t.to_device(&Device::Cpu)?.to_dtype(DType::F64)?.flatten_all()?.to_vec1::<f64>()?)
}

/// 원소별 |got-want| <= tol * max(|got|,|want|,1) 를 확인하고 최대 상대 오차를 돌려준다
fn assert_close(got: &Tensor, want: &Tensor, tol: f64, what: &str) -> anyhow::Result<f64> {
    assert_eq!(got.dims(), want.dims(), "{what}: shape mismatch");
    let g = to_vec_f64(got)?;
    let w = to_vec_f64(want)?;
    let mut worst = 0.0f64;
    for (i, (x, y)) in g.iter().zip(w.iter()).enumerate() {
        if x == y || (x.is_nan() && y.is_nan()) {
            continue;
        }
        assert!(x.is_finite(), "{what}: non-finite value {x} at {i} (cpu {y})");
        let scale = x.abs().max(y.abs()).max(1.0);
        let rel = (x - y).abs() / scale;
        worst = worst.max(rel);
        assert!(rel <= tol, "{what}: mismatch at {i}: {BACKEND} {x} vs cpu {y}");
    }
    Ok(worst)
}

fn randn(shape: &[usize]) -> anyhow::Result<Tensor> {
    Ok(Tensor::randn(0f32, 1f32, shape, &Device::Cpu)?)
}

/// 같은 연산을 CPU 와 GPU 에서 실행해 비교하고 `[GPU]` 줄을 남긴다
fn same_on_both(dev: &Device, inputs: &[&Tensor], tol: f64, what: &str, f: impl Fn(&[Tensor]) -> candle_core::Result<Tensor>) -> R {
    let cpu_in: Vec<Tensor> = inputs.iter().map(|t| (*t).clone()).collect();
    let gpu_in: Vec<Tensor> = inputs.iter().map(|t| t.to_device(dev)).collect::<Result<_, _>>()?;
    let want = f(&cpu_in)?;
    let got = f(&gpu_in)?;
    assert!(!got.device().is_cpu(), "{what}: result left the {BACKEND} device");
    let worst = assert_close(&got, &want, tol, what)?;
    println!("[GPU] {BACKEND} · {what}: 최대 상대 오차 {worst:.2e} (허용 {tol:.0e})");
    Ok(())
}

#[test]
fn device_selection_and_label() {
    let d = gpu();
    let label = ml::device_label(&d);
    let mem = ml::gpu_mem_info(&d).map(|(free, total)| format!("{:.1} / {:.1} GB 여유", free as f64 / 1e9, total as f64 / 1e9)).unwrap_or_else(|| "메모리 조회 없음".into());
    println!("[GPU] {BACKEND} · 장치 {label} · {} · {mem}", ml::gpu_name(&d).unwrap_or_else(|| "-".into()));
    assert_ne!(label, "CPU");
    assert_eq!(ml::device_label(&Device::Cpu), "CPU");
}

#[test]
fn attention_patterns_used_by_models() -> R {
    let dev = gpu();
    let q = randn(&[2, 4, 9, 16])?;
    let k = randn(&[2, 4, 9, 16])?;
    let v = randn(&[2, 4, 9, 16])?;
    let n = 9usize;
    let mut local = vec![0f32; n * n];
    for i in 0..n {
        for j in 0..n {
            if (i as i64 - j as i64).unsigned_abs() > 2 {
                local[i * n + j] = f32::NEG_INFINITY;
            }
        }
    }
    let local = Tensor::from_vec(local, (1, 1, n, n), &Device::Cpu)?;
    let mut pad = vec![0f32; 2 * n];
    pad[n + 7] = -1e30;
    pad[n + 8] = -1e30;
    let pad = Tensor::from_vec(pad, (2, 1, 1, n), &Device::Cpu)?;
    same_on_both(&dev, &[&q, &k, &v, &local, &pad], 1e-4, "masked attention (ModernBERT 창 + 패딩)", |t| {
        let att = (t[0].matmul(&t[1].transpose(D::Minus2, D::Minus1)?)? * 0.25)?;
        let att = att.broadcast_add(&t[3])?.broadcast_add(&t[4])?;
        let att = candle_nn::ops::softmax_last_dim(&att)?;
        att.matmul(&t[2])?.transpose(1, 2)?.reshape((2, n, 64))
    })?;
    same_on_both(&dev, &[&q, &k], 1e-4, "k.t() attention scores", |t| t[0].matmul(&t[1].t()?)? / 4.0)
}

#[test]
fn norm_and_activation_ops_used_by_models() -> R {
    let dev = gpu();
    let x = randn(&[2, 6, 96])?;
    let w = ((randn(&[96])? * 0.1)? + 1.0)?;
    let b = (randn(&[96])? * 0.1)?;
    same_on_both(&dev, &[&x, &w, &b], 1e-4, "LayerNorm (bias)", |t| candle_nn::LayerNorm::new(t[1].clone(), t[2].clone(), 1e-6).forward(&t[0]))?;
    same_on_both(&dev, &[&x, &w], 1e-4, "LayerNorm (no bias, ModernBERT)", |t| candle_nn::LayerNorm::new_no_bias(t[1].clone(), 1e-5).forward(&t[0]))?;
    same_on_both(&dev, &[&x], 1e-4, "gelu_erf", |t| t[0].gelu_erf())?;
    same_on_both(&dev, &[&x], 1e-4, "gelu(tanh) + relu", |t| t[0].gelu()? + t[0].relu()?)?;
    same_on_both(&dev, &[&x], 1e-5, "softmax_last_dim", |t| candle_nn::ops::softmax_last_dim(&t[0]))?;
    same_on_both(&dev, &[&x], 1e-4, "TTM 표준화 (mean/var → broadcast)", |t| {
        let mean = t[0].mean_keepdim(D::Minus1)?;
        let xc = t[0].broadcast_sub(&mean)?;
        let denom = (xc.sqr()?.mean_keepdim(D::Minus1)? + 1e-5)?.sqrt()?;
        xc.broadcast_div(&denom)
    })
}

#[test]
fn rope_and_indexing_ops_used_by_models() -> R {
    let dev = gpu();
    let q = randn(&[1, 4, 8, 16])?;
    let cos = randn(&[8, 8])?.cos()?;
    let sin = randn(&[8, 8])?.sin()?;
    let table = randn(&[32, 24])?;
    let idx = Tensor::new(&[3u32, 0, 31, 7, 7], &Device::Cpu)?;
    same_on_both(&dev, &[&q, &cos, &sin], 1e-4, "rotary_emb::rope (ModernBERT)", |t| candle_nn::rotary_emb::rope(&t[0].contiguous()?, &t[1], &t[2]))?;
    same_on_both(&dev, &[&table, &idx], 1e-6, "index_select (TTM 표 · 임베딩)", |t| t[0].index_select(&t[1], 0))?;
    same_on_both(&dev, &[&table], 1e-6, "narrow · i() · permute", |t| t[0].i((2..10, ..))?.narrow(1, 4, 8)?.reshape((2, 4, 8))?.permute((2, 0, 1))?.contiguous())
}

#[test]
fn siglip_patch_embedding_and_position_interpolation() -> R {
    let dev = gpu();
    let pixel = randn(&[1, 3, 32, 32])?;
    let w = (randn(&[24, 3, 8, 8])? * 0.05)?;
    let pos = randn(&[1, 24, 4, 4])?;
    same_on_both(&dev, &[&pixel, &w], 1e-4, "conv2d patch embed (SigLIP2)", |t| t[0].conv2d(&t[1], 0, 8, 1, 1)?.flatten_from(2)?.transpose(1, 2))?;
    same_on_both(&dev, &[&pos], 1e-5, "interpolate2d (위치 임베딩 리사이즈)", |t| t[0].interpolate2d(6, 6))
}

fn tiny_modernbert_cfg() -> ModernBertConfig {
    ModernBertConfig::from_json(&serde_json::json!({
        "vocab_size": 64,
        "hidden_size": 32,
        "num_hidden_layers": 3,
        "num_attention_heads": 4,
        "intermediate_size": 48,
        "local_attention": 4,
        "global_attn_every_n_layers": 2,
        "norm_bias": false,
        "attention_bias": false,
        "mlp_bias": false
    }))
    .expect("tiny ModernBERT config")
}

#[test]
fn tiny_modernbert_matches_cpu() -> R {
    let dev = gpu();
    let cfg = tiny_modernbert_cfg();
    let vm = VarMap::new();
    let cpu_model = ModernBert::load(&cfg, VarBuilder::from_varmap(&vm, DType::F32, &Device::Cpu))?;
    let weights: HashMap<String, Tensor> = vm.data().lock().unwrap().iter().map(|(k, v)| (k.clone(), v.as_tensor().clone())).collect();
    let ids = Tensor::new(&[[1u32, 5, 9, 2, 33, 7, 0, 0], [4, 8, 15, 16, 23, 42, 63, 3]], &Device::Cpu)?;
    let mask = Tensor::new(&[[1u32, 1, 1, 1, 1, 1, 0, 0], [1, 1, 1, 1, 1, 1, 1, 1]], &Device::Cpu)?;
    let want = cpu_model.forward(&ids, Some(&mask))?;
    let w: HashMap<String, Tensor> = weights.iter().map(|(k, v)| Ok((k.clone(), v.to_device(&dev)?))).collect::<anyhow::Result<_>>()?;
    let model = ModernBert::load(&cfg, VarBuilder::from_tensors(w, DType::F32, &dev))?;
    let got = model.forward(&ids.to_device(&dev)?, Some(&mask.to_device(&dev)?))?;
    assert!(!got.device().is_cpu());
    let worst = assert_close(&got, &want, 1e-3, "tiny ModernBERT hidden states")?;
    println!("[GPU] {BACKEND} · tiny ModernBERT (laya 인코더) 은닉 상태: 최대 상대 오차 {worst:.2e} (허용 1e-3)");
    Ok(())
}

#[test]
fn matmul_matches_cpu_and_reports_speed() -> R {
    // 큰 행렬곱 한 번: 결과 대조 + 시간 (GPU 가 CPU 보다 느리면 장치가 소프트웨어 경로이거나 드라이버 문제)
    let dev = gpu();
    let a = randn(&[512, 512])?;
    let b = randn(&[512, 512])?;
    let t0 = std::time::Instant::now();
    let want = a.matmul(&b)?;
    let cpu_ms = t0.elapsed().as_secs_f64() * 1e3;
    let (ag, bg) = (a.to_device(&dev)?, b.to_device(&dev)?);
    let _ = ag.matmul(&bg)?.sum_all()?.to_scalar::<f32>()?; // 첫 호출(커널 준비)은 시간에서 뺀다
    let t1 = std::time::Instant::now();
    let got = ag.matmul(&bg)?;
    let _ = got.sum_all()?.to_scalar::<f32>()?;
    let gpu_ms = t1.elapsed().as_secs_f64() * 1e3;
    let worst = assert_close(&got, &want, 1e-3, "matmul 512x512")?;
    println!("[GPU] {BACKEND} · matmul 512×512: 최대 상대 오차 {worst:.2e} · GPU {gpu_ms:.2} ms · CPU {cpu_ms:.2} ms");
    Ok(())
}

#[test]
fn flush_device_does_not_break_the_device() -> R {
    let dev = gpu();
    {
        let a = Tensor::zeros((1024, 1024), DType::F32, &dev)?;
        let b = (a + 1.0)?;
        assert_eq!(b.sum_all()?.to_scalar::<f32>()?, (1024 * 1024) as f32);
    }
    ml::flush_device();
    let c = (Tensor::ones((256, 256), DType::F32, &dev)? * 2.0)?;
    assert_eq!(c.sum_all()?.to_scalar::<f32>()?, (2 * 256 * 256) as f32);
    println!("[GPU] {BACKEND} · flush_device 뒤에도 장치 사용 가능");
    Ok(())
}
