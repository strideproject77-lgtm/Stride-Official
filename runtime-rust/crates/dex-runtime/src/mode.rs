//! Shared execution-mode policy for the single Stride runtime.
//!
//! These modes select capabilities/backends; they do not create separate runtimes.
//! Managed DEX execution remains in the shared runtime in every mode.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionMode {
    /// No JIT and no user certificate. Uses prebuilt, app-signed native artifacts
    /// where available; APKs requiring unsupported native artifacts are rejected.
    FullyJitLess,
    /// No JIT. A matching developer identity is required for device-prepared,
    /// converted native artifacts.
    JitLessWithP12,
    /// Uses a platform-authorized JIT path. A P12 is not itself JIT authorization.
    FullyJit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManagedExecutionBackend {
    /// Initial shared baseline; future optimizations remain behind this interface.
    Interpreter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeExecutionBackend {
    PrebuiltSignedArtifacts,
    CertificateSignedArtifacts,
    AuthorizedJitTranslation,
    /// Mode selection is valid, but the APK's native requirements cannot yet run.
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeCapabilities {
    pub matching_p12_available: bool,
    pub authorized_jit_available: bool,
    pub prebuilt_native_artifacts_available: bool,
    pub certificate_native_pipeline_ready: bool,
    pub jit_native_pipeline_ready: bool,
}

impl Default for RuntimeCapabilities {
    fn default() -> Self {
        Self {
            matching_p12_available: false,
            authorized_jit_available: false,
            prebuilt_native_artifacts_available: false,
            certificate_native_pipeline_ready: false,
            jit_native_pipeline_ready: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModeError {
    P12Required,
    AuthorizedJitRequired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExecutionPlan {
    pub mode: ExecutionMode,
    pub managed: ManagedExecutionBackend,
    pub native: NativeExecutionBackend,
    pub jit_enabled: bool,
    pub p12_required: bool,
}

impl ExecutionMode {
    /// Resolve a user-selected mode against capabilities detected at runtime.
    /// This only builds a truthful execution plan; it does not claim that a
    /// not-yet-implemented native backend can execute an APK.
    pub fn resolve(self, caps: RuntimeCapabilities) -> Result<ExecutionPlan, ModeError> {
        let (native, jit_enabled, p12_required) = match self {
            Self::FullyJitLess => (
                if caps.prebuilt_native_artifacts_available {
                    NativeExecutionBackend::PrebuiltSignedArtifacts
                } else {
                    NativeExecutionBackend::Unavailable
                },
                false,
                false,
            ),
            Self::JitLessWithP12 => {
                if !caps.matching_p12_available {
                    return Err(ModeError::P12Required);
                }
                (
                    if caps.certificate_native_pipeline_ready {
                        NativeExecutionBackend::CertificateSignedArtifacts
                    } else {
                        NativeExecutionBackend::Unavailable
                    },
                    false,
                    true,
                )
            }
            Self::FullyJit => {
                if !caps.authorized_jit_available {
                    return Err(ModeError::AuthorizedJitRequired);
                }
                (
                    if caps.jit_native_pipeline_ready {
                        NativeExecutionBackend::AuthorizedJitTranslation
                    } else {
                        NativeExecutionBackend::Unavailable
                    },
                    true,
                    false,
                )
            }
        };

        Ok(ExecutionPlan {
            mode: self,
            managed: ManagedExecutionBackend::Interpreter,
            native,
            jit_enabled,
            p12_required,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_modes_share_the_same_managed_runtime() {
        let no_native = RuntimeCapabilities::default();
        for mode in [
            ExecutionMode::FullyJitLess,
            ExecutionMode::JitLessWithP12,
            ExecutionMode::FullyJit,
        ] {
            let caps = match mode {
                ExecutionMode::FullyJitLess => no_native,
                ExecutionMode::JitLessWithP12 => RuntimeCapabilities {
                    matching_p12_available: true,
                    ..no_native
                },
                ExecutionMode::FullyJit => RuntimeCapabilities {
                    authorized_jit_available: true,
                    ..no_native
                },
            };
            let plan = mode.resolve(caps).unwrap();
            assert_eq!(plan.managed, ManagedExecutionBackend::Interpreter);
        }
    }

    #[test]
    fn fully_jitless_never_requires_p12_or_jit() {
        let plan = ExecutionMode::FullyJitLess
            .resolve(RuntimeCapabilities::default())
            .unwrap();
        assert!(!plan.jit_enabled);
        assert!(!plan.p12_required);
        assert_eq!(plan.native, NativeExecutionBackend::Unavailable);
    }

    #[test]
    fn p12_mode_requires_matching_identity_but_not_jit() {
        assert_eq!(
            ExecutionMode::JitLessWithP12.resolve(RuntimeCapabilities::default()),
            Err(ModeError::P12Required)
        );
        let plan = ExecutionMode::JitLessWithP12
            .resolve(RuntimeCapabilities {
                matching_p12_available: true,
                ..RuntimeCapabilities::default()
            })
            .unwrap();
        assert!(!plan.jit_enabled);
        assert!(plan.p12_required);
    }

    #[test]
    fn jit_mode_requires_platform_authorization_not_a_p12() {
        assert_eq!(
            ExecutionMode::FullyJit.resolve(RuntimeCapabilities::default()),
            Err(ModeError::AuthorizedJitRequired)
        );
        let plan = ExecutionMode::FullyJit
            .resolve(RuntimeCapabilities {
                authorized_jit_available: true,
                matching_p12_available: false,
                ..RuntimeCapabilities::default()
            })
            .unwrap();
        assert!(plan.jit_enabled);
        assert!(!plan.p12_required);
    }

    #[test]
    fn native_backends_are_reported_only_when_the_pipeline_is_ready() {
        let p12_plan = ExecutionMode::JitLessWithP12
            .resolve(RuntimeCapabilities {
                matching_p12_available: true,
                certificate_native_pipeline_ready: true,
                ..RuntimeCapabilities::default()
            })
            .unwrap();
        assert_eq!(
            p12_plan.native,
            NativeExecutionBackend::CertificateSignedArtifacts
        );

        let jit_plan = ExecutionMode::FullyJit
            .resolve(RuntimeCapabilities {
                authorized_jit_available: true,
                jit_native_pipeline_ready: true,
                ..RuntimeCapabilities::default()
            })
            .unwrap();
        assert_eq!(
            jit_plan.native,
            NativeExecutionBackend::AuthorizedJitTranslation
        );
    }

    #[test]
    fn prebuilt_artifacts_work_without_p12_or_jit() {
        let plan = ExecutionMode::FullyJitLess
            .resolve(RuntimeCapabilities {
                prebuilt_native_artifacts_available: true,
                ..RuntimeCapabilities::default()
            })
            .unwrap();
        assert_eq!(plan.native, NativeExecutionBackend::PrebuiltSignedArtifacts);
        assert!(!plan.jit_enabled);
        assert!(!plan.p12_required);
    }
}
