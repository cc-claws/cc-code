use std::{io, process::Stdio, time::Duration};

use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    process::{ChildStderr, ChildStdin, ChildStdout, Command},
};

#[cfg(windows)]
mod windows_job;

/// 根进程退出后，继承管道的后代不能无限延长输出收尾。
const PIPE_DRAIN_TIMEOUT: Duration = Duration::from_secs(2);

/// 持有受管理的进程；Windows 上同时持有 Job，drop 时终止全部后代。
/// 调用方取走管道后，应在根进程退出时 drop 本对象，再等待管道 EOF。
/// Unix 上每个被管理的子进程都是独立进程组组长（`process_group(0)`），
/// drop 时 kill 整个进程组，不留孤儿孙进程（#310）。
pub struct ManagedChild {
    #[cfg(windows)]
    child: windows_job::JobChild,
    #[cfg(not(windows))]
    child: tokio::process::Child,
    /// spawn 时记录的直接子进程 pid（= 进程组 pgid）。
    /// `Child::id()` 在 wait 后返回 None，这里存一份，保证 Drop 时总能 killpg。
    #[cfg(not(windows))]
    pid: u32,
}

impl ManagedChild {
    pub fn spawn(mut command: Command) -> io::Result<Self> {
        command.kill_on_drop(true);
        #[cfg(not(windows))]
        {
            // 独立进程组：后续 killpg 杀整棵进程树时不会误伤父进程自己。
            // （tokio::process::Command 自带 process_group 方法，无需 CommandExt。）
            command.process_group(0);
        }
        #[cfg(windows)]
        let child = windows_job::spawn(command)?;
        #[cfg(not(windows))]
        let child = command.spawn()?;
        #[cfg(not(windows))]
        let pid = child.id().expect("刚 spawn 的子进程一定有 pid");
        Ok(Self {
            child,
            #[cfg(not(windows))]
            pid,
        })
    }

    pub fn take_stdin(&mut self) -> Option<ChildStdin> {
        #[cfg(windows)]
        {
            self.child.child().stdin().take()
        }
        #[cfg(not(windows))]
        {
            self.child.stdin.take()
        }
    }

    pub fn take_stdout(&mut self) -> Option<ChildStdout> {
        #[cfg(windows)]
        {
            self.child.child().stdout().take()
        }
        #[cfg(not(windows))]
        {
            self.child.stdout.take()
        }
    }

    pub fn take_stderr(&mut self) -> Option<ChildStderr> {
        #[cfg(windows)]
        {
            self.child.child().stderr().take()
        }
        #[cfg(not(windows))]
        {
            self.child.stderr.take()
        }
    }

    pub async fn wait(&mut self) -> io::Result<std::process::ExitStatus> {
        #[cfg(windows)]
        {
            self.child.child().wait().await
        }
        #[cfg(not(windows))]
        {
            self.child.wait().await
        }
    }
}

#[cfg(not(windows))]
impl Drop for ManagedChild {
    fn drop(&mut self) {
        // 子进程是独立进程组组长（spawn 时 process_group(0)），pgid == pid，
        // killpg 不会误伤父进程。超时/取消后孙进程（如 sleep、后台任务）
        // 否则变孤儿（#310）。目标已退出时 killpg 返回 ESRCH，无害。
        unsafe {
            libc::killpg(self.pid as libc::pid_t, libc::SIGKILL);
        }
        // 之后 tokio 的 kill_on_drop 再补刀直接子进程（已死则为 no-op）。
    }
}

/// Execute once, feeding stdin while draining both output pipes.
/// The execution timeout covers the root process, independently of bounded pipe
/// draining. Dropping this future kills the whole process group on Unix
/// (via `ManagedChild`'s Drop → killpg) and the entire job on Windows.
/// This deliberately does not retry failed commands, which may have side effects.
pub(crate) async fn output_with_input_timeout(
    mut command: Command,
    input: &[u8],
    execution_timeout: Duration,
) -> io::Result<std::process::Output> {
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = ManagedChild::spawn(command)?;
    let (stdin, stdout, stderr) = (child.take_stdin(), child.take_stdout(), child.take_stderr());
    let write_input = async move {
        if let Some(mut stdin) = stdin {
            stdin.write_all(input).await?;
        }
        Ok::<_, io::Error>(())
    };
    let mut stdout_bytes = Vec::new();
    let mut stderr_bytes = Vec::new();
    let mut written = Ok(());
    let mut stdout_result = Ok(());
    let mut stderr_result = Ok(());
    let status = {
        // 使用当前 future 内的 IO，取消时一起 drop，不能留下分离的 reader/writer。
        // 缓冲放在外层，排空超时仍保留已经收到的拦截理由和命令输出。
        let io = async {
            tokio::join!(
                async { written = write_input.await },
                async { stdout_result = read_pipe(stdout, &mut stdout_bytes).await },
                async { stderr_result = read_pipe(stderr, &mut stderr_bytes).await },
            );
        };
        tokio::pin!(io);
        let (status, io_finished) = {
            let wait = tokio::time::timeout(execution_timeout, child.wait());
            tokio::pin!(wait);
            tokio::select! {
                biased;
                status = &mut wait => (status, false),
                _ = &mut io => (wait.await, true),
            }
        };
        // 先释放进程树归属，再等管道；Windows 后代持有的句柄也会因此关闭。
        drop(child);
        if !io_finished
            && tokio::time::timeout(PIPE_DRAIN_TIMEOUT, &mut io)
                .await
                .is_err()
        {
            tracing::warn!("根进程退出后管道排空超时，保留退出码和已捕获输出");
        }
        status
    };
    let status = status.map_err(|_| {
        io::Error::new(
            io::ErrorKind::TimedOut,
            "Shell command exceeded its execution timeout",
        )
    })??;
    // A hook may intentionally exit without consuming input, including exit 2.
    // Preserve its exit status instead of masking that decision with BrokenPipe.
    if let Err(error) = written {
        if error.kind() != io::ErrorKind::BrokenPipe {
            return Err(error);
        }
    }
    stdout_result?;
    stderr_result?;
    Ok(std::process::Output {
        status,
        stdout: stdout_bytes,
        stderr: stderr_bytes,
    })
}

async fn read_pipe(pipe: Option<impl AsyncRead + Unpin>, output: &mut Vec<u8>) -> io::Result<()> {
    if let Some(mut pipe) = pipe {
        pipe.read_to_end(output).await?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "capture_test.rs"]
mod tests;
