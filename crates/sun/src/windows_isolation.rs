use std::ffi::c_void;
use std::ffi::OsString;
use std::fs;
use std::io::{self, Write};
use std::mem::{size_of, zeroed};
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{error, fmt};

type Handle = *mut c_void;
type Bool = i32;
type Dword = u32;

const TOKEN_ASSIGN_PRIMARY: Dword = 0x0001;
const TOKEN_DUPLICATE: Dword = 0x0002;
const TOKEN_QUERY: Dword = 0x0008;
const TOKEN_ADJUST_DEFAULT: Dword = 0x0080;
const DISABLE_MAX_PRIVILEGE: Dword = 0x0001;
const TOKEN_USER_CLASS: Dword = 1;
const TOKEN_INTEGRITY_LEVEL_CLASS: Dword = 25;
const SE_GROUP_INTEGRITY: Dword = 0x0000_0020;
const SDDL_REVISION_1: Dword = 1;
const DACL_SECURITY_INFORMATION: Dword = 0x0000_0004;
const LABEL_SECURITY_INFORMATION: Dword = 0x0000_0010;
const PROTECTED_DACL_SECURITY_INFORMATION: Dword = 0x8000_0000;
const SE_FILE_OBJECT: Dword = 1;
const ACL_SIZE_INFORMATION_CLASS: Dword = 2;
const SYSTEM_MANDATORY_LABEL_ACE_TYPE: u8 = 0x11;
const SYSTEM_MANDATORY_LABEL_NO_WRITE_UP: Dword = 0x0000_0001;
const SECURITY_MANDATORY_MEDIUM_RID: Dword = 0x0000_2000;
const FILE_ATTRIBUTE_REPARSE_POINT: Dword = 0x0000_0400;
const INVALID_FILE_ATTRIBUTES: Dword = Dword::MAX;
const STARTF_USESTDHANDLES: Dword = 0x0000_0100;
const CREATE_SUSPENDED: Dword = 0x0000_0004;
const EXTENDED_STARTUPINFO_PRESENT: Dword = 0x0008_0000;
const PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES: usize = 0x0002_0009;
const STD_INPUT_HANDLE: Dword = -10_i32 as Dword;
const STD_OUTPUT_HANDLE: Dword = -11_i32 as Dword;
const STD_ERROR_HANDLE: Dword = -12_i32 as Dword;
const INFINITE: Dword = Dword::MAX;
const WAIT_FAILED: Dword = Dword::MAX;
const WAIT_TIMEOUT: Dword = 258;
const STILL_ACTIVE: Dword = 259;
const PROCESS_QUERY_LIMITED_INFORMATION: Dword = 0x1000;
const SYNCHRONIZE: Dword = 0x0010_0000;
const BOOTSTRAP_SETUP_FAILURE_EXIT_CODE: u8 = 125;
const BOOTSTRAP_MARKER_ENV: &str = "SUNLIGHT_INTERNAL_ISOLATION_MARKER";
const APPCONTAINER_SID_ENV: &str = "SUNLIGHT_INTERNAL_APPCONTAINER_SID";
#[cfg(debug_assertions)]
const TEST_FAILPOINT_ENV: &str = "SUNLIGHT_INTERNAL_TEST_WINDOWS_ISOLATION_FAILPOINT";
#[cfg(debug_assertions)]
const TEST_PARENT_EXE_ENV: &str = "SUNLIGHT_INTERNAL_TEST_PARENT_EXE";
#[cfg(debug_assertions)]
const TEST_PARENT_PID_ENV: &str = "SUNLIGHT_INTERNAL_TEST_PARENT_PID";

#[repr(C)]
struct SidAndAttributes {
    sid: *mut c_void,
    attributes: Dword,
}

#[repr(C)]
struct TokenMandatoryLabel {
    label: SidAndAttributes,
}

#[repr(C)]
struct AclSizeInformation {
    ace_count: Dword,
    acl_bytes_in_use: Dword,
    acl_bytes_free: Dword,
}

#[repr(C)]
struct AceHeader {
    ace_type: u8,
    ace_flags: u8,
    ace_size: u16,
}

#[repr(C)]
struct MandatoryLabelAce {
    header: AceHeader,
    mask: Dword,
    sid_start: Dword,
}

#[repr(C)]
struct StartupInfoW {
    cb: Dword,
    reserved: *mut u16,
    desktop: *mut u16,
    title: *mut u16,
    x: Dword,
    y: Dword,
    x_size: Dword,
    y_size: Dword,
    x_count_chars: Dword,
    y_count_chars: Dword,
    fill_attribute: Dword,
    flags: Dword,
    show_window: u16,
    reserved2_size: u16,
    reserved2: *mut u8,
    std_input: Handle,
    std_output: Handle,
    std_error: Handle,
}

#[repr(C)]
struct StartupInfoExW {
    startup_info: StartupInfoW,
    attribute_list: *mut c_void,
}

#[repr(C)]
struct SecurityCapabilities {
    app_container_sid: *mut c_void,
    capabilities: *mut SidAndAttributes,
    capability_count: Dword,
    reserved: Dword,
}

