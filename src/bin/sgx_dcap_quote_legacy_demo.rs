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
const TEE_SGX_DCAP_QL_LIB_PATH_ENV: &str = "TEE_SGX_DCAP_QL_LIB_PATH";
#[cfg(feature = "tee-hardware")]
const TEE_SGX_REPORT_B64_ENV: &str = "TEE_SGX_REPORT_B64";
#[cfg(feature = "tee-hardware")]
const TEE_SGX_REPORT_GENERATOR_CMD_ENV: &str = "TEE_SGX_REPORT_GENERATOR_CMD";
#[cfg(feature = "tee-hardware")]
const TEE_SGX_REPORT_HEX_ENV: &str = "TEE_SGX_REPORT_HEX";
#[cfg(feature = "tee-hardware")]
const TEE_SGX_REPORT_PATH_ENV: &str = "TEE_SGX_REPORT_PATH";
#[cfg(feature = "tee-hardware")]
const SGX_REPORT_LEN: usize = 432;

#[cfg(feature = "tee-hardware")]
fn main() {
    let library_path = env::var(TEE_SGX_DCAP_QL_LIB_PATH_ENV)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "libsgx_dcap_ql.so.1".to_string());
    println!("demo=legacy");
    println!("ql_library={library_path}");
    println!("skips_get_target_info=true");
    println!(
        "report_source_hint={TEE_SGX_REPORT_GENERATOR_CMD_ENV} | {TEE_SGX_REPORT_B64_ENV} | {TEE_SGX_REPORT_HEX_ENV} | {TEE_SGX_REPORT_PATH_ENV}"
    );

    let report = [0u8; SGX_REPORT_LEN];
    let library = unsafe { open_library(&library_path) };
    let get_quote_size: unsafe extern "C" fn(*mut u32) -> u32 =
        unsafe { load_symbol(library, "sgx_qe_get_quote_size") };
    let get_quote: unsafe extern "C" fn(*const c_void, u32, *mut u8) -> u32 =
        unsafe { load_symbol(library, "sgx_qe_get_quote") };

    let mut quote_size = 0u32;
    let quote_size_rc = unsafe { get_quote_size(&mut quote_size) };
    println!("get_quote_size_rc=0x{quote_size_rc:08x}");
    println!("quote_size={quote_size}");

    if quote_size_rc == 0 && quote_size > 0 {
        let mut quote = vec![0u8; quote_size as usize];
        let quote_rc = unsafe { get_quote(report.as_ptr().cast(), quote_size, quote.as_mut_ptr()) };
        println!("get_quote_rc=0x{quote_rc:08x}");
    } else {
        println!("get_quote_rc=skipped");
    }

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
