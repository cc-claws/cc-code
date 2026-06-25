//! 测真实 HTTPS 请求 50 轮的 RSS 累积
//! 这是验证"TLS/连接池累积"假设的关键测试
//! 跳过条件：没设置 ANTHROPIC_API_KEY 或网络不通

use std::time::Duration;

#[cfg(unix)]
fn current_rss_kb() -> usize {
    if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
        for line in status.lines() {
            if let Some(rest) = line.strip_prefix("VmRSS:") {
                if let Some(num) = rest.split_whitespace().next() {
                    if let Ok(n) = num.parse::<usize>() {
                        return n;
                    }
                }
            }
        }
    }
    0
}

#[cfg(not(unix))]
fn current_rss_kb() -> usize {
    0
}

#[tokio::test]
async fn measure_50_real_https_requests() {
    let api_key = std::env::var("ANTHROPIC_API_KEY")
        .or_else(|_| std::env::var("ANTHROPIC_AUTH_TOKEN"))
        .unwrap_or_default();
    let base_url = std::env::var("ANTHROPIC_BASE_URL")
        .unwrap_or_else(|_| "https://api.anthropic.com".to_string());
    let model =
        std::env::var("ANTHROPIC_MODEL").unwrap_or_else(|_| "claude-sonnet-4-6".to_string());

    if api_key.is_empty() {
        eprintln!("\n[SKIP] 未设置 ANTHROPIC_API_KEY/AUTH_TOKEN，跳过 50 轮真实 HTTPS 测试");
        return;
    }

    // 复用 peri 的 build_reqwest_client（pool_max_idle_per_host=1, idle 30s）
    let client = reqwest::Client::builder()
        .pool_max_idle_per_host(1)
        .pool_idle_timeout(Duration::from_secs(30))
        .build()
        .expect("client build");

    let baseline = current_rss_kb();
    println!("\n=== 50 轮真实 HTTPS 请求累积 ===");
    println!(
        "基线 RSS: {} KB ({:.2} MB)",
        baseline,
        baseline as f64 / 1024.0
    );
    println!("endpoint: {}", base_url);
    println!("model: {}", model);

    let mut prev_rss = baseline;
    for round in 1..=50 {
        let body = serde_json::json!({
            "model": model,
            "max_tokens": 50,
            "messages": [{"role": "user", "content": format!("第 {round} 轮：回复 'ok' 即可")}]
        });
        let url = format!("{}/v1/messages", base_url.trim_end_matches('/'));
        let resp = client
            .post(&url)
            .header("x-api-key", &api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .timeout(Duration::from_secs(30))
            .send()
            .await;

        let status = match &resp {
            Ok(r) => r.status().as_u16(),
            Err(e) => {
                eprintln!("[轮 {round}] 请求失败: {e}");
                0
            }
        };
        // 读完 body 避免连接 keep-alive 失效
        if let Ok(r) = resp {
            let _ = r.text().await;
        }

        if round % 10 == 0 {
            let rss = current_rss_kb();
            let delta = rss.saturating_sub(prev_rss);
            let total = rss.saturating_sub(baseline);
            println!(
                "轮 {round:>2}: RSS = {} KB ({:>6.2} MB)  +{} KB 本轮，累计 +{} KB ({:.2} MB) [last status {status}]",
                rss, rss as f64 / 1024.0, delta, total, total as f64 / 1024.0,
            );
            prev_rss = rss;
        }
    }

    // 等 1 秒，让 hyper connection pool idle timeout 起作用
    tokio::time::sleep(Duration::from_secs(2)).await;
    let after_wait = current_rss_kb();
    println!(
        "\n等 2s 后（hyper idle cleanup）: RSS = {} KB ({:+} KB 相对基线，jemalloc/hyper 持有的部分)",
        after_wait,
        after_wait as isize - baseline as isize,
    );

    drop(client);
    tokio::time::sleep(Duration::from_millis(500)).await;
    let after_drop = current_rss_kb();
    println!(
        "drop client + 等 500ms: RSS = {} KB ({:+} KB 相对基线)\n",
        after_drop,
        after_drop as isize - baseline as isize,
    );
}
