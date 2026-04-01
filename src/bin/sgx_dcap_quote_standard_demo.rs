#[cfg(not(feature = "tee-hardware"))]
fn main() {
    eprintln!("requires `--features tee-hardware`");
    std::process::exit(1);
}

#[cfg(feature = "tee-hardware")]
use libc::{RTLD_LAZY, c_void, dlclose, dlerror, dlopen, dlsym};
#[cfg(feature = "tee-hardware")]
use std::env;
#[cfg(feature = "tee-hardware")]
use std::ffi::{CStr, CString};
#[cfg(feature = "tee-hardware")]
use vault_service::tee::SgxHostRuntime;
#[cfg(feature = "tee-hardware")]
use vault_service::tee::ffi_types::{SGX_REPORT_DATA_LEN, SGX_TARGET_INFO_LEN};

#[cfg(feature = "tee-hardware")]
const TEE_SGX_DCAP_QL_LIB_PATH_ENV: &str = "TEE_SGX_DCAP_QL_LIB_PATH";

#[cfg(feature = "tee-hardware")]
fn main() {
    let library_path = env::var(TEE_SGX_DCAP_QL_LIB_PATH_ENV)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "libsgx_dcap_ql.so.1".to_string());
    let runtime = SgxHostRuntime::load_from_env().expect("failed to load enclave runtime");
    let report_data = [0u8; SGX_REPORT_DATA_LEN];

    println!("demo=standard");
    println!("ql_library={library_path}");
    println!("enclave_path={}", runtime.enclave_path().display());

    let library = unsafe { open_library(&library_path) };
    let get_target_info: unsafe extern "C" fn(*mut c_void) -> u32 =
        unsafe { load_symbol(library, "sgx_qe_get_target_info") };
    let get_quote_size: unsafe extern "C" fn(*mut u32) -> u32 =
        unsafe { load_symbol(library, "sgx_qe_get_quote_size") };
    let get_quote: unsafe extern "C" fn(*const c_void, u32, *mut u8) -> u32 =
        unsafe { load_symbol(library, "sgx_qe_get_quote") };

    let mut target_info = [0u8; SGX_TARGET_INFO_LEN];
    let target_info_rc = unsafe { get_target_info(target_info.as_mut_ptr().cast()) };
    println!("get_target_info_rc=0x{target_info_rc:08x}");
    if target_info_rc != 0 {
        println!("failed_stage=get_target_info");
        unsafe {
            dlclose(library);
        }
        return;
    }

    let report = runtime
        .get_targeted_report(target_info, report_data)
        .expect("targeted report generation failed");
    println!("targeted_report_len={}", report.len());

    let mut quote_size = 0u32;
    let quote_size_rc = unsafe { get_quote_size(&mut quote_size) };
    println!("get_quote_size_rc=0x{quote_size_rc:08x}");
    println!("quote_size={quote_size}");
    if quote_size_rc != 0 {
        println!("failed_stage=get_quote_size");
        unsafe {
            dlclose(library);
        }
        return;
    }

    let mut quote = vec![0u8; quote_size as usize];
    let quote_rc = unsafe { get_quote(report.as_ptr().cast(), quote_size, quote.as_mut_ptr()) };
    println!("get_quote_rc=0x{quote_rc:08x}");
    println!(
        "failed_stage={}",
        if quote_rc == 0 { "none" } else { "get_quote" }
    );

    unsafe {
        dlclose(library);
    }
}

#[cfg(feature = "tee-hardware")]
unsafe fn open_library(path: &str) -> *mut c_void {
    let c_path = CString::new(path).expect("library path contains NUL");
    let handle = unsafe { dlopen(c_path.as_ptr(), RTLD_LAZY) };
    assert!(!handle.is_null(), "failed to load `{path}`: {}", unsafe {
        last_dlerror()
    });
    handle
}

#[cfg(feature = "tee-hardware")]
unsafe fn load_symbol<T: Copy>(library: *mut c_void, name: &str) -> T {
    let c_name = CString::new(name).expect("symbol contains NUL");
    let symbol = unsafe { dlsym(library, c_name.as_ptr()) };
    assert!(
        !symbol.is_null(),
        "failed to resolve `{name}`: {}",
        unsafe { last_dlerror() }
    );
    unsafe { std::mem::transmute_copy(&symbol) }
}

#[cfg(feature = "tee-hardware")]
unsafe fn last_dlerror() -> String {
    let ptr = unsafe { dlerror() };
    if ptr.is_null() {
        return "unknown dlerror".to_string();
    }
    unsafe { CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned()
}