#[repr(C)]
struct ProcessInformation {
    process: Handle,
    thread: Handle,
    process_id: Dword,
    thread_id: Dword,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct FileTime {
    low: Dword,
    high: Dword,
}

#[link(name = "advapi32")]
extern "system" {
    fn OpenProcessToken(process: Handle, desired_access: Dword, token: *mut Handle) -> Bool;
    fn GetTokenInformation(
        token: Handle,
        information_class: Dword,
        information: *mut c_void,
        information_length: Dword,
        return_length: *mut Dword,
    ) -> Bool;
    fn CreateRestrictedToken(
        existing_token: Handle,
        flags: Dword,
        disable_sid_count: Dword,
        sids_to_disable: *const c_void,
        delete_privilege_count: Dword,
        privileges_to_delete: *const c_void,
        restricted_sid_count: Dword,
        sids_to_restrict: *const c_void,
        new_token: *mut Handle,
    ) -> Bool;
    fn SetTokenInformation(
        token: Handle,
        information_class: Dword,
        information: *const c_void,
        information_length: Dword,
    ) -> Bool;
    fn ConvertSidToStringSidW(sid: *mut c_void, string_sid: *mut *mut u16) -> Bool;
    fn ConvertStringSidToSidW(string_sid: *const u16, sid: *mut *mut c_void) -> Bool;
    fn ConvertStringSecurityDescriptorToSecurityDescriptorW(
        descriptor: *const u16,
        revision: Dword,
        security_descriptor: *mut *mut c_void,
        descriptor_size: *mut Dword,
    ) -> Bool;
    fn SetFileSecurityW(
        file_name: *const u16,
        security_information: Dword,
        security_descriptor: *const c_void,
    ) -> Bool;
    fn GetNamedSecurityInfoW(
        object_name: *const u16,
        object_type: Dword,
        security_information: Dword,
        owner: *mut *mut c_void,
        group: *mut *mut c_void,
        dacl: *mut *mut c_void,
        sacl: *mut *mut c_void,
        security_descriptor: *mut *mut c_void,
    ) -> Dword;
    fn GetAclInformation(
        acl: *mut c_void,
        information: *mut c_void,
        information_length: Dword,
        information_class: Dword,
    ) -> Bool;
    fn GetAce(acl: *mut c_void, ace_index: Dword, ace: *mut *mut c_void) -> Bool;
    fn IsValidSid(sid: *mut c_void) -> Bool;
    fn GetSidSubAuthorityCount(sid: *mut c_void) -> *mut u8;
    fn GetSidSubAuthority(sid: *mut c_void, sub_authority: Dword) -> *mut Dword;
    fn CreateProcessAsUserW(
        token: Handle,
        application_name: *const u16,
        command_line: *mut u16,
        process_attributes: *const c_void,
        thread_attributes: *const c_void,
        inherit_handles: Bool,
        creation_flags: Dword,
        environment: *const c_void,
        current_directory: *const u16,
        startup_info: *const StartupInfoW,
        process_information: *mut ProcessInformation,
    ) -> Bool;
}

#[link(name = "userenv")]
extern "system" {
    fn CreateAppContainerProfile(
        app_container_name: *const u16,
        display_name: *const u16,
        description: *const u16,
        capabilities: *const SidAndAttributes,
        capability_count: Dword,
        app_container_sid: *mut *mut c_void,
    ) -> i32;
    fn DeleteAppContainerProfile(app_container_name: *const u16) -> i32;
}

#[link(name = "kernel32")]
extern "system" {
    fn GetCurrentProcess() -> Handle;
    fn OpenProcess(desired_access: Dword, inherit_handle: Bool, process_id: Dword) -> Handle;
    #[cfg(debug_assertions)]
    fn QueryFullProcessImageNameW(
        process: Handle,
        flags: Dword,
        executable_name: *mut u16,
        size: *mut Dword,
    ) -> Bool;
    fn GetFileAttributesW(file_name: *const u16) -> Dword;
    fn GetLengthSid(sid: *mut c_void) -> Dword;
    fn GetStdHandle(std_handle: Dword) -> Handle;
    fn ResumeThread(thread: Handle) -> Dword;
    fn WaitForSingleObject(handle: Handle, milliseconds: Dword) -> Dword;
    fn GetExitCodeProcess(process: Handle, exit_code: *mut Dword) -> Bool;
    fn GetProcessTimes(
        process: Handle,
        creation: *mut FileTime,
        exit: *mut FileTime,
        kernel: *mut FileTime,
        user: *mut FileTime,
    ) -> Bool;
    fn TerminateProcess(process: Handle, exit_code: Dword) -> Bool;
    fn CloseHandle(handle: Handle) -> Bool;
    fn LocalFree(memory: *mut c_void) -> *mut c_void;
    fn FreeSid(sid: *mut c_void) -> *mut c_void;
    fn SearchPathW(
        path: *const u16,
        file_name: *const u16,
        extension: *const u16,
        buffer_length: Dword,
        buffer: *mut u16,
        file_part: *mut *mut u16,
    ) -> Dword;
    fn InitializeProcThreadAttributeList(
        attribute_list: *mut c_void,
        attribute_count: Dword,
        flags: Dword,
        size: *mut usize,
    ) -> Bool;
    fn UpdateProcThreadAttribute(
        attribute_list: *mut c_void,
        flags: Dword,
        attribute: usize,
        value: *mut c_void,
        size: usize,
        previous_value: *mut c_void,
        return_size: *mut usize,
    ) -> Bool;
    fn DeleteProcThreadAttributeList(attribute_list: *mut c_void);
    fn ExitProcess(exit_code: Dword) -> !;
}

struct OwnedHandle(Handle);

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { CloseHandle(self.0) };
        }
    }
}

struct LocalMemory(*mut c_void);

impl Drop for LocalMemory {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { LocalFree(self.0) };
        }
    }
}

struct OwnedSid(*mut c_void);

impl Drop for OwnedSid {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { FreeSid(self.0) };
        }
    }
}

struct AttributeList {
    storage: Vec<usize>,
}

impl AttributeList {
    fn new() -> io::Result<Self> {
        let mut bytes = 0;
        unsafe { InitializeProcThreadAttributeList(std::ptr::null_mut(), 1, 0, &mut bytes) };
        if bytes == 0 {
            return Err(last_error("size AppContainer process attribute list"));
        }
        let words = bytes
            .checked_add(size_of::<usize>() - 1)
            .and_then(|value| value.checked_div(size_of::<usize>()))
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "attribute list overflow"))?;
        let mut storage = vec![0_usize; words];
        if unsafe {
            InitializeProcThreadAttributeList(storage.as_mut_ptr() as *mut c_void, 1, 0, &mut bytes)
        } == 0
        {
            return Err(last_error("initialize AppContainer process attribute list"));
        }
        Ok(Self { storage })
    }

    fn as_mut_ptr(&mut self) -> *mut c_void {
        self.storage.as_mut_ptr() as *mut c_void
    }
}

impl Drop for AttributeList {
    fn drop(&mut self) {
        unsafe { DeleteProcThreadAttributeList(self.as_mut_ptr()) };
    }
}

pub(crate) struct PreparedIsolation {
    runtime_root: PathBuf,
    marker: PathBuf,
    projection_root: PathBuf,
    app_container_name: Option<String>,
    app_container_sid: Option<String>,
    cleanup_journal: Option<PathBuf>,
    active: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum IsolationSetupDimension {
    Filesystem,
    Network,
}

#[derive(Debug)]
pub(crate) struct IsolationSetupError {
    dimension: IsolationSetupDimension,
    source: io::Error,
}

impl IsolationSetupError {
    fn filesystem(source: io::Error) -> Self {
        Self {
            dimension: IsolationSetupDimension::Filesystem,
            source,
        }
    }

    fn network(source: io::Error) -> Self {
        Self {
            dimension: IsolationSetupDimension::Network,
            source,
        }
    }

    pub(crate) fn dimension(&self) -> IsolationSetupDimension {
        self.dimension
    }
}

impl fmt::Display for IsolationSetupError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.source.fmt(formatter)
    }
}

impl error::Error for IsolationSetupError {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        Some(&self.source)
    }
}

pub(crate) enum BootstrapVerification {
    Complete,
    FailedAfterCommandMayHaveStarted,
}

