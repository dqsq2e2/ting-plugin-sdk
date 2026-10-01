//! Runtime-independent plugin SDK. State belongs to an instance; borrowed Host
//! callbacks and binary buffers cannot be retained beyond an invocation.

pub mod native;
#[cfg(target_arch = "wasm32")]
pub mod wasm;

use contract::format_calls::{ChunkRef, ResourceId};
use contract::protocol::{CallResult, PluginError, PluginErrorCode};
pub use serde_json;
pub use ting_plugin_contract as contract;

pub type Result<T> = std::result::Result<T, SdkError>;

#[derive(Debug)]
pub struct SdkError {
    pub code: PluginErrorCode,
    pub message: String,
}

impl std::fmt::Display for SdkError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for SdkError {}

impl SdkError {
    pub fn new(code: PluginErrorCode, message: impl Into<String>) -> Self {
        let mut message = message.into();
        while message.len() > 512 {
            message.pop();
        }
        Self { code, message }
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new(PluginErrorCode::InvalidInput, message)
    }

    pub fn parse(_: impl std::fmt::Display) -> Self {
        Self::new(
            PluginErrorCode::ParseError,
            "Plugin data could not be parsed",
        )
    }
}

/// Same Host semantics on both Rust runtimes; Native/WASM adapt only transport.
/// Bytes are separate from JSON, and writes are never automatically retried.
pub trait Host {
    fn invoke(&self, method: &str, input: serde_json::Value) -> Result<serde_json::Value>;
    fn read_at(&self, id: &ResourceId, offset: u64, max_bytes: usize) -> Result<(Vec<u8>, bool)>;
    fn write_at(&self, id: &ResourceId, offset: u64, bytes: &[u8]) -> Result<usize>;
    fn chunk_create(&self, bytes: &[u8]) -> Result<ChunkRef>;
    fn chunk_copy(&self, id: &ChunkRef) -> Result<Vec<u8>>;
}

pub fn host_call<T: serde::de::DeserializeOwned>(
    host: &dyn Host,
    method: &str,
    input: impl serde::Serialize,
) -> Result<T> {
    let input = serde_json::to_value(input).map_err(SdkError::parse)?;
    serde_json::from_value(host.invoke(method, input)?).map_err(SdkError::parse)
}

pub fn read_exact_range(
    host: &dyn Host,
    resource: &ResourceId,
    offset: u64,
    length: usize,
) -> Result<Vec<u8>> {
    let mut output = Vec::with_capacity(length);
    while output.len() < length {
        let count =
            (length - output.len()).min(contract::format_calls::MAX_MEDIA_CHUNK_BYTES as usize);
        let (bytes, eof) = host.read_at(resource, offset + output.len() as u64, count)?;
        if bytes.len() > count || bytes.is_empty() || (eof && output.len() + bytes.len() != length)
        {
            return Err(SdkError::new(
                PluginErrorCode::InvalidOutput,
                "Unexpected resource range length",
            ));
        }
        output.extend_from_slice(&bytes);
    }
    Ok(output)
}

/// A bounded Host HTTP response. Callers handling upstream error pages can
/// inspect the status without guessing from the response body.
pub struct HttpResponse {
    pub status: u16,
    pub body: Vec<u8>,
}

/// Fetch through the Host; its domain and redirect checks apply to all runtimes.
pub fn http_request_response(
    host: &dyn Host,
    url: &str,
    method: &str,
    headers: serde_json::Value,
    body: Option<&str>,
) -> Result<HttpResponse> {
    #[derive(serde::Deserialize)]
    struct Response {
        resource: ResourceId,
        length: u64,
        status: u16,
    }
    let response: Response = host_call(
        host,
        "http.request",
        serde_json::json!({
            "url": url, "method": method, "headers": headers,
            "body": body, "timeout_ms": 30_000,
        }),
    )?;
    let outcome = if (100..=599).contains(&response.status) && response.length <= 8 * 1024 * 1024 {
        read_exact_range(host, &response.resource, 0, response.length as usize)
    } else {
        Err(SdkError::invalid("Invalid Host HTTP response"))
    };
    let closed = host.invoke(
        "resources.close",
        serde_json::json!({ "resource": response.resource }),
    );
    match (outcome, closed) {
        (Ok(bytes), Ok(_)) => Ok(HttpResponse {
            status: response.status,
            body: bytes,
        }),
        (Err(error), _) => Err(error),
        (_, Err(error)) => Err(error),
    }
}

