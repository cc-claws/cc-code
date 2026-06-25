//! 测量首次 HTTPS 请求到真实 Provider（GLM BigModel）的 RSS 成本
//! 排查 TLS 状态 + native-certs 加载 + HTTP/2 连接池是否为剩余 19MB 的大头

#![cfg(unix)]

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

fn print_rss(label: &str, baseline: &mut usize) {
    let rss = current_rss_kb();
    let delta = rss.saturating_sub(*baseline);
    println!(
        "[{:40}] RSS = {:>8} KB ({:6.2} MB) | +{:>6} KB ({:+.2} MB)",
        label,
        rss,
        rss as f64 / 1024.0,
        delta,
        delta as f64 / 1024.0
    );
    *baseline = rss;
}

#[tokio::test]
async fn measure_https_first_call_real_provider() {
    println!("\n=== 测量首次 HTTPS 请求到 GLM BigModel 的 RSS 成本 ===\n");

    let mut base = current_rss_kb();
    print_rss("基线（tokio runtime 已启动）", &mut base);

    // === 阶段 1：只构建 reqwest::Client（rustls TLS + native certs） ===
    println!("\n--- 阶段 1: 构建 reqwest::Client ---");
    let client = reqwest::Client::builder()
        .use_rustls_tls()
        .build()
        .expect("build client");
    print_rss("reqwest::Client::build() (rustls)", &mut base);

    // === 阶段 2：DNS 解析 ===
    println!("\n--- 阶段 2: DNS 解析 ---");
    let _ = tokio::net::lookup_host("open.bigmodel.cn:443").await;
    print_rss("DNS lookup open.bigmodel.cn:443", &mut base);

    // === 阶段 3：首次 TLS 握手（建立 socket + TLS state + HPACK） ===
    println!("\n--- 阶段 3: 首次 HTTPS GET（TLS 握手 + HTTP/2）---");
    let result = client.get("https://open.bigmodel.cn/").send().await;
    let _ = result.map(|r| r.status());
    print_rss("首次 HTTPS GET open.bigmodel.cn", &mut base);

    // === 阶段 4：第二次请求（验证连接池复用） ===
    println!("\n--- 阶段 4: 第二次请求（应命中连接池） ---");
    let result2 = client.get("https://open.bigmodel.cn/").send().await;
    let _ = result2.map(|r| r.status());
    print_rss("第二次 HTTPS GET", &mut base);

    // === 阶段 5：再发 5 次请求看是否持续涨 ===
    println!("\n--- 阶段 5: 再 5 次请求（确认是否稳定）---");
    for i in 1..=5 {
        let _ = client.get("https://open.bigmodel.cn/").send().await;
        print_rss(&format!("第 {}+2 次请求", i), &mut base);
    }

    // === 阶段 6：测试真实 Anthropic 兼容 endpoint（POST /api/anthropic/v1/messages） ===
    // 不带有效 body，预期 4xx，但能测出实际握手 + TLS 协商成本
    println!("\n--- 阶段 6: 模拟真实 API 调用（POST messages endpoint） ---");
    let result3 = client
        .post("https://open.bigmodel.cn/api/anthropic/v1/messages")
        .header("content-type", "application/json")
        .body(r#"{"model":"glm-5.2","messages":[],"max_tokens":1}"#)
        .send()
        .await;
    if let Ok(resp) = result3 {
        println!("  HTTP status: {}", resp.status());
        let _ = resp.bytes().await;
    }
    print_rss("POST messages endpoint", &mut base);

    println!("\n=== 结论 ===");
    let total_delta = current_rss_kb().saturating_sub(current_rss_kb());
    let _ = total_delta;
    println!("观察上方各阶段 RSS 增量，判断大头来自：");
    println!("- Client 构造（rustls + native certs）");
    println!("- 首次 TLS 握手");
    println!("- HTTP/2 HPACK 状态");
    println!("- 连接池保持");
    drop(client);
    print_rss("drop client 后", &mut base);
}

#[tokio::test]
async fn measure_https_to_other_domains() {
    //! 对比不同域名的 TLS 成本（看是否是 GLM 特有）
    println!("\n=== 对比不同域名 HTTPS 成本 ===\n");

    let mut base = current_rss_kb();
    print_rss("基线", &mut base);

    let client = reqwest::Client::builder().use_rustls_tls().build().unwrap();
    print_rss("Client 构造", &mut base);

    for domain in &[
        "https://open.bigmodel.cn/",
        "https://api.anthropic.com/",
        "https://api.openai.com/",
        "https://www.google.com/",
    ] {
        let _ = client.get(*domain).send().await;
        print_rss(&format!("GET {}", domain), &mut base);
    }
}