impl PreparedIsolation {
    pub(crate) fn prepare(
        source_root: &Path,
        projection_root: &Path,
        execution_id: &str,
        network_disabled: bool,
    ) -> Result<Self, IsolationSetupError> {
        let parent = projection_root.parent().ok_or_else(|| {
            IsolationSetupError::filesystem(io::Error::new(
                io::ErrorKind::InvalidInput,
                "projection root has no parent",
            ))
        })?;
        let runtime_root = parent.join(format!(".{execution_id}-private"));
        if runtime_root.exists() {
            return Err(IsolationSetupError::filesystem(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "execution private runtime root already exists",
            )));
        }
        let managed_root = projection_root
            .parent()
            .and_then(Path::parent)
            .ok_or_else(|| {
                IsolationSetupError::filesystem(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "projection root is not managed",
                ))
            })?;
        let source_root =
            absolute_lexical_path(source_root).map_err(IsolationSetupError::filesystem)?;
        let managed_root =
            fs::canonicalize(managed_root).map_err(IsolationSetupError::filesystem)?;
        if let Err(error) = fs::create_dir_all(runtime_root.join("temp"))
            .and_then(|()| fs::create_dir_all(runtime_root.join("home/AppData/Local")))
            .and_then(|()| fs::create_dir_all(runtime_root.join("home/AppData/Roaming")))
        {
            let _ = fs::remove_dir_all(&runtime_root);
            return Err(IsolationSetupError::filesystem(error));
        }
        let cleanup_entry = if network_disabled {
            Some(
                CleanupJournalEntry::create(&source_root, projection_root, &runtime_root)
                    .map_err(IsolationSetupError::network)?,
            )
        } else {
            None
        };
        let app_container = cleanup_entry
            .as_ref()
            .map(|entry| {
                inject_test_network_setup_failure()
                    .and_then(|()| create_execution_app_container(&entry.profile_name))
            })
            .transpose()
            .map_err(|error| {
                if let Some(entry) = &cleanup_entry {
                    if cleanup_journal_resources(entry).is_ok() {
                        let _ = fs::remove_file(&entry.journal_path);
                    }
                } else {
                    let _ = fs::remove_dir_all(&runtime_root);
                }
                IsolationSetupError::network(error)
            })?;
        let app_container_name = app_container.as_ref().map(|value| value.0.clone());
        let app_container_sid = app_container.as_ref().map(|value| value.1.clone());
        if let Err(error) = inject_test_setup_failure()
            .and_then(|()| validate_source_tree(&source_root, &managed_root))
            .and_then(|()| secure_private_tree(projection_root, app_container_sid.as_deref()))
            .and_then(|()| secure_private_tree(&runtime_root, app_container_sid.as_deref()))
        {
            if let Some(entry) = &cleanup_entry {
                if cleanup_journal_resources(entry).is_ok() {
                    let _ = fs::remove_file(&entry.journal_path);
                }
            } else {
                let _ = fs::remove_dir_all(&runtime_root);
            }
            return Err(IsolationSetupError::filesystem(error));
        }
        Ok(Self {
            marker: runtime_root.join("low-integrity-ready"),
            runtime_root,
            projection_root: projection_root.to_path_buf(),
            app_container_name,
            app_container_sid,
            cleanup_journal: cleanup_entry.map(|entry| entry.journal_path),
            active: true,
        })
    }

    pub(crate) fn bootstrap_command(&self, argv: &[String]) -> io::Result<Command> {
        let executable = std::env::current_exe()?;
        let mut command = Command::new(executable);
        command
            .arg("__sunlight-low-integrity-bootstrap")
            .arg(&argv[0]);
        command.env(BOOTSTRAP_MARKER_ENV, &self.marker);
        if let Some(sid) = &self.app_container_sid {
            command.env(APPCONTAINER_SID_ENV, sid);
        }
        Ok(command)
    }

    pub(crate) fn validate_command_compatibility(&self, argv: &[String]) -> io::Result<()> {
        if self.app_container_sid.is_none() {
            return Ok(());
        }
        let program = argv.first().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "missing execution program")
        })?;
        let executable = search_executable(program)?;
        let executable = fs::canonicalize(&executable)?;
        let appcontainer_readable_install_roots = [
            "SYSTEMROOT",
            "ProgramFiles",
            "ProgramFiles(x86)",
            "ProgramW6432",
        ]
        .into_iter()
        .filter_map(env_path)
        .filter_map(|path| fs::canonicalize(path).ok())
        .collect::<Vec<_>>();
        let projection_root = fs::canonicalize(&self.projection_root)?;
        let runtime_root = fs::canonicalize(&self.runtime_root)?;
        if appcontainer_readable_install_roots
            .iter()
            .any(|root| path_is_within(&executable, root))
            || path_is_within(&executable, &projection_root)
            || path_is_within(&executable, &runtime_root)
        {
            return Ok(());
        }
        Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!(
                "AppContainer cannot safely expose PATH toolchain executable {}; install the tool in an AppContainer-readable system location or use a different execution backend",
                executable.display()
            ),
        ))
    }

    pub(crate) fn setup_dimension(&self) -> IsolationSetupDimension {
        if self.app_container_sid.is_some() {
            IsolationSetupDimension::Network
        } else {
            IsolationSetupDimension::Filesystem
        }
    }

    pub(crate) fn configure_environment(&self, command: &mut Command) {
        let temp = self.runtime_root.join("temp");
        let home = self.runtime_root.join("home");
        command
            .env(BOOTSTRAP_MARKER_ENV, &self.marker)
            .env("TEMP", &temp)
            .env("TMP", &temp)
            .env("USERPROFILE", &home)
            .env("HOMEDRIVE", "")
            .env("HOMEPATH", &home)
            .env("LOCALAPPDATA", home.join("AppData/Local"))
            .env("APPDATA", home.join("AppData/Roaming"));
        if let Some(sid) = &self.app_container_sid {
            command.env(APPCONTAINER_SID_ENV, sid);
        }
    }

    pub(crate) fn verify_bootstrap(
        &self,
        status: Option<i32>,
    ) -> io::Result<BootstrapVerification> {
        let error_path = self.marker.with_extension("error");
        if error_path.is_file() {
            if self.marker.with_extension("launching").is_file() {
                return Ok(BootstrapVerification::FailedAfterCommandMayHaveStarted);
            }
            let setup_error = fs::read_to_string(&error_path)
                .unwrap_or_else(|_| "bootstrap setup evidence unavailable".to_string());
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!(
                    "low-integrity bootstrap did not complete before command launch (exit code {})",
                    status.map_or_else(|| "unavailable".to_string(), |code| code.to_string())
                ),
            ))
            .map_err(|error| io::Error::new(error.kind(), format!("{error}: {setup_error}")));
        }
        if status == Some(BOOTSTRAP_SETUP_FAILURE_EXIT_CODE.into())
            && self.marker.with_extension("launching").is_file()
            && !self.marker.with_extension("complete").is_file()
        {
            return Ok(BootstrapVerification::FailedAfterCommandMayHaveStarted);
        }
        if !self.marker.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!(
                    "low-integrity bootstrap produced no setup evidence (exit code {})",
                    status.map_or_else(|| "unavailable".to_string(), |code| code.to_string())
                ),
            ));
        }
        Ok(BootstrapVerification::Complete)
    }

    pub(crate) fn finish(&mut self) -> io::Result<()> {
        if !self.active {
            return Ok(());
        }
        inject_test_cleanup_failure()?;
        remove_tree_if_present(&self.runtime_root)?;
        secure_tree_if_present(&self.projection_root, None)?;
        if let Some(name) = self.app_container_name.as_deref() {
            delete_execution_app_container(name)?;
        }
        if let Some(journal) = self.cleanup_journal.as_deref() {
            fs::remove_file(journal)?;
        }
        self.active = false;
        Ok(())
    }
}

struct CleanupJournalEntry {
    journal_path: PathBuf,
    profile_name: String,
    owner_pid: Dword,
    owner_created: u64,
    projection_root: PathBuf,
    runtime_root: PathBuf,
}

