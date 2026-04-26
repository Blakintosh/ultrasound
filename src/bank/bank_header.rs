use std::{mem, ptr};

use crate::{bank::bank_entry::BankEntry, source_asset_cache::Checksum};

const MAGIC: u32 = 592991538u32;
const BAD_MAGIC: u32 = 3735928559u32;

const ZONE_NAME_LENGTH: usize = 64;
const PLATFORM_LENGTH: usize = 8;
const LANGUAGE_LENGTH: usize = 2;

const MAX_DEPENDENCIES: u32 = 8;
const SINGLE_DEPENDENCY_SIZE: u32 = 8;

const BUILD_VERSION: u32 = 2;
const FORMAT_VERSION: u32 = 15;

#[repr(C, packed)]
pub struct BankHeader {
    pub magic: u32,
    pub format_version: u32,
    pub entry_size: u32,
    pub checksum_size: u32,
    pub dependency_size: u32,
    pub entry_count: u32,
    pub dependency_count: u32,
    pub build_version: u32,
    pub file_size: i64,
    pub entry_offset: i64,
    pub converted_checksum_offset: i64,
    pub bank_checksum: Checksum,
    pub dependencies: [u8; 512],
    pub source_checksum_offset: i64,
    pub asset_name_offset: i64,
    pub zone_name: [u8; ZONE_NAME_LENGTH],
    pub platform: [u8; PLATFORM_LENGTH],
    pub language: [u8; LANGUAGE_LENGTH],
    pub converted_asset_version: i32,
    pub padding0: i32,
    pub _reserved: [u8; 1366],
}

const _: () = assert!(std::mem::size_of::<BankHeader>() == 2048);

impl BankHeader {
    pub fn new() -> Self {
        unsafe { std::mem::zeroed() }
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        let expected = std::mem::size_of::<Self>();
        if bytes.len() < expected {
            return Err(format!(
                "BankHeader buffer too small: {} < {}",
                bytes.len(),
                expected
            ));
        }
        Ok(unsafe { std::ptr::read_unaligned(bytes.as_ptr() as *const Self) })
    }

    pub fn to_bytes(&self) -> [u8; 2048] {
        let mut out = [0u8; 2048];

        unsafe {
            ptr::copy_nonoverlapping(
                self as *const Self as *const u8,
                out.as_mut_ptr(),
                mem::size_of::<Self>(),
            );
        }
        out
    }

    pub fn set_zone_name(&mut self, name: &str) {
        let ascii = name.as_bytes();
        let len = ascii.len().min(ZONE_NAME_LENGTH);

        self.zone_name[..len].copy_from_slice(&ascii[..len]);
    }

    pub fn set_platform(&mut self, platform: &str) {
        let ascii = platform.as_bytes();
        let len = ascii.len().min(PLATFORM_LENGTH);

        self.platform[..len].copy_from_slice(&ascii[..len]);
    }

    pub fn set_language(&mut self, language: &str) {
        let ascii = language.as_bytes();
        let len = ascii.len().min(LANGUAGE_LENGTH);

        self.language[..len].copy_from_slice(&ascii[..len]);
    }

    pub fn set_dependencies(
        &mut self,
        file_name: &str,
        dependencies: &[&str],
    ) -> Result<(), String> {
        let dependency_count = dependencies.len();
        if dependency_count > MAX_DEPENDENCIES as usize {
            return Err(format!(
                "File {} has too many bank dependencies. Maximum of {} allowed.",
                file_name, MAX_DEPENDENCIES
            ));
        }

        self.dependencies.fill(0);

        for i in 0..dependency_count {
            if dependencies[i].len() > 64 {
                return Err(format!(
                    "Bank dependency {} on file {} is too long",
                    i, file_name
                ));
            }

            let bytes = dependencies[i].as_bytes();
            let offset = i * 64;
            let len = bytes.len().min(64);

            self.dependencies[offset..offset + len].copy_from_slice(&bytes[..len]);
        }

        Ok(())
    }

    pub fn invalidate(&mut self) {
        self.magic = BAD_MAGIC;
        self.build_version = BUILD_VERSION;
        self.format_version = FORMAT_VERSION;
        self.file_size = 0;
        self.entry_count = 0;
        self.entry_offset = -1;
        self.converted_checksum_offset = -1;
        self.entry_size = 0;
        self.checksum_size = 0;
        self.dependency_size = 0;
        self.dependency_count = 0;
        self.bank_checksum = Checksum::from_data(&[])
    }

    pub fn fix(&mut self, file_count: i32, file_size: i64) {
        self.magic = MAGIC;
        self.build_version = BUILD_VERSION;
        self.format_version = FORMAT_VERSION;
        self.entry_count = file_count as u32;
        self.entry_size = size_of::<BankEntry>() as u32;
        self.checksum_size = size_of::<Checksum>() as u32;
        self.dependency_size = MAX_DEPENDENCIES * SINGLE_DEPENDENCY_SIZE;
        self.dependency_count = MAX_DEPENDENCIES;
        self.file_size = file_size;
    }

    pub fn is_sane(&self, file_size: i64) -> Result<(), String> {
        if self.magic != MAGIC {
            return Err("Invalid magic".to_string());
        }
        if self.build_version != 0 && self.build_version != BUILD_VERSION {
            return Err("Mismatched build version".to_string());
        }
        if self.format_version != 0 && self.format_version != FORMAT_VERSION {
            return Err("Mismatched format version".to_string());
        }
        if self.file_size != 0 && self.file_size != file_size {
            return Err("Invalid file size".to_string());
        }
        if self.entry_size as usize != size_of::<BankEntry>() {
            return Err("Invalid entry size".to_string());
        }
        if self.checksum_size as usize != size_of::<Checksum>() {
            return Err("Invalid checksum size".to_string());
        }
        if self.dependency_size != 64 {
            return Err("Invalid dependency size".to_string());
        }
        if self.dependency_count != 8 {
            return Err("Invalid dependency count".to_string());
        }
        if self.entry_offset < 0
            || self.entry_offset > file_size
            || (self.entry_offset as usize) < size_of::<BankHeader>()
        {
            return Err("Invalid entry offset".to_string());
        }
        if self.converted_checksum_offset < 0
            || self.converted_checksum_offset > file_size
            || (self.converted_checksum_offset as usize) < size_of::<BankHeader>()
        {
            return Err("Invalid checksum offset".to_string());
        }
        Ok(())
    }
}
