use std::ffi::c_void;
use std::io;
use std::mem::{size_of, zeroed};
use std::os::windows::io::AsRawHandle;
use std::os::windows::process::CommandExt;
use std::process::{Child, Command};

use sunlight_core::repository::ExecutionPolicy;

type Handle = *mut c_void;
type Bool = i32;
type Dword = u32;
type UlongPtr = usize;

const CREATE_SUSPENDED: Dword = 0x0000_0004;
const JOB_OBJECT_EXTENDED_LIMIT_INFORMATION_CLASS: i32 = 9;
const JOB_OBJECT_ASSOCIATE_COMPLETION_PORT_INFORMATION_CLASS: i32 = 7;
const JOB_OBJECT_LIMIT_PROCESS_TIME: Dword = 0x0000_0002;
const JOB_OBJECT_LIMIT_JOB_TIME: Dword = 0x0000_0004;
const JOB_OBJECT_LIMIT_ACTIVE_PROCESS: Dword = 0x0000_0008;
const JOB_OBJECT_LIMIT_PROCESS_MEMORY: Dword = 0x0000_0100;
const JOB_OBJECT_LIMIT_JOB_MEMORY: Dword = 0x0000_0200;
const JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE: Dword = 0x0000_2000;
const JOB_OBJECT_MSG_END_OF_JOB_TIME: Dword = 1;
const JOB_OBJECT_MSG_END_OF_PROCESS_TIME: Dword = 2;
const JOB_OBJECT_MSG_ACTIVE_PROCESS_LIMIT: Dword = 3;
const JOB_OBJECT_MSG_PROCESS_MEMORY_LIMIT: Dword = 9;
const JOB_OBJECT_MSG_JOB_MEMORY_LIMIT: Dword = 10;
const TH32CS_SNAPTHREAD: Dword = 0x0000_0004;
const THREAD_SUSPEND_RESUME: Dword = 0x0000_0002;
const WAIT_TIMEOUT: Dword = 258;
const INVALID_HANDLE_VALUE: Handle = -1_isize as Handle;
const SUNLIGHT_JOB_TERMINATION_EXIT_CODE: Dword = 0xE000_0001;

#[repr(C)]
struct IoCounters {
    read_operation_count: u64,
    write_operation_count: u64,
    other_operation_count: u64,
    read_transfer_count: u64,
    write_transfer_count: u64,
    other_transfer_count: u64,
}

#[repr(C)]
struct JobObjectBasicLimitInformation {
    per_process_user_time_limit: i64,
    per_job_user_time_limit: i64,
    limit_flags: Dword,
    minimum_working_set_size: usize,
    maximum_working_set_size: usize,
    active_process_limit: Dword,
    affinity: UlongPtr,
    priority_class: Dword,
    scheduling_class: Dword,
}

#[repr(C)]
struct JobObjectExtendedLimitInformation {
    basic_limit_information: JobObjectBasicLimitInformation,
    io_info: IoCounters,
    process_memory_limit: usize,
    job_memory_limit: usize,
    peak_process_memory_used: usize,
    peak_job_memory_used: usize,
}

#[repr(C)]
struct JobObjectAssociateCompletionPort {
    completion_key: *mut c_void,
    completion_port: Handle,
}

#[repr(C)]
struct ThreadEntry32 {
    size: Dword,
    usage: Dword,
    thread_id: Dword,
    owner_process_id: Dword,
    base_priority: i32,
    delta_priority: i32,
    flags: Dword,
}

#[link(name = "kernel32")]
extern "system" {
    fn CreateJobObjectW(attributes: *const c_void, name: *const u16) -> Handle;
    fn SetInformationJobObject(
        job: Handle,
        information_class: i32,
        information: *const c_void,
        information_length: Dword,
    ) -> Bool;
    fn AssignProcessToJobObject(job: Handle, process: Handle) -> Bool;
    fn TerminateJobObject(job: Handle, exit_code: Dword) -> Bool;
    fn CreateIoCompletionPort(
        file_handle: Handle,
        existing_completion_port: Handle,
        completion_key: UlongPtr,
        concurrent_threads: Dword,
    ) -> Handle;
    fn GetQueuedCompletionStatus(
        completion_port: Handle,
        bytes_transferred: *mut Dword,
        completion_key: *mut UlongPtr,
        overlapped: *mut *mut c_void,
        milliseconds: Dword,
    ) -> Bool;
    fn CreateToolhelp32Snapshot(flags: Dword, process_id: Dword) -> Handle;
    fn Thread32First(snapshot: Handle, entry: *mut ThreadEntry32) -> Bool;
    fn Thread32Next(snapshot: Handle, entry: *mut ThreadEntry32) -> Bool;
    fn OpenThread(desired_access: Dword, inherit_handle: Bool, thread_id: Dword) -> Handle;
    fn ResumeThread(thread: Handle) -> Dword;
    fn CloseHandle(handle: Handle) -> Bool;
    fn GetLastError() -> Dword;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ResourceTermination {
    CpuTime,
    ProcessMemory,
    JobMemory,
    ActiveProcessLimit,
}

impl ResourceTermination {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::CpuTime => "cpu_time_limit",
            Self::ProcessMemory => "process_memory_limit",
            Self::JobMemory => "job_memory_limit",
            Self::ActiveProcessLimit => "active_process_limit",
        }
    }
}

