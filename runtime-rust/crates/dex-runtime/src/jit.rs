//! Capability-gated host bridge for an authorized ARM64 JIT backend.
//!
//! This module does not request private entitlements or bypass iOS policy.
//! The platform adapter is intentionally separate from the portable Rust runtime.
//! A platform adapter must prove authorization and implement the W^X transition.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JitArchitecture {
    Arm64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JitHostCapabilities {
    pub architecture: JitArchitecture,
    pub max_code_bytes: usize,
    pub supports_write_xor_execute: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CodeRegion(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JitArtifact<'a> {
    pub architecture: JitArchitecture,
    pub bytes: &'a [u8],
    pub entry_offset: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InstalledCode {
    pub region: CodeRegion,
    pub entry_offset: usize,
    pub code_len: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JitError {
    AuthorizationDenied,
    WriteExecuteTransitionUnavailable,
    UnsupportedArchitecture,
    EmptyArtifact,
    ArtifactTooLarge,
    InvalidEntryOffset,
    HostFailure,
}

/// Implemented only by a platform adapter backed by a documented, authorized
/// executable-memory path. Writes happen before the region is sealed executable.
pub trait AuthorizedJitHost {
    fn authorize(&mut self) -> Result<JitHostCapabilities, JitError>;
    fn allocate_writable(&mut self, size: usize) -> Result<CodeRegion, JitError>;
    fn write_writable(
        &mut self,
        region: CodeRegion,
        offset: usize,
        bytes: &[u8],
    ) -> Result<(), JitError>;
    fn seal_executable(&mut self, region: CodeRegion) -> Result<(), JitError>;
    fn synchronize_instruction_cache(&mut self, region: CodeRegion) -> Result<(), JitError>;
    fn release(&mut self, region: CodeRegion) -> Result<(), JitError>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct AuthorizedJitBackend;

impl AuthorizedJitBackend {
    /// Installs compiler-produced ARM64 code only after the host confirms
    /// authorization, a bounded allocation, and a write-to-execute transition.
    /// The host owns all platform memory operations and policy enforcement.
    pub fn install<H: AuthorizedJitHost>(
        &self,
        host: &mut H,
        artifact: JitArtifact<'_>,
    ) -> Result<InstalledCode, JitError> {
        if artifact.bytes.is_empty() {
            return Err(JitError::EmptyArtifact);
        }
        if artifact.entry_offset >= artifact.bytes.len() {
            return Err(JitError::InvalidEntryOffset);
        }
        if artifact.architecture != JitArchitecture::Arm64 {
            return Err(JitError::UnsupportedArchitecture);
        }

        let caps = host.authorize()?;
        if caps.architecture != artifact.architecture {
            return Err(JitError::UnsupportedArchitecture);
        }
        if !caps.supports_write_xor_execute {
            return Err(JitError::WriteExecuteTransitionUnavailable);
        }
        if artifact.bytes.len() > caps.max_code_bytes {
            return Err(JitError::ArtifactTooLarge);
        }

        let region = host.allocate_writable(artifact.bytes.len())?;
        let install_result = (|| {
            host.write_writable(region, 0, artifact.bytes)?;
            host.seal_executable(region)?;
            host.synchronize_instruction_cache(region)?;
            Ok(InstalledCode {
                region,
                entry_offset: artifact.entry_offset,
                code_len: artifact.bytes.len(),
            })
        })();

        if install_result.is_err() {
            let _ = host.release(region);
        }
        install_result
    }

    pub fn release<H: AuthorizedJitHost>(
        &self,
        host: &mut H,
        installed: InstalledCode,
    ) -> Result<(), JitError> {
        host.release(installed.region)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct MockHost {
        authorized: bool,
        fail_seal: bool,
        released: bool,
        wrote: bool,
        sealed: bool,
        cache_synced: bool,
    }

    impl AuthorizedJitHost for MockHost {
        fn authorize(&mut self) -> Result<JitHostCapabilities, JitError> {
            if !self.authorized {
                return Err(JitError::AuthorizationDenied);
            }
            Ok(JitHostCapabilities {
                architecture: JitArchitecture::Arm64,
                max_code_bytes: 4096,
                supports_write_xor_execute: true,
            })
        }

        fn allocate_writable(&mut self, _size: usize) -> Result<CodeRegion, JitError> {
            Ok(CodeRegion(7))
        }

        fn write_writable(
            &mut self,
            _region: CodeRegion,
            _offset: usize,
            _bytes: &[u8],
        ) -> Result<(), JitError> {
            self.wrote = true;
            Ok(())
        }

        fn seal_executable(&mut self, _region: CodeRegion) -> Result<(), JitError> {
            if self.fail_seal {
                return Err(JitError::HostFailure);
            }
            self.sealed = true;
            Ok(())
        }

        fn synchronize_instruction_cache(&mut self, _region: CodeRegion) -> Result<(), JitError> {
            self.cache_synced = true;
            Ok(())
        }

        fn release(&mut self, _region: CodeRegion) -> Result<(), JitError> {
            self.released = true;
            Ok(())
        }
    }

    #[test]
    fn refuses_to_install_without_platform_authorization() {
        let mut host = MockHost::default();
        let result = AuthorizedJitBackend.install(
            &mut host,
            JitArtifact {
                architecture: JitArchitecture::Arm64,
                bytes: &[1, 2, 3, 4],
                entry_offset: 0,
            },
        );
        assert_eq!(result, Err(JitError::AuthorizationDenied));
        assert!(!host.wrote);
    }

    #[test]
    fn installs_only_after_write_seal_and_cache_sync() {
        let mut host = MockHost {
            authorized: true,
            ..MockHost::default()
        };
        let installed = AuthorizedJitBackend
            .install(
                &mut host,
                JitArtifact {
                    architecture: JitArchitecture::Arm64,
                    bytes: &[1, 2, 3, 4],
                    entry_offset: 0,
                },
            )
            .unwrap();
        assert_eq!(installed.region, CodeRegion(7));
        assert!(host.wrote && host.sealed && host.cache_synced);
    }

    #[test]
    fn releases_region_when_executable_seal_fails() {
        let mut host = MockHost {
            authorized: true,
            fail_seal: true,
            ..MockHost::default()
        };
        let result = AuthorizedJitBackend.install(
            &mut host,
            JitArtifact {
                architecture: JitArchitecture::Arm64,
                bytes: &[1, 2, 3, 4],
                entry_offset: 0,
            },
        );
        assert_eq!(result, Err(JitError::HostFailure));
        assert!(host.released);
    }

    #[test]
    fn rejects_invalid_artifacts_before_requesting_authorization() {
        let mut host = MockHost::default();
        let empty = AuthorizedJitBackend.install(
            &mut host,
            JitArtifact {
                architecture: JitArchitecture::Arm64,
                bytes: &[],
                entry_offset: 0,
            },
        );
        assert_eq!(empty, Err(JitError::EmptyArtifact));
        let invalid_entry = AuthorizedJitBackend.install(
            &mut host,
            JitArtifact {
                architecture: JitArchitecture::Arm64,
                bytes: &[1, 2],
                entry_offset: 2,
            },
        );
        assert_eq!(invalid_entry, Err(JitError::InvalidEntryOffset));
    }
}