pub fn http_request(
    host: &dyn Host,
    url: &str,
    method: &str,
    headers: serde_json::Value,
    body: Option<&str>,
) -> Result<Vec<u8>> {
    Ok(http_request_response(host, url, method, headers, body)?.body)
}

/// Implement once and export through `export_plugin!`. Operations are fixed,
/// inspectable declarations and cannot change dynamically after registration.
pub trait Plugin: Default {
    const ID: &'static str;
    const OPERATIONS: &'static [&'static str];
    fn initialize(&mut self, _config: serde_json::Value, _host: &dyn Host) -> Result<()> {
        Ok(())
    }
    fn shutdown(&mut self, _host: &dyn Host) -> Result<()> {
        Ok(())
    }
    fn invoke(
        &mut self,
        operation: &str,
        input: serde_json::Value,
        host: &dyn Host,
    ) -> Result<serde_json::Value>;
}

pub fn dispatch<P: Plugin>(
    plugin: &mut P,
    operation: &str,
    input: serde_json::Value,
    host: &dyn Host,
) -> CallResult<serde_json::Value> {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match operation {
        "initialize" => plugin
            .initialize(input.get("config").cloned().unwrap_or(input), host)
            .map(|_| serde_json::json!({})),
        "shutdown" => plugin.shutdown(host).map(|_| serde_json::json!({})),
        operation if P::OPERATIONS.contains(&operation) => plugin.invoke(operation, input, host),
        _ => Err(SdkError::new(
            PluginErrorCode::UnsupportedOperation,
            "Operation is not declared",
        )),
    }))
    .unwrap_or_else(|_| {
        Err(SdkError::new(
            PluginErrorCode::InternalError,
            "Plugin invocation panicked",
        ))
    });
    match result {
        Ok(data) => CallResult::success(data),
        Err(error) => CallResult::failure(PluginError {
            code: error.code,
            message: if error.message.is_empty() {
                "Plugin invocation failed".into()
            } else {
                error.message
            },
            details: None,
            plugin_id: P::ID.into(),
            capability_id: "runtime".into(),
            operation: operation.into(),
            // Host binds its authoritative request identity at the gateway.
            request_id: "runtime".into(),
            retryable: false,
        }),
    }
}