pub(crate) struct ContainedChild {
    pub(crate) child: Child,
    job: OwnedHandle,
    completion_port: OwnedHandle,
}

#[derive(Debug)]
pub(crate) enum ContainmentSpawnError {
    Setup(io::Error),
    Command(io::Error),
}

struct OwnedHandle(Handle);

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        if !self.0.is_null() && self.0 != INVALID_HANDLE_VALUE {
            unsafe { CloseHandle(self.0) };
        }
    }
}

impl ContainedChild {
    pub(crate) fn spawn(
        mut command: Command,
        policy: &ExecutionPolicy,
    ) -> Result<Self, ContainmentSpawnError> {
        let job = OwnedHandle(unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) });
        if job.0.is_null() {
            return Err(ContainmentSpawnError::Setup(last_error(
                "create Windows Job Object",
            )));
        }
        let completion_port = OwnedHandle(unsafe {
            CreateIoCompletionPort(INVALID_HANDLE_VALUE, std::ptr::null_mut(), 0, 1)
        });
        if completion_port.0.is_null() {
            return Err(ContainmentSpawnError::Setup(last_error(
                "create Job Object completion port",
            )));
        }
        configure_job(job.0, completion_port.0, policy).map_err(ContainmentSpawnError::Setup)?;

        command.creation_flags(CREATE_SUSPENDED);
        let mut child = command.spawn().map_err(ContainmentSpawnError::Command)?;
        let process_handle = child.as_raw_handle() as Handle;
        if unsafe { AssignProcessToJobObject(job.0, process_handle) } == 0 {
            let error = last_error("assign suspended process to Windows Job Object");
            let _ = child.kill();
            let _ = child.wait();
            return Err(ContainmentSpawnError::Setup(error));
        }
        if let Err(error) = resume_process_threads(child.id()) {
            unsafe { TerminateJobObject(job.0, SUNLIGHT_JOB_TERMINATION_EXIT_CODE) };
            let _ = child.wait();
            return Err(ContainmentSpawnError::Setup(error));
        }
        Ok(Self {
            child,
            job,
            completion_port,
        })
    }

    pub(crate) fn terminate(&self) -> io::Result<()> {
        if unsafe { TerminateJobObject(self.job.0, SUNLIGHT_JOB_TERMINATION_EXIT_CODE) } == 0 {
            Err(last_error("terminate Windows Job Object"))
        } else {
            Ok(())
        }
    }

    pub(crate) fn poll_resource_termination(&self) -> io::Result<Option<ResourceTermination>> {
        let mut found = None;
        loop {
            let mut message = 0;
            let mut key = 0;
            let mut overlapped = std::ptr::null_mut();
            let ok = unsafe {
                GetQueuedCompletionStatus(
                    self.completion_port.0,
                    &mut message,
                    &mut key,
                    &mut overlapped,
                    0,
                )
            };
            if ok == 0 {
                let code = unsafe { GetLastError() };
                if code == WAIT_TIMEOUT {
                    return Ok(found);
                }
                return Err(io::Error::from_raw_os_error(code as i32));
            }
            found = found.or(match message {
                JOB_OBJECT_MSG_END_OF_JOB_TIME | JOB_OBJECT_MSG_END_OF_PROCESS_TIME => {
                    Some(ResourceTermination::CpuTime)
                }
                JOB_OBJECT_MSG_PROCESS_MEMORY_LIMIT => Some(ResourceTermination::ProcessMemory),
                JOB_OBJECT_MSG_JOB_MEMORY_LIMIT => Some(ResourceTermination::JobMemory),
                JOB_OBJECT_MSG_ACTIVE_PROCESS_LIMIT => {
                    Some(ResourceTermination::ActiveProcessLimit)
                }
                _ => None,
            });
        }
    }
}

