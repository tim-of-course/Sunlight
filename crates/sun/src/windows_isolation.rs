use std::ffi::c_void;
use std::fs;
use std::io;
use std::mem::{size_of, zeroed};
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::process::Command;

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
const STD_INPUT_HANDLE: Dword = -10_i32 as Dword;
const STD_OUTPUT_HANDLE: Dword = -11_i32 as Dword;
const STD_ERROR_HANDLE: Dword = -12_i32 as Dword;
const INFINITE: Dword = Dword::MAX;
const WAIT_FAILED: Dword = Dword::MAX;
const STILL_ACTIVE: Dword = 259;
const BOOTSTRAP_SETUP_FAILURE_EXIT_CODE: u8 = 125;
const BOOTSTRAP_MARKER_ENV: &str = "SUNLIGHT_INTERNAL_ISOLATION_MARKER";
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
struct ProcessInformation {
    process: Handle,
    thread: Handle,
    process_id: Dword,
    thread_id: Dword,
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

#[link(name = "kernel32")]
extern "system" {
    fn GetCurrentProcess() -> Handle;
    #[cfg(debug_assertions)]
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
    fn TerminateProcess(process: Handle, exit_code: Dword) -> Bool;
    fn CloseHandle(handle: Handle) -> Bool;
    fn LocalFree(memory: *mut c_void) -> *mut c_void;
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

pub(crate) struct PreparedIsolation {
    runtime_root: PathBuf,
    marker: PathBuf,
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
    ) -> io::Result<Self> {
        let parent = projection_root.parent().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "projection root has no parent")
        })?;
        let runtime_root = parent.join(format!(".{execution_id}-private"));
        if runtime_root.exists() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "execution private runtime root already exists",
            ));
        }
        fs::create_dir_all(runtime_root.join("temp"))?;
        fs::create_dir_all(runtime_root.join("home/AppData/Local"))?;
        fs::create_dir_all(runtime_root.join("home/AppData/Roaming"))?;
        let managed_root = projection_root
            .parent()
            .and_then(Path::parent)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "projection root is not managed",
                )
            })?;
        let source_root = absolute_lexical_path(source_root)?;
        let managed_root = fs::canonicalize(managed_root)?;
        if let Err(error) = inject_test_setup_failure()
            .and_then(|()| validate_source_tree(&source_root, &managed_root))
            .and_then(|()| secure_private_tree(projection_root))
            .and_then(|()| secure_private_tree(&runtime_root))
        {
            let _ = fs::remove_dir_all(&runtime_root);
            return Err(error);
        }
        Ok(Self {
            marker: runtime_root.join("low-integrity-ready"),
            runtime_root,
        })
    }

    pub(crate) fn bootstrap_command(&self, argv: &[String]) -> io::Result<Command> {
        let executable = std::env::current_exe()?;
        let mut command = Command::new(executable);
        command
            .arg("__sunlight-low-integrity-bootstrap")
            .arg(&argv[0]);
        command.env(BOOTSTRAP_MARKER_ENV, &self.marker);
        Ok(command)
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
}

impl Drop for PreparedIsolation {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.runtime_root);
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
    let token = restricted_low_integrity_token()?;
    let mut command_line = windows_command_line(argv);
    let mut startup: StartupInfoW = unsafe { zeroed() };
    startup.cb = size_of::<StartupInfoW>() as Dword;
    startup.flags = STARTF_USESTDHANDLES;
    startup.std_input = unsafe { GetStdHandle(STD_INPUT_HANDLE) };
    startup.std_output = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) };
    startup.std_error = unsafe { GetStdHandle(STD_ERROR_HANDLE) };
    let mut process: ProcessInformation = unsafe { zeroed() };
    if unsafe {
        CreateProcessAsUserW(
            token.0,
            std::ptr::null(),
            command_line.as_mut_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            1,
            CREATE_SUSPENDED,
            std::ptr::null(),
            std::ptr::null(),
            &startup,
            &mut process,
        )
    } == 0
    {
        return Err(last_error("create restricted low-integrity command"));
    }
    let process_handle = OwnedHandle(process.process);
    let thread_handle = OwnedHandle(process.thread);
    if let Err(error) = fs::write(marker, b"windows_low_integrity_private_projection_v1\n") {
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

fn secure_private_tree(root: &Path) -> io::Result<()> {
    let descriptor = private_security_descriptor()?;
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

#[cfg(debug_assertions)]
fn test_setup_failure_requested() -> bool {
    if std::env::var(TEST_FAILPOINT_ENV).as_deref() != Ok("prepare_after_runtime_root") {
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

#[cfg(debug_assertions)]
fn parent_process() -> Option<(Dword, PathBuf)> {
    const PROCESS_QUERY_LIMITED_INFORMATION: Dword = 0x1000;
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

fn private_security_descriptor() -> io::Result<LocalMemory> {
    let sid = current_user_sid_string()?;
    let sddl = wide_null(&format!(
        "D:P(A;OICI;FA;;;{sid})(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)S:(ML;OICI;NW;;;LW)"
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

        let error = match PreparedIsolation::prepare(&source, &projection, "exec_failure") {
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

        let error = match PreparedIsolation::prepare(&source, &projection, "exec_source_reparse") {
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
}