impl CleanupJournalEntry {
    fn create(repo_root: &Path, projection_root: &Path, runtime_root: &Path) -> io::Result<Self> {
        let profile_name = execution_app_container_name();
        let journal_root = cleanup_journal_root(repo_root);
        fs::create_dir_all(&journal_root)?;
        let entry = Self {
            journal_path: journal_root.join(format!("{profile_name}.cleanup")),
            profile_name,
            owner_pid: std::process::id(),
            owner_created: current_process_creation_time()?,
            projection_root: absolute_lexical_path(projection_root)?,
            runtime_root: absolute_lexical_path(runtime_root)?,
        };
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&entry.journal_path)?;
        file.write_all(entry.serialize().as_bytes())?;
        file.sync_all()?;
        Ok(entry)
    }

    fn serialize(&self) -> String {
        format!(
            "version=1\nprofile={}\nowner_pid={}\nowner_created={}\nprojection={}\nruntime={}\n",
            self.profile_name,
            self.owner_pid,
            self.owner_created,
            encode_path(&self.projection_root),
            encode_path(&self.runtime_root),
        )
    }

    fn load(path: PathBuf) -> io::Result<Self> {
        let body = fs::read_to_string(&path)?;
        let field = |name: &str| {
            body.lines()
                .find_map(|line| line.strip_prefix(&format!("{name}=")))
                .ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("cleanup journal is missing {name}"),
                    )
                })
        };
        if field("version")? != "1" {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "unsupported cleanup journal version",
            ));
        }
        let profile_name = field("profile")?.to_string();
        if !is_sunlight_profile_name(&profile_name)
            || path.file_stem().and_then(|value| value.to_str()) != Some(profile_name.as_str())
        {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "cleanup journal does not name a Sunlight profile",
            ));
        }
        Ok(Self {
            journal_path: path,
            profile_name,
            owner_pid: field("owner_pid")?.parse().map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidData, "invalid cleanup owner PID")
            })?,
            owner_created: field("owner_created")?.parse().map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidData, "invalid cleanup owner time")
            })?,
            projection_root: decode_path(field("projection")?)?,
            runtime_root: decode_path(field("runtime")?)?,
        })
    }
}

fn cleanup_journal_root(repo_root: &Path) -> PathBuf {
    repo_root
        .join(".sunlight")
        .join("local")
        .join("windows-appcontainer-cleanup")
}

pub(crate) fn recover_stale_app_containers(
    repo_root: &Path,
    managed_execution_boundary: &Path,
) -> io::Result<()> {
    let repo_root = absolute_lexical_path(repo_root)?;
    let repo_boundary = fs::canonicalize(&repo_root)?;
    let managed_execution_boundary = fs::canonicalize(managed_execution_boundary)?;
    let root = cleanup_journal_root(&repo_root);
    validate_existing_ancestor(&root, &repo_boundary, "cleanup journal root")?;
    let entries = match fs::read_dir(&root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    for entry in entries {
        let path = entry?.path();
        if !path.is_file() {
            continue;
        }
        if file_attributes(&path)? & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "cleanup journal must be a repository-local regular file",
            ));
        }
        let journal = CleanupJournalEntry::load(path.clone())?;
        validate_cleanup_allocation(&journal, &managed_execution_boundary)?;
        if cleanup_owner_is_live(journal.owner_pid, journal.owner_created) {
            continue;
        }
        let claim = path.with_extension(format!(
            "recovering-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or(0)
        ));
        if fs::rename(&path, &claim).is_err() {
            continue;
        }
        let mut journal = CleanupJournalEntry::load(claim.clone())?;
        journal.journal_path = claim;
        validate_cleanup_allocation(&journal, &managed_execution_boundary)?;
        recover_journal_resources(&journal, &managed_execution_boundary)?;
        fs::remove_file(&journal.journal_path)?;
    }
    Ok(())
}

fn validate_cleanup_allocation(entry: &CleanupJournalEntry, boundary: &Path) -> io::Result<()> {
    let comparable_boundary = comparable_windows_path(boundary);
    let comparable_projection = comparable_windows_path(&entry.projection_root);
    let comparable_runtime = comparable_windows_path(&entry.runtime_root);
    let allocation = entry.projection_root.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::PermissionDenied,
            "cleanup projection has no allocation",
        )
    })?;
    let comparable_allocation = comparable_windows_path(allocation);
    let canonical_allocation = comparable_windows_path(&fs::canonicalize(allocation)?);
    let allocation_name = comparable_allocation
        .file_name()
        .and_then(|value| value.to_str());
    let runtime_name = comparable_runtime
        .file_name()
        .and_then(|value| value.to_str());
    if comparable_projection
        .file_name()
        .and_then(|value| value.to_str())
        != Some("root")
        || canonical_allocation.parent() != Some(comparable_boundary.as_path())
        || !allocation_name.is_some_and(|value| value.starts_with("projection_execution_"))
        || comparable_runtime.parent() != Some(comparable_allocation.as_path())
        || !runtime_name
            .is_some_and(|value| value.starts_with(".exec_") && value.ends_with("-private"))
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "cleanup journal paths do not name a managed Sunlight execution allocation",
        ));
    }
    validate_existing_ancestor(&entry.projection_root, boundary, "cleanup projection")?;
    validate_existing_ancestor(&entry.runtime_root, boundary, "cleanup runtime")
}

fn validate_existing_ancestor(path: &Path, boundary: &Path, label: &str) -> io::Result<()> {
    let ancestor = path
        .ancestors()
        .find(|ancestor| ancestor.exists())
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("{label} has no existing ancestor"),
            )
        })?;
    let comparable_boundary = comparable_windows_path(boundary);
    let canonical = comparable_windows_path(&fs::canonicalize(ancestor)?);
    if canonical != comparable_boundary && !canonical.starts_with(&comparable_boundary) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("{label} escapes the configured managed execution boundary"),
        ));
    }
    let mut current = ancestor.to_path_buf();
    loop {
        if comparable_windows_path(&fs::canonicalize(&current)?) == comparable_boundary {
            break;
        }
        if file_attributes(&current)? & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!("{label} contains a reparse point: {}", current.display()),
            ));
        }
        current = current.parent().map(Path::to_path_buf).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!("{label} has no canonical path to its configured boundary"),
            )
        })?;
    }
    Ok(())
}

fn comparable_windows_path(path: &Path) -> PathBuf {
    let text = path.as_os_str().to_string_lossy();
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        PathBuf::from(format!(r"\\{rest}"))
    } else if let Some(rest) = text.strip_prefix(r"\\?\") {
        PathBuf::from(rest)
    } else {
        path.to_path_buf()
    }
}

fn recover_journal_resources(entry: &CleanupJournalEntry, boundary: &Path) -> io::Result<()> {
    validate_existing_ancestor(&entry.runtime_root, boundary, "cleanup runtime")?;
    remove_tree_if_present(&entry.runtime_root)?;
    validate_existing_ancestor(&entry.projection_root, boundary, "cleanup projection")?;
    secure_tree_if_present(&entry.projection_root, None)?;
    delete_execution_app_container(&entry.profile_name)
}

fn cleanup_journal_resources(entry: &CleanupJournalEntry) -> io::Result<()> {
    remove_tree_if_present(&entry.runtime_root)?;
    secure_tree_if_present(&entry.projection_root, None)?;
    delete_execution_app_container(&entry.profile_name)
}

fn is_sunlight_profile_name(name: &str) -> bool {
    let mut parts = name.split('-');
    parts.next() == Some("Sunlight")
        && parts.next().is_some_and(|part| {
            !part.is_empty() && part.chars().all(|value| value.is_ascii_hexdigit())
        })
        && parts.next().is_some_and(|part| {
            !part.is_empty() && part.chars().all(|value| value.is_ascii_hexdigit())
        })
        && parts.next().is_none()
}

fn remove_tree_if_present(path: &Path) -> io::Result<()> {
    match fs::remove_dir_all(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        result => result,
    }
}

fn secure_tree_if_present(path: &Path, app_container_sid: Option<&str>) -> io::Result<()> {
    if path.exists() {
        secure_private_tree(path, app_container_sid)
    } else {
        Ok(())
    }
}

fn current_process_creation_time() -> io::Result<u64> {
    process_creation_time(unsafe { GetCurrentProcess() })
}

