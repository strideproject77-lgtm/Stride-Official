//! Safe ELF identification and ABI inspection primitives.
//! This crate does not load or execute untrusted native code.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElfClass {
    Elf32,
    Elf64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElfArchitecture {
    Arm,
    Aarch64,
    Other(u16),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ElfIdentity {
    pub class: ElfClass,
    pub architecture: ElfArchitecture,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElfError {
    Truncated,
    InvalidMagic,
    UnsupportedClass(u8),
    UnsupportedByteOrder(u8),
    UnsupportedVersion(u8),
}

pub fn inspect_elf_header(bytes: &[u8]) -> Result<ElfIdentity, ElfError> {
    if bytes.len() < 20 {
        return Err(ElfError::Truncated);
    }
    if &bytes[0..4] != b"\x7fELF" {
        return Err(ElfError::InvalidMagic);
    }

    let class = match bytes[4] {
        1 => ElfClass::Elf32,
        2 => ElfClass::Elf64,
        value => return Err(ElfError::UnsupportedClass(value)),
    };

    let little_endian = match bytes[5] {
        1 => true,
        2 => false,
        value => return Err(ElfError::UnsupportedByteOrder(value)),
    };

    if bytes[6] != 1 {
        return Err(ElfError::UnsupportedVersion(bytes[6]));
    }

    let machine = if little_endian {
        u16::from_le_bytes([bytes[18], bytes[19]])
    } else {
        u16::from_be_bytes([bytes[18], bytes[19]])
    };

    let architecture = match machine {
        40 => ElfArchitecture::Arm,
        183 => ElfArchitecture::Aarch64,
        value => ElfArchitecture::Other(value),
    };

    Ok(ElfIdentity { class, architecture })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(class: u8, endian: u8, machine: u16) -> [u8; 20] {
        let mut bytes = [0u8; 20];
        bytes[0..4].copy_from_slice(b"\x7fELF");
        bytes[4] = class;
        bytes[5] = endian;
        bytes[6] = 1;
        let encoded = if endian == 1 { machine.to_le_bytes() } else { machine.to_be_bytes() };
        bytes[18..20].copy_from_slice(&encoded);
        bytes
    }

    #[test]
    fn identifies_arm32_little_endian() {
        let result = inspect_elf_header(&header(1, 1, 40)).unwrap();
        assert_eq!(result, ElfIdentity { class: ElfClass::Elf32, architecture: ElfArchitecture::Arm });
    }

    #[test]
    fn identifies_arm64_little_endian() {
        let result = inspect_elf_header(&header(2, 1, 183)).unwrap();
        assert_eq!(result, ElfIdentity { class: ElfClass::Elf64, architecture: ElfArchitecture::Aarch64 });
    }

    #[test]
    fn rejects_non_elf_input() {
        assert_eq!(inspect_elf_header(b"not an elf header"), Err(ElfError::Truncated));
    }
}