fn configure_job(job: Handle, completion_port: Handle, policy: &ExecutionPolicy) -> io::Result<()> {
    let cpu_100ns = policy
        .cpu_time_limit_ms
        .checked_mul(10_000)
        .and_then(|value| i64::try_from(value).ok())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "CPU limit overflow"))?;
    let mut limits: JobObjectExtendedLimitInformation = unsafe { zeroed() };
    limits.basic_limit_information.per_process_user_time_limit = cpu_100ns;
    limits.basic_limit_information.per_job_user_time_limit = cpu_100ns;
    limits.basic_limit_information.active_process_limit = policy.active_process_limit;
    limits.basic_limit_information.limit_flags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
        | JOB_OBJECT_LIMIT_ACTIVE_PROCESS
        | JOB_OBJECT_LIMIT_PROCESS_TIME
        | JOB_OBJECT_LIMIT_JOB_TIME
        | JOB_OBJECT_LIMIT_PROCESS_MEMORY
        | JOB_OBJECT_LIMIT_JOB_MEMORY;
    limits.process_memory_limit =
        usize::try_from(policy.process_memory_limit_bytes).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidInput, "process memory limit overflow")
        })?;
    limits.job_memory_limit = usize::try_from(policy.job_memory_limit_bytes)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "job memory limit overflow"))?;
    if unsafe {
        SetInformationJobObject(
            job,
            JOB_OBJECT_EXTENDED_LIMIT_INFORMATION_CLASS,
            &limits as *const _ as *const c_void,
            size_of::<JobObjectExtendedLimitInformation>() as Dword,
        )
    } == 0
    {
        return Err(last_error("configure Windows Job Object resource limits"));
    }
    let association = JobObjectAssociateCompletionPort {
        completion_key: job,
        completion_port,
    };
    if unsafe {
        SetInformationJobObject(
            job,
            JOB_OBJECT_ASSOCIATE_COMPLETION_PORT_INFORMATION_CLASS,
            &association as *const _ as *const c_void,
            size_of::<JobObjectAssociateCompletionPort>() as Dword,
        )
    } == 0
    {
        return Err(last_error("associate Windows Job Object completion port"));
    }
    Ok(())
}

fn resume_process_threads(process_id: Dword) -> io::Result<()> {
    let snapshot = OwnedHandle(unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) });
    if snapshot.0 == INVALID_HANDLE_VALUE {
        return Err(last_error("enumerate suspended process threads"));
    }
    let mut entry: ThreadEntry32 = unsafe { zeroed() };
    entry.size = size_of::<ThreadEntry32>() as Dword;
    let mut has_entry = unsafe { Thread32First(snapshot.0, &mut entry) } != 0;
    let mut resumed = 0;
    while has_entry {
        if entry.owner_process_id == process_id {
            let thread =
                OwnedHandle(unsafe { OpenThread(THREAD_SUSPEND_RESUME, 0, entry.thread_id) });
            if thread.0.is_null() {
                return Err(last_error("open suspended process thread"));
            }
            if unsafe { ResumeThread(thread.0) } == Dword::MAX {
                return Err(last_error("resume contained process thread"));
            }
            resumed += 1;
        }
        has_entry = unsafe { Thread32Next(snapshot.0, &mut entry) } != 0;
    }
    if resumed == 0 {
        Err(io::Error::new(
            io::ErrorKind::Other,
            "failed to find suspended process thread",
        ))
    } else {
        Ok(())
    }
}

fn last_error(context: &str) -> io::Error {
    let error = io::Error::last_os_error();
    io::Error::new(error.kind(), format!("{context}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::process::Stdio;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn assignment_failure_kills_suspended_root_before_command_code_runs() {
        let marker = std::env::temp_dir().join(format!(
            "sunlight-job-fail-closed-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut command = Command::new("cmd.exe");
        command
            .args([
                "/D",
                "/C",
                &format!("echo escaped>\"{}\"", marker.display()),
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(CREATE_SUSPENDED);
        let mut child = command.spawn().expect("suspended helper should spawn");

        let assigned = unsafe {
            AssignProcessToJobObject(INVALID_HANDLE_VALUE, child.as_raw_handle() as Handle)
        };
        assert_eq!(assigned, 0, "invalid Job Object assignment must fail");
        child.kill().expect("suspended helper should be terminated");
        child.wait().expect("suspended helper should be reaped");
        assert!(
            !PathBuf::from(&marker).exists(),
            "command code escaped before failed containment setup closed"
        );
        let _ = fs::remove_file(marker);
    }
}