fn process_creation_time(process: Handle) -> io::Result<u64> {
    let mut creation = FileTime { low: 0, high: 0 };
    let mut exit = creation;
    let mut kernel = creation;
    let mut user = creation;
    if unsafe { GetProcessTimes(process, &mut creation, &mut exit, &mut kernel, &mut user) } == 0 {
        return Err(last_error("read process creation time"));
    }
    Ok((u64::from(creation.high) << 32) | u64::from(creation.low))
}

fn cleanup_owner_is_live(pid: Dword, created: u64) -> bool {
    let process = OwnedHandle(unsafe {
        OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | SYNCHRONIZE, 0, pid)
    });
    if process.0.is_null() {
        return false;
    }
    process_creation_time(process.0).ok() == Some(created)
        && unsafe { WaitForSingleObject(process.0, 0) } == WAIT_TIMEOUT
}

fn encode_path(path: &Path) -> String {
    path.as_os_str()
        .encode_wide()
        .map(|unit| format!("{unit:04x}"))
        .collect()
}

fn decode_path(value: &str) -> io::Result<PathBuf> {
    if value.len() % 4 != 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid cleanup journal path",
        ));
    }
    let units = value
        .as_bytes()
        .chunks(4)
        .map(|chunk| {
            let text = std::str::from_utf8(chunk).map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidData, "invalid cleanup journal path")
            })?;
            u16::from_str_radix(text, 16).map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidData, "invalid cleanup journal path")
            })
        })
        .collect::<io::Result<Vec<_>>>()?;
    Ok(PathBuf::from(OsString::from_wide(&units)))
}

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name).map(PathBuf::from)
}

fn path_is_within(path: &Path, root: &Path) -> bool {
    path == root || path.starts_with(root)
}

pub(crate) fn search_executable(program: &str) -> io::Result<PathBuf> {
    let append_exe = Path::new(program).extension().is_none();
    let program = wide_null(program);
    let extension = wide_null(".exe");
    let extension = if append_exe {
        extension.as_ptr()
    } else {
        std::ptr::null()
    };
    let mut buffer = vec![0_u16; 32_768];
    let length = unsafe {
        SearchPathW(
            std::ptr::null(),
            program.as_ptr(),
            extension,
            buffer.len() as Dword,
            buffer.as_mut_ptr(),
            std::ptr::null_mut(),
        )
    };
    if length == 0 {
        return Err(last_error(
            "resolve execution program for AppContainer compatibility",
        ));
    }
    if length as usize >= buffer.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "resolved execution program path is too long",
        ));
    }
    Ok(PathBuf::from(OsString::from_wide(
        &buffer[..length as usize],
    )))
}

impl Drop for PreparedIsolation {
    fn drop(&mut self) {
        if self.active {
            let _ = self.finish();
        }
    }
}

pub(crate) fn bootstrap(argv: &[String]) -> ! {
    let marker = std::env::var_os(BOOTSTRAP_MARKER_ENV).map(PathBuf::from);
    match marker
        .as_deref()
        .ok_or_else(|| io::Error::new(io::ErrorKind::PermissionDenied, "missing isolation marker"))
        .and_then(|marker| bootstrap_inner(argv, marker))
    {
        Ok(code) => unsafe { ExitProcess(code) },
        Err(error) => {
            if let Some(marker) = marker {
                let _ = fs::write(marker.with_extension("error"), error.to_string());
            }
            eprintln!("sun: Windows filesystem isolation bootstrap failed: {error}");
            unsafe { ExitProcess(BOOTSTRAP_SETUP_FAILURE_EXIT_CODE as Dword) }
        }
    }
}

fn bootstrap_inner(argv: &[String], marker: &Path) -> io::Result<Dword> {
    if argv.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "missing command",
        ));
    }
    std::env::remove_var(BOOTSTRAP_MARKER_ENV);
    let app_container_sid = std::env::var(APPCONTAINER_SID_ENV).ok();
    std::env::remove_var(APPCONTAINER_SID_ENV);
    let token = restricted_low_integrity_token()?;
    let mut app_container_sid_memory = app_container_sid
        .as_deref()
        .map(sid_from_string)
        .transpose()?;
    let mut security_capabilities = SecurityCapabilities {
        app_container_sid: app_container_sid_memory
            .as_mut()
            .map_or(std::ptr::null_mut(), |sid| sid.0),
        capabilities: std::ptr::null_mut(),
        capability_count: 0,
        reserved: 0,
    };
    let mut attributes = if app_container_sid_memory.is_some() {
        Some(AttributeList::new()?)
    } else {
        None
    };
    if let Some(attributes) = attributes.as_mut() {
        if unsafe {
            UpdateProcThreadAttribute(
                attributes.as_mut_ptr(),
                0,
                PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES,
                &mut security_capabilities as *mut _ as *mut c_void,
                size_of::<SecurityCapabilities>(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        } == 0
        {
            return Err(last_error(
                "set capability-less AppContainer process attribute",
            ));
        }
    }
    let mut command_line = windows_command_line(argv);
    let mut startup: StartupInfoExW = unsafe { zeroed() };
    startup.startup_info.cb = if attributes.is_some() {
        size_of::<StartupInfoExW>() as Dword
    } else {
        size_of::<StartupInfoW>() as Dword
    };
    startup.startup_info.flags = STARTF_USESTDHANDLES;
    startup.startup_info.std_input = unsafe { GetStdHandle(STD_INPUT_HANDLE) };
    startup.startup_info.std_output = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) };
    startup.startup_info.std_error = unsafe { GetStdHandle(STD_ERROR_HANDLE) };
    startup.attribute_list = attributes
        .as_mut()
        .map_or(std::ptr::null_mut(), AttributeList::as_mut_ptr);
    let mut process: ProcessInformation = unsafe { zeroed() };
    if unsafe {
        CreateProcessAsUserW(
            token.0,
            std::ptr::null(),
            command_line.as_mut_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            1,
            CREATE_SUSPENDED
                | if attributes.is_some() {
                    EXTENDED_STARTUPINFO_PRESENT
                } else {
                    0
                },
            std::ptr::null(),
            std::ptr::null(),
            &startup.startup_info,
            &mut process,
        )
    } == 0
    {
        return Err(last_error(if attributes.is_some() {
            "create capability-less AppContainer low-integrity command"
        } else {
            "create restricted low-integrity command"
        }));
    }
    let process_handle = OwnedHandle(process.process);
    let thread_handle = OwnedHandle(process.thread);
    let marker_evidence = if attributes.is_some() {
        b"windows_appcontainer_no_network_capabilities_v1\n".as_slice()
    } else {
        b"windows_low_integrity_private_projection_v1\n".as_slice()
    };
    if let Err(error) = fs::write(marker, marker_evidence) {
        terminate_suspended_process(process_handle.0);
        return Err(error);
    }
    if let Err(error) = fs::write(
        marker.with_extension("launching"),
        b"restricted command launch attempted\n",
    ) {
        terminate_suspended_process(process_handle.0);
        return Err(error);
    }
    if unsafe { ResumeThread(thread_handle.0) } == Dword::MAX {
        let error = last_error("resume restricted low-integrity command");
        terminate_suspended_process(process_handle.0);
        return Err(error);
    }
    if unsafe { WaitForSingleObject(process_handle.0, INFINITE) } == WAIT_FAILED {
        return Err(last_error("wait for restricted low-integrity command"));
    }
    let mut exit_code = STILL_ACTIVE;
    if unsafe { GetExitCodeProcess(process_handle.0, &mut exit_code) } == 0 {
        return Err(last_error("read restricted command exit code"));
    }
    fs::write(
        marker.with_extension("complete"),
        format!("restricted command completed with exit code {exit_code}\n"),
    )?;
    Ok(exit_code)
}

