use std::io;

use process_wrap::tokio::{ChildWrapper, CommandWrap, CommandWrapper, JobObject};
use tokio::process::Command;
use windows::Win32::System::Threading::{CREATE_NO_WINDOW, CREATE_SUSPENDED};

/// Own the job until the root exits or execution is cancelled.
/// Explicit termination avoids depending on process-wrap's wrapper lookups for
/// KillOnDrop (9.1 temporarily removes wrappers from the core during spawn).
pub(super) struct JobChild(Box<dyn ChildWrapper>);

impl JobChild {
    pub(super) fn child(&mut self) -> &mut dyn ChildWrapper {
        // Wait on the root while the owning job guard covers descendants.
        self.0.inner_mut()
    }
}

impl Drop for JobChild {
    fn drop(&mut self) {
        let _ = self.0.start_kill();
    }
}

#[derive(Debug)]
struct HiddenJob;

impl CommandWrapper for HiddenJob {
    fn pre_spawn(&mut self, command: &mut Command, _core: &CommandWrap) -> io::Result<()> {
        // Start suspended so descendants cannot escape job assignment. Set both
        // flags together instead of depending on CreationFlags wrapper ordering.
        command.creation_flags((CREATE_NO_WINDOW | CREATE_SUSPENDED).0);
        Ok(())
    }

    fn wrap_child(
        &mut self,
        child: Box<dyn ChildWrapper>,
        core: &CommandWrap,
    ) -> io::Result<Box<dyn ChildWrapper>> {
        // JobObject assigns the suspended child and resumes its threads.
        JobObject.wrap_child(child, core)
    }
}

pub(super) fn spawn(command: Command) -> io::Result<JobChild> {
    CommandWrap::from(command)
        .wrap(HiddenJob)
        .spawn()
        .map(JobChild)
}
