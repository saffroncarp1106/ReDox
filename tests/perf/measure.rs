use std::time::Instant;

fn median(values: &[f64]) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    (sorted[(sorted.len() - 1) / 2] + sorted[sorted.len() / 2]) / 2.0
}

fn random(seed: &mut u64) -> u64 {
    *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
    *seed >> 32
}

pub fn timed(iterations: usize, mut work: impl FnMut()) -> f64 {
    let start = Instant::now();
    for _ in 0..iterations {
        work();
    }
    start.elapsed().as_secs_f64() * 1e9 / iterations as f64
}

pub fn paired(name: &str, mut before: impl FnMut() -> f64, mut after: impl FnMut() -> f64) {
    for _ in 0..3 {
        before();
        after();
    }
    let mut order: Vec<_> = (0..30).map(|i| i % 2 == 0).collect();
    let mut seed = 0x123456789abcdef;
    for i in (1..order.len()).rev() {
        order.swap(i, random(&mut seed) as usize % (i + 1));
    }
    let mut original = Vec::new();
    let mut revised = Vec::new();
    for original_first in order {
        let (a, b) = if original_first {
            (before(), after())
        } else {
            let b = after();
            (before(), b)
        };
        assert!(a > 0.0 && b > 0.0);
        original.push(a);
        revised.push(b);
    }
    let ratios: Vec<_> = original
        .iter()
        .zip(&revised)
        .map(|(a, b)| (a / b).ln())
        .collect();
    let speedup = (ratios.iter().sum::<f64>() / ratios.len() as f64).exp();
    let mut bootstrap = Vec::new();
    for _ in 0..5000 {
        let mean = (0..ratios.len())
            .map(|_| ratios[random(&mut seed) as usize % ratios.len()])
            .sum::<f64>()
            / ratios.len() as f64;
        bootstrap.push(mean.exp());
    }
    bootstrap.sort_by(f64::total_cmp);
    let interval = [bootstrap[125], bootstrap[4874]];
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    let result = serde_json::json!({
        "name":name, "profile":profile, "unit":"nanoseconds per operation",
        "original":original, "revised":revised,
        "original_median":median(&original), "revised_median":median(&revised),
        "geometric_mean_speedup":speedup, "paired_bootstrap_95_percent_interval":interval,
        "method":"30 paired batches; 3 warmup pairs; balanced randomized order; 5000 paired bootstrap resamples; one process, not end-to-end application performance"
    });
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("temp/codex-pipeline");
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(
        directory.join(format!("{profile}-{name}.json")),
        serde_json::to_vec_pretty(&result).unwrap(),
    )
    .unwrap();
    println!(
        "{name} [{profile}]: median original={:.3} us, revised={:.3} us; speedup={speedup:.2}x, paired 95% interval [{:.2}, {:.2}]",
        median(&original) / 1000.0,
        median(&revised) / 1000.0,
        interval[0],
        interval[1]
    );
}