#[macro_export]
macro_rules! export_plugin {
    ($plugin:ty) => {
        #[cfg(not(target_arch = "wasm32"))]
        #[unsafe(no_mangle)]
        pub extern "C" fn ting_plugin_abi_v2() -> *const $crate::contract::native_abi::NativeAbiHeader {
            static ABI: $crate::contract::native_abi::NativePluginAbiV2 = $crate::native::abi::<$plugin>();
            &ABI.header
        }

        #[cfg(target_arch = "wasm32")]
        mod ting_exports {
            use super::*;
            const ABI_ERROR: i32 = -7;

            thread_local! {
                static INSTANCE: std::cell::RefCell<$plugin> = std::cell::RefCell::new(<$plugin>::default());
            }
            #[unsafe(no_mangle)]
            pub extern "C" fn initialize() -> i32 {
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| 0))
                    .unwrap_or(ABI_ERROR)
            }
            #[unsafe(no_mangle)]
            pub extern "C" fn shutdown() -> i32 {
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    INSTANCE.with(|slot| {
                        *slot.borrow_mut() = <$plugin>::default();
                    });
                    0
                }))
                .unwrap_or(ABI_ERROR)
            }
            #[unsafe(no_mangle)]
            pub extern "C" fn ting_abi_revision() -> i32 { 2 }
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn supports(pointer: *const u8, len: usize) -> i32 {
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    if pointer.is_null() {
                        return 0;
                    }
                    let name = unsafe { std::str::from_utf8(std::slice::from_raw_parts(pointer, len)) };
                    i32::from(name.is_ok_and(|name| matches!(name, "initialize" | "shutdown")
                        || <$plugin as $crate::Plugin>::OPERATIONS.contains(&name)))
                }))
                .unwrap_or(0)
            }
            #[unsafe(no_mangle)]
            pub extern "C" fn alloc(length: usize) -> *mut u8 {
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    if length > $crate::native::MAX_CONTROL_BYTES + 1 {
                        return std::ptr::null_mut();
                    }
                    Box::into_raw(vec![0u8; length].into_boxed_slice()) as *mut u8
                }))
                .unwrap_or(std::ptr::null_mut())
            }
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn dealloc(pointer: *mut u8, length: usize) {
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    if !pointer.is_null() && length <= $crate::native::MAX_CONTROL_BYTES + 1 {
                        drop(unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(pointer, length)) });
                    }
                }));
            }
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn invoke(method: *const u8, input: *const u8) -> *mut u8 {
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
                    if method.is_null() || input.is_null() {
                        return std::ptr::null_mut();
                    }
                    let name = unsafe { std::ffi::CStr::from_ptr(method.cast()) }.to_str();
                    let input = unsafe { std::ffi::CStr::from_ptr(input.cast()) }.to_bytes();
                    let result = match (name, $crate::serde_json::from_slice(input)) {
                        (Ok(name), Ok(input)) => INSTANCE.with(|slot| {
                            $crate::dispatch::<$plugin>(&mut *slot.borrow_mut(), name, input, &$crate::wasm::WasmHost)
                        }),
                        _ => return std::ptr::null_mut(),
                    };
                    let mut bytes = match $crate::serde_json::to_vec(&result) {
                        Ok(bytes) => bytes,
                        Err(_) => return std::ptr::null_mut(),
                    };
                    if bytes.len() > $crate::native::MAX_CONTROL_BYTES {
                        return std::ptr::null_mut();
                    }
                    bytes.push(0);
                    Box::into_raw(bytes.into_boxed_slice()) as *mut u8
                }))
                .unwrap_or(std::ptr::null_mut())
            }
        }
    };
}

#[cfg(test)]
mod http_tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    struct HttpHost {
        closed: AtomicBool,
        status: u16,
        declared_length: u64,
        bytes: Vec<u8>,
    }

    impl Host for HttpHost {
        fn invoke(&self, method: &str, _: serde_json::Value) -> Result<serde_json::Value> {
            match method {
                "http.request" => Ok(serde_json::json!({
                    "resource": "response", "length": self.declared_length, "status": self.status
                })),
                "resources.close" => {
                    self.closed.store(true, Ordering::SeqCst);
                    Ok(serde_json::json!({}))
                }
                _ => Err(SdkError::invalid("Unknown Host operation")),
            }
        }

        fn read_at(
            &self,
            _: &ResourceId,
            offset: u64,
            max_bytes: usize,
        ) -> Result<(Vec<u8>, bool)> {
            let bytes = self.bytes.get(offset as usize..).unwrap_or_default();
            let part = bytes[..bytes.len().min(max_bytes)].to_vec();
            Ok((part, bytes.len() <= max_bytes))
        }
        fn write_at(&self, _: &ResourceId, _: u64, _: &[u8]) -> Result<usize> {
            unreachable!()
        }
        fn chunk_create(&self, _: &[u8]) -> Result<ChunkRef> {
            unreachable!()
        }
        fn chunk_copy(&self, _: &ChunkRef) -> Result<Vec<u8>> {
            unreachable!()
        }
    }

    #[test]
    fn http_preserves_upstream_status_and_releases_response_resource() {
        let host = HttpHost {
            closed: AtomicBool::new(false),
            status: 404,
            declared_length: 3,
            bytes: b"bad".to_vec(),
        };
        let result = http_request_response(
            &host,
            "https://example.com",
            "GET",
            serde_json::json!({}),
            None,
        )
        .unwrap();
        assert_eq!(result.status, 404);
        assert_eq!(result.body, b"bad");
        assert!(host.closed.load(Ordering::SeqCst));
    }

    #[test]
    fn http_releases_resource_after_short_read() {
        let host = HttpHost {
            closed: AtomicBool::new(false),
            status: 200,
            declared_length: 4,
            bytes: b"bad".to_vec(),
        };
        assert!(
            http_request_response(
                &host,
                "https://example.com",
                "GET",
                serde_json::json!({}),
                None
            )
            .is_err()
        );
        assert!(host.closed.load(Ordering::SeqCst));
    }
}
