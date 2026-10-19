use cc_middlewares::process::shell_command;

const PROBE_TEST: &str = "jemalloc_config_test::test_jemalloc_startup_probe";
const EXPECTED_CONFIG: &str = "CC_TEST_JEMALLOC_EXPECTED_CONFIG";

// jemalloc 在首次分配时固化选项，每种配置必须在独立进程启动前设置。
async fn make_probe_output(config: Option<&str>, legacy_config: Option<&str>, expected: &str) {
    let executable = std::env::current_exe().expect("应能定位当前测试程序");
    // 路径通过环境变量作为一个参数传递，不把路径插入 shell 脚本。
    let script =
        format!("exec \"$CC_TEST_JEMALLOC_EXECUTABLE\" --exact {PROBE_TEST} --ignored --nocapture");
    let mut command = shell_command(&script, &[]);
    command
        .env("CC_TEST_JEMALLOC_EXECUTABLE", executable)
        .env(EXPECTED_CONFIG, expected)
        .env_remove("_RJEM_MALLOC_CONF")
        .env_remove("MALLOC_CONF")
        .kill_on_drop(true);
    if let Some(config) = config {
        command.env("_RJEM_MALLOC_CONF", config);
    }
    if let Some(config) = legacy_config {
        command.env("MALLOC_CONF", config);
    }
    let output = tokio::time::timeout(std::time::Duration::from_secs(30), command.output())
        .await
        .expect("jemalloc 启动探针不应挂起")
        .expect("应能启动 jemalloc 探针");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success() && stdout.contains("1 passed"),
        "jemalloc 实际配置不符合预期：stdout={stdout}, stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[tokio::test]
async fn test_jemalloc_startup_defaults() {
    make_probe_output(None, None, "0,0,true").await;
}

#[tokio::test]
async fn test_jemalloc_startup_environment_override() {
    make_probe_output(
        Some("dirty_decay_ms:1234,muzzy_decay_ms:5678,background_thread:false"),
        Some("dirty_decay_ms:9999"),
        "1234,5678,false",
    )
    .await;
}

#[tokio::test]
async fn test_jemalloc_startup_partial_override() {
    make_probe_output(Some("dirty_decay_ms:1234"), None, "1234,0,true").await;
}

#[tokio::test]
async fn test_jemalloc_startup_ignores_unprefixed_environment() {
    make_probe_output(
        None,
        Some("dirty_decay_ms:9999,muzzy_decay_ms:9999,background_thread:false"),
        "0,0,true",
    )
    .await;
}

#[test]
#[ignore = "仅由父测试在隔离环境的子进程中执行"]
fn test_jemalloc_startup_probe() {
    let config = std::env::var(EXPECTED_CONFIG).expect("父测试应传入预期配置");
    let fields: Vec<_> = config.split(',').collect();
    let dirty: isize = fields[0].parse().expect("预期 dirty_decay_ms 应为整数");
    let muzzy: isize = fields[1].parse().expect("预期 muzzy_decay_ms 应为整数");
    let background: bool = fields[2]
        .parse()
        .expect("预期 background_thread 应为布尔值");
    for (name, expected) in [
        (b"opt.dirty_decay_ms\0".as_slice(), dirty),
        (b"arenas.dirty_decay_ms\0".as_slice(), dirty),
        (b"opt.muzzy_decay_ms\0".as_slice(), muzzy),
        (b"arenas.muzzy_decay_ms\0".as_slice(), muzzy),
    ] {
        // Safety: 键名 NUL 结尾，jemalloc 将这些选项定义为 ssize_t。
        let actual = unsafe { tikv_jemalloc_ctl::raw::read::<isize>(name) }
            .expect("应能读取 jemalloc decay 选项");
        assert_eq!(
            actual,
            expected,
            "{} 配置应生效",
            String::from_utf8_lossy(name)
        );
    }
    for name in [
        b"opt.background_thread\0".as_slice(),
        b"background_thread\0".as_slice(),
    ] {
        // Safety: 键名 NUL 结尾，启动选项与运行时开关均为 bool。
        let actual = unsafe { tikv_jemalloc_ctl::raw::read::<bool>(name) }
            .expect("应能读取 jemalloc 后台线程选项");
        assert_eq!(
            actual,
            background,
            "{} 配置应生效",
            String::from_utf8_lossy(name)
        );
    }
}
