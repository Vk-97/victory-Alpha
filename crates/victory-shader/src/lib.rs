//! Xbox Series shaders to host shaders.
//!
//! An Xbox Series shader is a standard DXBC container ([`dxbc`]). Its `DXIL`
//! part ([`dxil`]) may carry Xbox additions around the LLVM bitcode: an
//! `XDAT` table before it indexing blobs after it, among them `xbop`
//! (compiler options) and the zlib-compressed GFX10 code. The bitcode may use
//! console-only intrinsics, so the GFX10 code is translated in general.

pub mod bvh;
pub mod dxbc;
pub mod dxil;
pub mod emit;
pub mod gfx10;
pub mod io;
pub mod ir;
pub mod native;
pub mod resource;
pub mod spill;
pub mod structure;

pub use dxbc::{Container, FourCC, Part};
pub use dxil::{DxilProgram, ShaderKind, XboxExtras};
pub use native::{
    HwStage, NativeProgram, ResourceType, RootParam, RootType, UsedResource, native_programs,
};

/// Pointer wrapper for the DirectX 12 Agility SDK `D3D12SDKPath` export.
#[cfg(windows)]
#[doc(hidden)]
#[repr(transparent)]
pub struct AgilitySdkPath(pub *const core::ffi::c_char);

// SAFETY: only constructed from static NUL-terminated C string literals.
#[cfg(windows)]
unsafe impl Sync for AgilitySdkPath {}

/// Parses the decimal Agility SDK minor version string emitted by `build.rs`.
#[cfg(windows)]
#[doc(hidden)]
pub const fn parse_sdk_version(s: &str) -> u32 {
    let bytes = s.as_bytes();
    let mut out = 0u32;
    let mut i = 0;
    while i < bytes.len() {
        if let b'0'..=b'9' = bytes[i] {
            out = out * 10 + (bytes[i] - b'0') as u32;
        }
        i += 1;
    }
    out
}

/// Exports `D3D12SDKVersion` and `D3D12SDKPath` so `D3D12.dll` loads the staged
/// DirectX 12 Agility SDK runtime from `.\D3D12\` (required on Windows 10 for SM 6.6).
#[cfg(windows)]
#[macro_export]
macro_rules! export_agility_sdk {
    ($version_str:expr) => {
        #[allow(non_upper_case_globals)]
        #[unsafe(no_mangle)]
        pub static D3D12SDKVersion: u32 = $crate::parse_sdk_version($version_str);

        #[allow(non_upper_case_globals)]
        #[unsafe(no_mangle)]
        pub static D3D12SDKPath: $crate::AgilitySdkPath =
            $crate::AgilitySdkPath(c".\\D3D12\\".as_ptr());
    };
}

#[cfg(all(test, windows, victory_agility_sdk))]
export_agility_sdk!(env!("VICTORY_AGILITY_SDK_VERSION"));

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ShaderError {
    #[error("not a DXBC container")]
    NotDxbc,
    #[error("malformed shader: {0}")]
    Malformed(&'static str),
    #[error("GFX10 code at {offset:#x}: {reason}")]
    Isa { offset: usize, reason: &'static str },
    /// Valid code the translator cannot handle yet: an opcode name or a construct.
    #[error("GFX10 code at {offset:#x}: {what} is not supported")]
    Unsupported { offset: usize, what: &'static str },
}
