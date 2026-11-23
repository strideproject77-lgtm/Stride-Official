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

/// Result of compiling a restricted, straight-line DEX method to ARM64.
///
/// The bytes are compiler output only. They are not executable until an
/// authorized platform host installs them through AuthorizedJitBackend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledDexMethod {
    pub architecture: JitArchitecture,
    pub code: Vec<u8>,
    pub entry_offset: usize,
    pub dex_instructions: usize,
}

impl CompiledDexMethod {
    pub fn artifact(&self) -> JitArtifact<'_> {
        JitArtifact {
            architecture: self.architecture,
            bytes: &self.code,
            entry_offset: self.entry_offset,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JitCompileError {
    EmptyMethod,
    InvalidRegisterCount(usize),
    TruncatedInstruction { pc: usize },
    InvalidRegister { pc: usize, register: usize },
    UnsupportedOpcode { pc: usize, opcode: u8 },
    MissingReturn,
}

fn emit_word(code: &mut Vec<u8>, instruction: u32) {
    code.extend_from_slice(&instruction.to_le_bytes());
}

fn emit_mov_imm32(code: &mut Vec<u8>, register: u8, value: i32) {
    let bits = value as u32;
    emit_word(code, 0x5280_0000 | ((bits & 0xffff) << 5) | register as u32);
    if bits >> 16 != 0 {
        emit_word(code, 0x72a0_0000 | (((bits >> 16) & 0xffff) << 5) | register as u32);
    }
}

fn emit_mov_register(code: &mut Vec<u8>, destination: u8, source: u8) {
    // ORR Wd, WZR, Wm (the AArch64 MOV register alias).
    emit_word(code, 0x2a00_03e0 | ((source as u32) << 16) | destination as u32);
}

/// Compile a small integer-only DEX method into an AArch64 leaf function.
///
/// ABI: int32_t entry(const uint32_t *dex_registers). The entry loads the
/// method's register file, executes supported const/add/sub/mul/return
/// instructions, and returns the selected DEX register in W0. Calls, branches,
/// object operations, and exception edges intentionally fall back to the
/// interpreter until their semantics have dedicated compiler support.
pub fn compile_dex_method(
    code_units: &[u16],
    registers_size: usize,
) -> Result<CompiledDexMethod, JitCompileError> {
    if code_units.is_empty() {
        return Err(JitCompileError::EmptyMethod);
    }
    if registers_size == 0 || registers_size > 16 {
        return Err(JitCompileError::InvalidRegisterCount(registers_size));
    }

    let mut code = Vec::new();
    // Preserve the incoming register-file pointer in X16 before loading W0.
    emit_word(&mut code, 0xaa00_03f0);
    for register in 0..registers_size {
        // LDR Wt, [X16, #register * 4] (unsigned scaled immediate).
        let byte_offset = register * 4;
        emit_word(
            &mut code,
            0xb940_0000
                | (((byte_offset / 4) as u32) << 10)
                | (16 << 5)
                | register as u32,
        );
    }

    let mut pc = 0usize;
    let mut instruction_count = 0usize;
    let mut returned = false;
    while pc < code_units.len() {
        let first = code_units[pc];
        let opcode = (first & 0xff) as u8;
        let a8 = (first >> 8) as u8;
        let a4 = a8 & 0x0f;
        let mut width = 1usize;

        let check_register = |register: usize| {
            if register >= registers_size || register >= 16 {
                Err(JitCompileError::InvalidRegister { pc, register })
            } else {
                Ok(register as u8)
            }
        };

        match opcode {
            0x12 => { // const/4 vA, #+B
                let register = check_register(a4 as usize)?;
                let literal = ((first >> 12) & 0x0f) as i8;
                let literal = if literal & 0x08 != 0 { literal - 16 } else { literal };
                emit_mov_imm32(&mut code, register, literal as i32);
            }
            0x13 => { // const/16 vAA, #+BBBB
                if pc + 1 >= code_units.len() {
                    return Err(JitCompileError::TruncatedInstruction { pc });
                }
                width = 2;
                let register = check_register(a8 as usize)?;
                emit_mov_imm32(&mut code, register, code_units[pc + 1] as i16 as i32);
            }
            0x14 => { // const vAA, #+BBBBBBBB
                if pc + 2 >= code_units.len() {
                    return Err(JitCompileError::TruncatedInstruction { pc });
                }
                width = 3;
                let register = check_register(a8 as usize)?;
                let value = (code_units[pc + 1] as u32)
                    | ((code_units[pc + 2] as u32) << 16);
                emit_mov_imm32(&mut code, register, value as i32);
            }
            0x90 | 0x91 | 0x92 => { // add-int, sub-int, mul-int (23x)
                if pc + 1 >= code_units.len() {
                    return Err(JitCompileError::TruncatedInstruction { pc });
                }
                width = 2;
                let destination = check_register(a8 as usize)?;
                let sources = code_units[pc + 1];
                let left = check_register((sources & 0xff) as usize)?;
                let right = check_register((sources >> 8) as usize)?;
                let base = match opcode {
                    0x90 => 0x0b00_0000, // ADD Wd, Wn, Wm
                    0x91 => 0x4b00_0000, // SUB Wd, Wn, Wm
                    _ => 0x1b00_7c00,    // MUL Wd, Wn, Wm (MADD with WZR)
                };
                emit_word(
                    &mut code,
                    base | ((right as u32) << 16)
                        | ((left as u32) << 5)
                        | destination as u32,
                );
            }
            0x0f => { // return vAA
                let register = check_register(a8 as usize)?;
                if register != 0 {
                    emit_mov_register(&mut code, 0, register);
                }
                emit_word(&mut code, 0xd65f_03c0); // RET
                returned = true;
                instruction_count += 1;
                break;
            }
            0x0e => { // return-void
                emit_mov_imm32(&mut code, 0, 0);
                emit_word(&mut code, 0xd65f_03c0);
                returned = true;
                instruction_count += 1;
                break;
            }
            _ => return Err(JitCompileError::UnsupportedOpcode { pc, opcode }),
        }
        pc += width;
        instruction_count += 1;
    }

    if !returned {
        return Err(JitCompileError::MissingReturn);
    }

    Ok(CompiledDexMethod {
        architecture: JitArchitecture::Arm64,
        code,
        entry_offset: 0,
        dex_instructions: instruction_count,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledDexMethod {
    pub code: InstalledCode,
    pub dex_instructions: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JitPipelineError {
    Compile(JitCompileError),
    Install(JitError),
}

impl AuthorizedJitBackend {
    /// Compiles a supported DEX method and installs it through the authorized
    /// platform host. This deliberately does not expose an entry point until
    /// the host has completed its W^X seal and instruction-cache synchronization.
    pub fn compile_and_install<H: AuthorizedJitHost>(
        &self,
        host: &mut H,
        code_units: &[u16],
        registers_size: usize,
    ) -> Result<InstalledDexMethod, JitPipelineError> {
        let compiled = compile_dex_method(code_units, registers_size)
            .map_err(JitPipelineError::Compile)?;
        let dex_instructions = compiled.dex_instructions;
        let code = self.install(host, compiled.artifact())
            .map_err(JitPipelineError::Install)?;
        Ok(InstalledDexMethod { code, dex_instructions })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compiles_straight_line_dex_integer_add_to_arm64() {
        // const/4 v1, 3; const/4 v2, 4; add-int v0, v1, v2; return v0
        let method = compile_dex_method(&[0x3112, 0x4212, 0x0090, 0x0201, 0x000f], 3).unwrap();
        assert_eq!(method.architecture, JitArchitecture::Arm64);
        assert_eq!(method.entry_offset, 0);
        assert_eq!(method.dex_instructions, 4);
        assert_eq!(method.code.len() % 4, 0);
        assert_eq!(&method.code[method.code.len() - 4..], &[0xc0, 0x03, 0x5f, 0xd6]);
        assert_eq!(method.artifact().bytes, method.code.as_slice());
    }

    #[test]
    fn compiler_sign_extends_negative_const4_values() {
        // const/4 v1, -1; return v1
        let method = compile_dex_method(&[0xf112, 0x010f], 2).unwrap();
        let words = method.code.chunks_exact(4)
            .map(|bytes| u32::from_le_bytes(bytes.try_into().unwrap()))
            .collect::<Vec<_>>();
        assert_eq!(words[3], 0x529f_ffe1); // MOVZ W1, #0xffff
        assert_eq!(words[4], 0x72bf_ffe1); // MOVK W1, #0xffff, LSL #16
        assert_eq!(*words.last().unwrap(), 0xd65f_03c0); // RET
    }

    #[test]
    fn compiler_rejects_unimplemented_control_flow_instructions() {
        assert_eq!(
            compile_dex_method(&[0x0028], 1),
            Err(JitCompileError::UnsupportedOpcode { pc: 0, opcode: 0x28 })
        );
    }

    #[test]
    fn compiler_rejects_truncated_const_and_invalid_registers() {
        assert_eq!(
            compile_dex_method(&[0x0013], 1),
            Err(JitCompileError::TruncatedInstruction { pc: 0 })
        );
        assert_eq!(
            compile_dex_method(&[0x1112, 0x000f], 1),
            Err(JitCompileError::InvalidRegister { pc: 0, register: 1 })
        );
    }

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
