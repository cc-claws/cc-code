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

// macOS 使用 macho ABI，jemalloc 的 configure 因此不定义 JEMALLOC_BACKGROUND_THREAD
// （configure.ac 的 `abi != macho` 条件），该平台不提供后台线程能力，默认配置也不写入
// background_thread，`opt.background_thread` 保持默认 false。
fn expected_config(
    dirty_decay_ms: isize,
    muzzy_decay_ms: isize,
    background_thread: bool,
) -> String {
    let background_thread = background_thread && cfg!(not(target_os = "macos"));
    format!("{dirty_decay_ms},{muzzy_decay_ms},{background_thread}")
}

#[tokio::test]
async fn test_jemalloc_startup_defaults() {
    let expected = expected_config(0, 0, true);
    make_probe_output(None, None, &expected).await;
}

#[tokio::test]
async fn test_jemalloc_startup_environment_override() {
    let expected = expected_config(1234, 5678, false);
    make_probe_output(
        Some("dirty_decay_ms:1234,muzzy_decay_ms:5678,background_thread:false"),
        Some("dirty_decay_ms:9999"),
        &expected,
    )
    .await;
}

#[tokio::test]
async fn test_jemalloc_startup_partial_override() {
    let expected = expected_config(1234, 0, true);
    make_probe_output(Some("dirty_decay_ms:1234"), None, &expected).await;
}

#[tokio::test]
async fn test_jemalloc_startup_ignores_unprefixed_environment() {
    let expected = expected_config(0, 0, true);
    make_probe_output(
        None,
        Some("dirty_decay_ms:9999,muzzy_decay_ms:9999,background_thread:false"),
        &expected,
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
    // 启动选项 opt.background_thread 在所有平台都存在，macOS 只是没有编译后台线程
    // 能力，因此该键仍可读、值为默认的 false。
    //
    // Safety: 键名 NUL 结尾，启动选项为 bool。
    let opt_background =
        unsafe { tikv_jemalloc_ctl::raw::read::<bool>(b"opt.background_thread\0") }
            .expect("应能读取 jemalloc 启动选项 opt.background_thread");
    assert_eq!(
        opt_background, background,
        "opt.background_thread 配置应生效"
    );
    // 运行时开关 background_thread 在 macOS 上返回 ENOENT：ctl_background_thread
    // 在 have_background_thread 为 false 时直接报错，无法断言。
    // Safety: 键名 NUL 结尾，运行时开关为 bool。
    match unsafe { tikv_jemalloc_ctl::raw::read::<bool>(b"background_thread\0") } {
        Ok(actual) => assert_eq!(actual, background, "background_thread 配置应生效"),
        Err(err) if cfg!(target_os = "macos") => {
            eprintln!("跳过 background_thread：macOS 的 jemalloc 未编译后台线程支持（{err}）");
        }
        Err(err) => panic!("应能读取 jemalloc 后台线程选项：{err}"),
    }
}
