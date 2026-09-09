//! Windows job membership proof independent of process-wrap's completion packets.

use std::{io, sync::Arc, time::Duration};

use process_wrap::tokio::{CommandWrap, CommandWrapper, JobObject};
use tokio::process::{Child, Command};
use win32job::{ExtendedLimitInfo, Job};

const EXIT_TIMEOUT: Duration = Duration::from_secs(5);
const EXIT_POLL_INTERVAL: Duration = Duration::from_millis(25);

/// Retains the parent tracking job until every managed process has exited.
#[derive(Clone, Debug)]
pub(crate) struct TrackedJob(Arc<Job>);

impl TrackedJob {
    /// Attach after registering `JobObject` and before spawning. That wrapper
    /// creates the child suspended and resumes it only after `post_spawn`.
    pub(crate) fn attach(command: &mut CommandWrap) -> io::Result<Self> {
        // process-wrap 9.1 drains its wrapper map before running hooks, so this
        // prerequisite must be checked here rather than through a hook's core.
        if !command.has_wrap::<JobObject>() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "tracking job requires a suspended job-object spawn",
            ));
        }
        if command.has_wrap::<AssignTrackingJob>() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "a tracking job is already attached",
            ));
        }
        let mut limits = ExtendedLimitInfo::new();
        limits.limit_kill_on_job_close();
        let tracked = Self(Arc::new(
            Job::create_with_limit_info(&limits).map_err(io::Error::other)?,
        ));
        command.wrap(AssignTrackingJob(tracked.clone()));
        Ok(tracked)
    }

    /// The existing process wrapper issues termination. Only a successful empty
    /// membership query proves completion; query failures never mean "empty".
    pub(crate) async fn wait_for_exit(&self) -> io::Result<()> {
        tokio::time::timeout(EXIT_TIMEOUT, async {
            loop {
                if self
                    .0
                    .query_process_id_list()
                    .map_err(io::Error::other)?
                    .is_empty()
                {
                    return Ok(());
                }
                tokio::time::sleep(EXIT_POLL_INTERVAL).await;
            }
        })
        .await
        .map_err(|_| {
            io::Error::new(
                io::ErrorKind::TimedOut,
                "managed job termination was not confirmed",
            )
        })?
    }
}

#[derive(Debug)]
struct AssignTrackingJob(TrackedJob);