fn terminate_suspended_process(process: Handle) {
    unsafe {
        TerminateProcess(process, BOOTSTRAP_SETUP_FAILURE_EXIT_CODE as Dword);
        WaitForSingleObject(process, INFINITE);
    }
}

fn restricted_low_integrity_token() -> io::Result<OwnedHandle> {
    let mut process_token = std::ptr::null_mut();
    if unsafe {
        OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_ASSIGN_PRIMARY | TOKEN_DUPLICATE | TOKEN_QUERY | TOKEN_ADJUST_DEFAULT,
            &mut process_token,
        )
    } == 0
    {
        return Err(last_error("open bootstrap process token"));
    }
    let process_token = OwnedHandle(process_token);
    let mut restricted = std::ptr::null_mut();
    if unsafe {
        CreateRestrictedToken(
            process_token.0,
            DISABLE_MAX_PRIVILEGE,
            0,
            std::ptr::null(),
            0,
            std::ptr::null(),
            0,
            std::ptr::null(),
            &mut restricted,
        )
    } == 0
    {
        return Err(last_error("create restricted execution token"));
    }
    let restricted = OwnedHandle(restricted);
    let low_sid_text = wide_null("S-1-16-4096");
    let mut low_sid = std::ptr::null_mut();
    if unsafe { ConvertStringSidToSidW(low_sid_text.as_ptr(), &mut low_sid) } == 0 {
        return Err(last_error("create low-integrity SID"));
    }
    let low_sid = LocalMemory(low_sid);
    let label = TokenMandatoryLabel {
        label: SidAndAttributes {
            sid: low_sid.0,
            attributes: SE_GROUP_INTEGRITY,
        },
    };
    let length = (size_of::<TokenMandatoryLabel>() as Dword)
        .checked_add(unsafe { GetLengthSid(low_sid.0) })
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "token label overflow"))?;
    if unsafe {
        SetTokenInformation(
            restricted.0,
            TOKEN_INTEGRITY_LEVEL_CLASS,
            &label as *const _ as *const c_void,
            length,
        )
    } == 0
    {
        return Err(last_error("set restricted token integrity level"));
    }
    Ok(restricted)
}

fn secure_private_tree(root: &Path, app_container_sid: Option<&str>) -> io::Result<()> {
    let descriptor = private_security_descriptor(app_container_sid)?;
    let mut paths = Vec::new();
    collect_paths_without_reparse_points(root, &mut paths)?;
    for path in paths {
        let wide = wide_path(&path);
        if unsafe {
            SetFileSecurityW(
                wide.as_ptr(),
                DACL_SECURITY_INFORMATION
                    | PROTECTED_DACL_SECURITY_INFORMATION
                    | LABEL_SECURITY_INFORMATION,
                descriptor.0,
            )
        } == 0
        {
            return Err(last_error(&format!(
                "apply private low-integrity security descriptor to {}",
                path.display()
            )));
        }
    }
    Ok(())
}

fn validate_source_tree(root: &Path, managed_root: &Path) -> io::Result<()> {
    let attributes = file_attributes(root)?;
    if attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "source repository contains unsupported reparse point: {}",
                root.display()
            ),
        ));
    }
    if root.is_dir() && fs::canonicalize(root)? == managed_root {
        return Ok(());
    }
    validate_source_integrity(root)?;
    if root.is_dir() {
        for entry in fs::read_dir(root)? {
            validate_source_tree(&entry?.path(), managed_root)?;
        }
    }
    Ok(())
}

fn validate_source_integrity(path: &Path) -> io::Result<()> {
    let wide = wide_path(path);
    let mut sacl = std::ptr::null_mut();
    let mut descriptor = std::ptr::null_mut();
    let result = unsafe {
        GetNamedSecurityInfoW(
            wide.as_ptr(),
            SE_FILE_OBJECT,
            LABEL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut sacl,
            &mut descriptor,
        )
    };
    if result != 0 {
        return Err(io::Error::new(
            io::Error::from_raw_os_error(result as i32).kind(),
            format!(
                "read source integrity label for {}: {}",
                path.display(),
                io::Error::from_raw_os_error(result as i32)
            ),
        ));
    }
    let _descriptor = LocalMemory(descriptor);
    if sacl.is_null() {
        return Ok(());
    }
    let mut information: AclSizeInformation = unsafe { zeroed() };
    if unsafe {
        GetAclInformation(
            sacl,
            &mut information as *mut _ as *mut c_void,
            size_of::<AclSizeInformation>() as Dword,
            ACL_SIZE_INFORMATION_CLASS,
        )
    } == 0
    {
        return Err(last_error(&format!(
            "inspect source integrity ACL for {}",
            path.display()
        )));
    }
    for index in 0..information.ace_count {
        let mut ace = std::ptr::null_mut();
        if unsafe { GetAce(sacl, index, &mut ace) } == 0 {
            return Err(last_error(&format!(
                "read source integrity ACE for {}",
                path.display()
            )));
        }
        let ace = unsafe { &*(ace as *const MandatoryLabelAce) };
        if ace.header.ace_type != SYSTEM_MANDATORY_LABEL_ACE_TYPE {
            continue;
        }
        if usize::from(ace.header.ace_size) < size_of::<MandatoryLabelAce>() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("source integrity ACE is truncated: {}", path.display()),
            ));
        }
        let sid = &ace.sid_start as *const Dword as *mut c_void;
        if unsafe { IsValidSid(sid) } == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "source integrity label has an invalid SID: {}",
                    path.display()
                ),
            ));
        }
        let count = unsafe { *GetSidSubAuthorityCount(sid) } as Dword;
        if count == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("source integrity label has no RID: {}", path.display()),
            ));
        }
        let rid = unsafe { *GetSidSubAuthority(sid, count - 1) };
        if rid < SECURITY_MANDATORY_MEDIUM_RID || ace.mask & SYSTEM_MANDATORY_LABEL_NO_WRITE_UP == 0
        {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!(
                    "source repository object is not protected from low-integrity writes: {}",
                    path.display()
                ),
            ));
        }
    }
    Ok(())
}

fn collect_paths_without_reparse_points(root: &Path, paths: &mut Vec<PathBuf>) -> io::Result<()> {
    let attributes = file_attributes(root)?;
    if attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "execution projection contains unsupported reparse point: {}",
                root.display()
            ),
        ));
    }
    paths.push(root.to_path_buf());
    if root.is_dir() {
        for entry in fs::read_dir(root)? {
            collect_paths_without_reparse_points(&entry?.path(), paths)?;
        }
    }
    Ok(())
}

fn file_attributes(path: &Path) -> io::Result<Dword> {
    let wide = wide_path(path);
    let attributes = unsafe { GetFileAttributesW(wide.as_ptr()) };
    if attributes == INVALID_FILE_ATTRIBUTES {
        return Err(last_error(&format!("inspect {}", path.display())));
    }
    Ok(attributes)
}

fn absolute_lexical_path(path: &Path) -> io::Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let mut normalized = PathBuf::new();
    for component in absolute.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            component => normalized.push(component.as_os_str()),
        }
    }
    Ok(normalized)
}

fn inject_test_setup_failure() -> io::Result<()> {
    if test_setup_failure_requested() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "injected Windows isolation setup failure",
        ));
    }
    Ok(())
}

