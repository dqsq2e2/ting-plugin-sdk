//! WASM imports matching the Host's bounded binary adapter.

use crate::{Host, Result, SdkError};
use ting_plugin_contract::format_calls::{ChunkRef, MAX_MEDIA_CHUNK_BYTES, ResourceId};

#[link(wasm_import_module = "ting_env")]
unsafe extern "C" {
    fn host_invoke(method: *const u8, method_len: usize, input: *const u8, input_len: usize)
    -> i32;
    fn host_response_size(handle: i32) -> i32;
    fn host_read_body(handle: i32, output: *mut u8, capacity: usize) -> i32;
    fn resource_read_at(
        id: *const u8,
        id_len: usize,
        offset: i64,
        output: *mut u8,
        capacity: usize,
        eof: *mut u8,
    ) -> i32;
    fn resource_write_at(
        id: *const u8,
        id_len: usize,
        offset: i64,
        input: *const u8,
        input_len: usize,
    ) -> i32;
    fn chunk_create(input: *const u8, input_len: usize, output: *mut u8, capacity: usize) -> i32;
    fn chunk_copy(id: *const u8, id_len: usize, output: *mut u8, capacity: usize) -> i32;
}

pub struct WasmHost;

fn count(result: i32) -> Result<usize> {
    if result < 0 {
        crate::native::check_status(result)?;
    }
    Ok(result as usize)
}

impl Host for WasmHost {
    fn invoke(&self, method: &str, input: serde_json::Value) -> Result<serde_json::Value> {
        let input = serde_json::to_vec(&input).map_err(SdkError::parse)?;
        if input.len() > crate::native::MAX_CONTROL_BYTES || method.len() > 128 {
            return Err(SdkError::invalid("Host control input exceeds limit"));
        }
        let handle =
            unsafe { host_invoke(method.as_ptr(), method.len(), input.as_ptr(), input.len()) };
        if handle <= 0 {
            crate::native::check_status(handle)?;
        }
        let length = count(unsafe { host_response_size(handle) })?;
        if length > crate::native::MAX_CONTROL_BYTES {
            return Err(SdkError::invalid("Host response exceeds limit"));
        }
        let mut output = vec![0u8; length];
        let read = count(unsafe { host_read_body(handle, output.as_mut_ptr(), output.len()) })?;
        if read != length {
            return Err(SdkError::invalid("Host response length mismatch"));
        }
        serde_json::from_slice(&output).map_err(SdkError::parse)
    }

    fn read_at(&self, id: &ResourceId, offset: u64, max_bytes: usize) -> Result<(Vec<u8>, bool)> {
        if max_bytes == 0 || max_bytes > MAX_MEDIA_CHUNK_BYTES as usize || offset > i64::MAX as u64
        {
            return Err(SdkError::invalid("Invalid resource read range"));
        }
        let mut output = vec![0u8; max_bytes];
        let mut eof = 0u8;
        let length = count(unsafe {
            resource_read_at(
                id.0.as_ptr(),
                id.0.len(),
                offset as i64,
                output.as_mut_ptr(),
                output.len(),
                &mut eof,
            )
        })?;
        if length > output.len() {
            return Err(SdkError::invalid("Invalid read length"));
        }
        output.truncate(length);
        Ok((output, eof != 0))
    }

    fn write_at(&self, id: &ResourceId, offset: u64, bytes: &[u8]) -> Result<usize> {
        if offset > i64::MAX as u64 {
            return Err(SdkError::invalid("Invalid write offset"));
        }
        let written = count(unsafe {
            resource_write_at(
                id.0.as_ptr(),
                id.0.len(),
                offset as i64,
                bytes.as_ptr(),
                bytes.len(),
            )
        })?;
        if written > bytes.len() {
            return Err(SdkError::invalid("Invalid write length"));
        }
        Ok(written)
    }

    fn chunk_create(&self, bytes: &[u8]) -> Result<ChunkRef> {
        let mut output = [0u8; 128];
        let length = count(unsafe {
            chunk_create(
                bytes.as_ptr(),
                bytes.len(),
                output.as_mut_ptr(),
                output.len(),
            )
        })?;
        let bytes = output
            .get(..length)
            .ok_or_else(|| SdkError::invalid("Invalid chunk id length"))?;
        Ok(ChunkRef(
            std::str::from_utf8(bytes).map_err(SdkError::parse)?.into(),
        ))
    }

    fn chunk_copy(&self, id: &ChunkRef) -> Result<Vec<u8>> {
        let mut output = vec![0u8; MAX_MEDIA_CHUNK_BYTES as usize];
        let length = count(unsafe {
            chunk_copy(id.0.as_ptr(), id.0.len(), output.as_mut_ptr(), output.len())
        })?;
        if length > output.len() {
            return Err(SdkError::invalid("Invalid chunk length"));
        }
        output.truncate(length);
        Ok(output)
    }
}