impl CommandWrapper for AssignTrackingJob {
    fn post_spawn(
        &mut self,
        _command: &mut Command,
        child: &mut Child,
        _core: &CommandWrap,
    ) -> io::Result<()> {
        // process-wrap 9.1 runs every post_spawn before any wrap_child hook.
        // Assign the parent job before JobObject creates its nested job and
        // resumes the child; no descendant can escape through a spawn race.
        let handle = child
            .raw_handle()
            .ok_or_else(|| io::Error::other("suspended managed process handle is unavailable"))?;
        self.0
             .0
            .assign_process(handle as isize)
            .map_err(io::Error::other)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use process_wrap::tokio::KillOnDrop;
    use std::process::Stdio;

    #[test]
    fn tracking_job_rejects_attachment_without_suspension_wrapper() {
        let mut command = CommandWrap::from(Command::new("unused-fixture.exe"));
        assert_eq!(
            TrackedJob::attach(&mut command).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
    }

    #[tokio::test]
    async fn tracking_job_rejects_duplicate_attachment_and_confirms_empty_job() {
        let mut command = CommandWrap::from(Command::new("unused-fixture.exe"));
        command.wrap(JobObject);
        let tracked = TrackedJob::attach(&mut command).unwrap();
        assert_eq!(
            TrackedJob::attach(&mut command).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
        tracked.wait_for_exit().await.unwrap();
    }

    #[tokio::test]
    async fn tracking_job_retains_descendant_after_leader_exit_and_drains_before_cleanup() {
        let fixture = tempfile::tempdir().unwrap();
        let staging = fixture.path().join("staging.part");
        tokio::fs::write(&staging, b"retained until job is empty")
            .await
            .unwrap();
        let mut command = CommandWrap::from(descendant_command());
        command.wrap(JobObject).wrap(KillOnDrop);
        let tracked = TrackedJob::attach(&mut command).unwrap();
        let mut child = command.spawn().unwrap();
        let leader = usize::try_from(child.id().unwrap()).unwrap();
        // Deliberately await only the leader to exercise orphaned membership.
        let status = tokio::time::timeout(Duration::from_secs(10), child.inner_mut().wait())
            .await
            .unwrap()
            .unwrap();
        assert!(status.success());
        assert!(tracked
            .0
            .query_process_id_list()
            .unwrap()
            .iter()
            .any(|pid| *pid != leader));
        assert!(tokio::fs::try_exists(&staging).await.unwrap());
        child.start_kill().unwrap();
        tracked.wait_for_exit().await.unwrap();
        assert!(tracked.0.query_process_id_list().unwrap().is_empty());
        tokio::fs::remove_file(&staging).await.unwrap();
        assert!(!tokio::fs::try_exists(&staging).await.unwrap());
    }

    #[tokio::test]
    async fn managed_wait_terminates_descendants_after_successful_leader_exit() {
        use crate::addon_tool_gate::{ToolLease, ToolLeaseMode};

        let fixture = tempfile::tempdir().unwrap();
        let tool = fixture.path().join("tool-identity");
        std::fs::write(&tool, b"identity only").unwrap();
        let tools = [tool];
        let root = fixture.path().join("gate");
        let lease = ToolLease::try_acquire(&root, &tools, ToolLeaseMode::Use)
            .unwrap()
            .unwrap();
        let mut child = crate::addons::spawn_managed_addon_process(descendant_command()).unwrap();
        child.retain_tool_lease(lease);
        assert!(ToolLease::try_acquire(&root, &tools, ToolLeaseMode::Update)
            .unwrap()
            .is_none());
        let status = tokio::time::timeout(Duration::from_secs(10), child.wait())
            .await
            .expect("must terminate the descendant rather than wait for its 30-second lifetime")
            .unwrap();
        assert!(status.success());
        ToolLease::try_acquire(&root, &tools, ToolLeaseMode::Update)
            .unwrap()
            .expect("confirmed job cleanup permits maintenance without offline recovery")
            .confirm_cleanup()
            .unwrap();
    }

    #[tokio::test]
    async fn managed_drop_retires_tool_receipt_after_descendant_job_cleanup() {
        use crate::addon_tool_gate::{ToolLease, ToolLeaseMode};

        let fixture = tempfile::tempdir().unwrap();
        let tool = fixture.path().join("tool-identity");
        std::fs::write(&tool, b"identity only").unwrap();
        let tools = [tool];
        let root = fixture.path().join("gate");
        let lease = ToolLease::try_acquire(&root, &tools, ToolLeaseMode::Use)
            .unwrap()
            .unwrap();
        let mut child = crate::addons::spawn_managed_addon_process(descendant_command()).unwrap();
        child.retain_tool_lease(lease);
        // The fixture leader exits only after starting its long-lived child.
        // Awaiting only the leader leaves descendant cleanup to Drop.
        let status = tokio::time::timeout(Duration::from_secs(10), child.inner_mut().wait())
            .await
            .unwrap()
            .unwrap();
        assert!(status.success());
        assert!(ToolLease::try_acquire(&root, &tools, ToolLeaseMode::Update)
            .unwrap()
            .is_none());
        drop(child);
        tokio::time::timeout(Duration::from_secs(7), async {
            loop {
                if let Some(lease) =
                    ToolLease::try_acquire(&root, &tools, ToolLeaseMode::Update).unwrap()
                {
                    lease.confirm_cleanup().unwrap();
                    break;
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await
        .expect("drop cleanup permits maintenance without offline recovery");
    }

    fn descendant_command() -> Command {
        let mut process = Command::new("powershell.exe");
        process.args([
            "-NoProfile", "-NonInteractive", "-Command",
            r#"$info = New-Object System.Diagnostics.ProcessStartInfo; $info.FileName = 'powershell.exe'; $info.Arguments = '-NoProfile -NonInteractive -Command "Start-Sleep -Seconds 30"'; $info.UseShellExecute = $false; $info.CreateNoWindow = $true; $child = [System.Diagnostics.Process]::Start($info); $child.Dispose()"#,
        ]).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        process
    }
}