fn inject_test_network_setup_failure() -> io::Result<()> {
    if test_failure_requested("prepare_appcontainer") {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "injected Windows network isolation setup failure",
        ));
    }
    Ok(())
}

fn inject_test_cleanup_failure() -> io::Result<()> {
    if test_cleanup_failure_requested() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "injected Windows isolation cleanup failure",
        ));
    }
    Ok(())
}

#[cfg(debug_assertions)]
fn test_setup_failure_requested() -> bool {
    test_failure_requested("prepare_after_runtime_root")
}

#[cfg(debug_assertions)]
pub(crate) fn test_failure_requested(expected_failpoint: &str) -> bool {
    if std::env::var(TEST_FAILPOINT_ENV).as_deref() != Ok(expected_failpoint) {
        return false;
    }
    let Some(expected_pid) = std::env::var(TEST_PARENT_PID_ENV)
        .ok()
        .and_then(|value| value.parse::<Dword>().ok())
    else {
        return false;
    };
    let Some((parent_pid, parent_exe)) = parent_process() else {
        return false;
    };
    parent_pid == expected_pid
        && std::env::var_os(TEST_PARENT_EXE_ENV).is_some_and(|expected| {
            Path::new(&expected)
                .to_string_lossy()
                .eq_ignore_ascii_case(&parent_exe.to_string_lossy())
        })
        && parent_exe
            .file_stem()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("cli_json-"))
}

#[cfg(debug_assertions)]
fn test_cleanup_failure_requested() -> bool {
    if std::env::var(TEST_FAILPOINT_ENV).as_deref() != Ok("cleanup_after_command") {
        return false;
    }
    let Some(expected_pid) = std::env::var(TEST_PARENT_PID_ENV)
        .ok()
        .and_then(|value| value.parse::<Dword>().ok())
    else {
        return false;
    };
    let Some((parent_pid, parent_exe)) = parent_process() else {
        return false;
    };
    parent_pid == expected_pid
        && std::env::var_os(TEST_PARENT_EXE_ENV).is_some_and(|expected| {
            Path::new(&expected)
                .to_string_lossy()
                .eq_ignore_ascii_case(&parent_exe.to_string_lossy())
        })
        && parent_exe
            .file_stem()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("cli_json-"))
}

#[cfg(not(debug_assertions))]
fn test_setup_failure_requested() -> bool {
    false
}

#[cfg(not(debug_assertions))]
pub(crate) fn test_failure_requested(_expected_failpoint: &str) -> bool {
    false
}

#[cfg(not(debug_assertions))]
fn test_cleanup_failure_requested() -> bool {
    false
}

#[cfg(debug_assertions)]
fn parent_process() -> Option<(Dword, PathBuf)> {
    const PROCESS_BASIC_INFORMATION_CLASS: Dword = 0;
    #[repr(C)]
    struct ProcessBasicInformation {
        reserved1: *mut c_void,
        peb_base_address: *mut c_void,
        reserved2: [*mut c_void; 2],
        unique_process_id: usize,
        inherited_from_unique_process_id: usize,
    }
    #[link(name = "ntdll")]
    extern "system" {
        fn NtQueryInformationProcess(
            process: Handle,
            information_class: Dword,
            information: *mut c_void,
            information_length: Dword,
            return_length: *mut Dword,
        ) -> i32;
    }
    let mut information: ProcessBasicInformation = unsafe { zeroed() };
    if unsafe {
        NtQueryInformationProcess(
            GetCurrentProcess(),
            PROCESS_BASIC_INFORMATION_CLASS,
            &mut information as *mut _ as *mut c_void,
            size_of::<ProcessBasicInformation>() as Dword,
            std::ptr::null_mut(),
        )
    } < 0
    {
        return None;
    }
    let parent_pid = Dword::try_from(information.inherited_from_unique_process_id).ok()?;
    let process =
        OwnedHandle(unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, parent_pid) });
    if process.0.is_null() {
        return None;
    }
    let mut buffer = vec![0_u16; 32_768];
    let mut length = buffer.len() as Dword;
    if unsafe { QueryFullProcessImageNameW(process.0, 0, buffer.as_mut_ptr(), &mut length) } == 0 {
        return None;
    }
    Some((
        parent_pid,
        PathBuf::from(String::from_utf16(&buffer[..length as usize]).ok()?),
    ))
}

fn execution_app_container_name() -> String {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    format!("Sunlight-{:x}-{nonce:x}", std::process::id())
}

fn create_execution_app_container(name: &str) -> io::Result<(String, String)> {
    let wide_name = wide_null(&name);
    let display_name = wide_null("Sunlight isolated execution");
    let description = wide_null("Ephemeral capability-less Sunlight execution boundary");
    let mut sid = std::ptr::null_mut();
    let result = unsafe {
        CreateAppContainerProfile(
            wide_name.as_ptr(),
            display_name.as_ptr(),
            description.as_ptr(),
            std::ptr::null(),
            0,
            &mut sid,
        )
    };
    if result < 0 {
        return Err(hresult_error(
            result,
            "create per-execution AppContainer profile",
        ));
    }
    let sid = OwnedSid(sid);
    let sid_string = sid_to_string(sid.0).inspect_err(|_| {
        let _ = delete_execution_app_container(name);
    })?;
    Ok((name.to_string(), sid_string))
}

fn delete_execution_app_container(name: &str) -> io::Result<()> {
    let wide_name = wide_null(name);
    let result = unsafe { DeleteAppContainerProfile(wide_name.as_ptr()) };
    let code = result & 0xffff;
    if result < 0 && ![2, 1168].contains(&code) {
        Err(hresult_error(
            result,
            "delete per-execution AppContainer profile",
        ))
    } else {
        Ok(())
    }
}

fn hresult_error(result: i32, context: &str) -> io::Error {
    let code = result & 0xffff;
    io::Error::new(
        io::Error::from_raw_os_error(code).kind(),
        format!(
            "{context}: {} (HRESULT 0x{:08x})",
            io::Error::from_raw_os_error(code),
            result as u32
        ),
    )
}

fn sid_from_string(value: &str) -> io::Result<LocalMemory> {
    let value = wide_null(value);
    let mut sid = std::ptr::null_mut();
    if unsafe { ConvertStringSidToSidW(value.as_ptr(), &mut sid) } == 0 {
        return Err(last_error("parse AppContainer SID"));
    }
    Ok(LocalMemory(sid))
}

fn sid_to_string(sid: *mut c_void) -> io::Result<String> {
    let mut value = std::ptr::null_mut();
    if unsafe { ConvertSidToStringSidW(sid, &mut value) } == 0 {
        return Err(last_error("format AppContainer SID"));
    }
    let value = LocalMemory(value as *mut c_void);
    let mut length = 0;
    unsafe {
        while *(value.0 as *const u16).add(length) != 0 {
            length += 1;
        }
        String::from_utf16(std::slice::from_raw_parts(value.0 as *const u16, length)).map_err(
            |_| io::Error::new(io::ErrorKind::InvalidData, "AppContainer SID is not UTF-16"),
        )
    }
}

fn private_security_descriptor(app_container_sid: Option<&str>) -> io::Result<LocalMemory> {
    let sid = current_user_sid_string()?;
    let app_container_ace = app_container_sid
        .map(|sid| format!("(A;OICI;FA;;;{sid})"))
        .unwrap_or_default();
    let sddl = wide_null(&format!(
        "D:P(A;OICI;FA;;;{sid})(A;OICI;FA;;;SY)(A;OICI;FA;;;BA){app_container_ace}S:(ML;OICI;NW;;;LW)"
    ));
    let mut descriptor = std::ptr::null_mut();
    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            SDDL_REVISION_1,
            &mut descriptor,
            std::ptr::null_mut(),
        )
    } == 0
    {
        return Err(last_error(
            "build private low-integrity security descriptor",
        ));
    }
    Ok(LocalMemory(descriptor))
}

fn current_user_sid_string() -> io::Result<String> {
    let mut token = std::ptr::null_mut();
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        return Err(last_error("open current process token"));
    }
    let token = OwnedHandle(token);
    let mut needed = 0;
    unsafe {
        GetTokenInformation(
            token.0,
            TOKEN_USER_CLASS,
            std::ptr::null_mut(),
            0,
            &mut needed,
        )
    };
    if needed < size_of::<SidAndAttributes>() as Dword {
        return Err(last_error("size current user token information"));
    }
    let mut buffer = vec![0_u8; needed as usize];
    if unsafe {
        GetTokenInformation(
            token.0,
            TOKEN_USER_CLASS,
            buffer.as_mut_ptr() as *mut c_void,
            needed,
            &mut needed,
        )
    } == 0
    {
        return Err(last_error("read current user token information"));
    }
    let user = unsafe { &*(buffer.as_ptr() as *const SidAndAttributes) };
    let mut string_sid = std::ptr::null_mut();
    if unsafe { ConvertSidToStringSidW(user.sid, &mut string_sid) } == 0 {
        return Err(last_error("format current user SID"));
    }
    let string_sid_memory = LocalMemory(string_sid as *mut c_void);
    let mut length = 0;
    while unsafe { *string_sid.add(length) } != 0 {
        length += 1;
    }
    let value = String::from_utf16(unsafe { std::slice::from_raw_parts(string_sid, length) })
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid current user SID"))?;
    drop(string_sid_memory);
    Ok(value)
}

fn windows_command_line(argv: &[String]) -> Vec<u16> {
    let mut command_line = argv
        .iter()
        .map(|argument| quote_windows_argument(argument))
        .collect::<Vec<_>>()
        .join(" ")
        .encode_utf16()
        .collect::<Vec<_>>();
    command_line.push(0);
    command_line
}

fn quote_windows_argument(argument: &str) -> String {
    if !argument.is_empty()
        && !argument
            .chars()
            .any(|character| character.is_whitespace() || character == '"')
    {
        return argument.to_string();
    }
    let mut quoted = String::from("\"");
    let mut backslashes = 0;
    for character in argument.chars() {
        if character == '\\' {
            backslashes += 1;
        } else if character == '"' {
            quoted.push_str(&"\\".repeat(backslashes * 2 + 1));
            quoted.push('"');
            backslashes = 0;
        } else {
            quoted.push_str(&"\\".repeat(backslashes));
            backslashes = 0;
            quoted.push(character);
        }
    }
    quoted.push_str(&"\\".repeat(backslashes * 2));
    quoted.push('"');
    quoted
}

fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

fn wide_path(path: &Path) -> Vec<u16> {
    path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

fn last_error(context: &str) -> io::Error {
    let error = io::Error::last_os_error();
    io::Error::new(error.kind(), format!("{context}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn executable_search_applies_the_exe_extension() {
        let executable = search_executable("cmd").unwrap();
        assert_eq!(
            executable
                .file_name()
                .and_then(|name| name.to_str())
                .map(str::to_ascii_lowercase)
                .as_deref(),
            Some("cmd.exe")
        );
    }

    #[test]
    fn windows_argument_quoting_preserves_spaces_quotes_and_trailing_slashes() {
        assert_eq!(quote_windows_argument("plain"), "plain");
        assert_eq!(quote_windows_argument("two words"), "\"two words\"");
        assert_eq!(quote_windows_argument(""), "\"\"");
        assert_eq!(quote_windows_argument("a\\\"b"), "\"a\\\\\\\"b\"");
        assert_eq!(
            quote_windows_argument("path with slash\\"),
            "\"path with slash\\\\\""
        );
    }

    #[test]
    fn reparse_point_setup_failure_is_fail_closed_and_rolls_back_runtime_root() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base = std::env::temp_dir().join(format!(
            "sunlight-isolation-setup-failure-{}-{unique}",
            std::process::id()
        ));
        let source = base.join("source");
        let projection = base.join("managed/projection");
        let junction_target = base.join("junction-target");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(&projection).unwrap();
        fs::create_dir_all(&junction_target).unwrap();
        let junction = projection.join("escape");
        let output = Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command"])
            .arg("& { param($p,$t) New-Item -ItemType Junction -Path $p -Target $t | Out-Null }")
            .arg(&junction)
            .arg(&junction_target)
            .output()
            .unwrap();
        assert!(output.status.success(), "junction setup failed: {output:?}");

        let error = match PreparedIsolation::prepare(&source, &projection, "exec_failure", true) {
            Ok(_) => panic!("reparse-point isolation setup unexpectedly succeeded"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("unsupported reparse point"));
        assert!(!base.join("managed/.exec_failure-private").exists());
        assert!(!source.join("COMMAND_MUST_NOT_RUN").exists());

        fs::remove_dir(&junction).unwrap();
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn source_reparse_point_failure_is_attributed_to_the_source_repository() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base = std::env::temp_dir().join(format!(
            "sunlight-source-reparse-{}-{unique}",
            std::process::id()
        ));
        let source = base.join("source");
        let projection = base.join("managed/projection/root");
        let junction_target = base.join("junction-target");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(&projection).unwrap();
        fs::create_dir_all(&junction_target).unwrap();
        let junction = source.join("linked-source");
        let output = Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command"])
            .arg("& { param($p,$t) New-Item -ItemType Junction -Path $p -Target $t | Out-Null }")
            .arg(&junction)
            .arg(&junction_target)
            .output()
            .unwrap();
        assert!(output.status.success(), "junction setup failed: {output:?}");

        let error =
            match PreparedIsolation::prepare(&source, &projection, "exec_source_reparse", true) {
                Ok(_) => panic!("source reparse-point isolation setup unexpectedly succeeded"),
                Err(error) => error,
            };
        let message = error.to_string();
        assert!(message.contains("source repository contains unsupported reparse point"));
        assert!(!message.contains("execution projection contains"));
        assert!(!base
            .join("managed/projection/.exec_source_reparse-private")
            .exists());

        fs::remove_dir(&junction).unwrap();
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn stale_recovery_preserves_a_live_concurrent_profile() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base = std::env::temp_dir().join(format!(
            "sunlight-live-isolation-recovery-{}-{unique}",
            std::process::id()
        ));
        let source = base.join("source");
        let managed_root = source.join(".sunlight/projections");
        let projection = managed_root.join("projection_execution_copy_native_0001/root");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(&projection).unwrap();

        let mut isolation =
            PreparedIsolation::prepare(&source, &projection, "exec_live", true).unwrap();
        let journal = isolation.cleanup_journal.clone().unwrap();
        let profile = isolation.app_container_name.clone().unwrap();
        recover_stale_app_containers(&source, &managed_root).unwrap();

        assert!(journal.is_file());
        assert_eq!(
            isolation.app_container_name.as_deref(),
            Some(profile.as_str())
        );
        isolation.finish().unwrap();
        assert!(!journal.exists());
        fs::remove_dir_all(base).unwrap();
    }
}
