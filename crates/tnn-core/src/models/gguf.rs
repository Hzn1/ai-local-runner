//! Minimal, defensive GGUF header parser.
//!
//! We deliberately do not implement a full tensor loader. Before installing a
//! model we only need to prove that the file is a GGUF container of a supported
//! version, that it declares tensors, and that its architecture matches the
//! catalog entry. SHA-256 verification is the primary integrity control; this
//! parser is a second line of defence against a mislabelled or truncated file.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::errors::{AppError, Result};

const MAGIC: &[u8; 4] = b"GGUF";
const MAX_STRING_BYTES: u64 = 16 * 1024 * 1024;
const MAX_ARRAY_ELEMENTS: u64 = 50_000_000;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct GgufInfo {
    pub version: u32,
    pub tensor_count: u64,
    pub metadata_kv_count: u64,
    pub architecture: Option<String>,
    pub file_type: Option<u64>,
    pub file_size: u64,
}

pub fn read_gguf_info(path: &Path) -> Result<GgufInfo> {
    read_gguf_file(File::open(path)?)
}

/// Parse a file granted by a platform document picker without needing a
/// globally accessible filesystem path.
pub fn read_gguf_file(mut file: File) -> Result<GgufInfo> {
    let file_size = file.metadata()?.len();
    let mut magic = [0_u8; 4];
    file.read_exact(&mut magic).map_err(corrupt)?;
    if &magic != MAGIC {
        return Err(AppError::ModelUnsupported {
            detail: "not a GGUF file (bad magic)".into(),
        });
    }

    let version = read_u32(&mut file)?;
    if !(1..=3).contains(&version) {
        return Err(AppError::ModelUnsupported {
            detail: format!("unsupported GGUF version {version}"),
        });
    }

    let tensor_count = read_u64(&mut file)?;
    let metadata_kv_count = read_u64(&mut file)?;
    if tensor_count == 0 {
        return Err(AppError::ModelCorrupt {
            detail: "GGUF declares zero tensors".into(),
        });
    }
    if metadata_kv_count > 1_000_000 {
        return Err(AppError::ModelCorrupt {
            detail: "GGUF metadata count is implausible".into(),
        });
    }

    let mut architecture = None;
    let mut file_type = None;

    for _ in 0..metadata_kv_count {
        let key = read_string(&mut file)?;
        let value_type = read_u32(&mut file)?;

        if key == "general.architecture" && value_type == 8 {
            architecture = Some(read_string(&mut file)?);
        } else if key == "general.file_type" && matches!(value_type, 0..=6 | 10 | 11) {
            file_type = Some(read_unsigned(&mut file, value_type)?);
        } else {
            skip_value(&mut file, value_type, 0)?;
        }

        // Both fields appear at the front of every real GGUF. Stop early so a
        // giant tokenizer array cannot make startup slow.
        if architecture.is_some() && file_type.is_some() {
            break;
        }
    }

    Ok(GgufInfo {
        version,
        tensor_count,
        metadata_kv_count,
        architecture,
        file_type,
        file_size,
    })
}

/// Parse the header and enforce an optional expected architecture.
pub fn validate_gguf(path: &Path, expected_architecture: Option<&str>) -> Result<GgufInfo> {
    let info = read_gguf_info(path)?;
    let architecture = info
        .architecture
        .as_deref()
        .ok_or_else(|| AppError::ModelCorrupt {
            detail: "GGUF has no general.architecture".into(),
        })?;
    if let Some(expected) = expected_architecture {
        if !expected.eq_ignore_ascii_case(architecture) {
            return Err(AppError::ModelUnsupported {
                detail: format!("expected architecture {expected}, found {architecture}"),
            });
        }
    }
    Ok(info)
}

fn corrupt<E: std::fmt::Display>(error: E) -> AppError {
    AppError::ModelCorrupt {
        detail: format!("truncated or unreadable GGUF: {error}"),
    }
}

fn read_bytes(file: &mut File, len: usize) -> Result<Vec<u8>> {
    let mut buffer = vec![0_u8; len];
    file.read_exact(&mut buffer).map_err(corrupt)?;
    Ok(buffer)
}

fn read_u32(file: &mut File) -> Result<u32> {
    let bytes = read_bytes(file, 4)?;
    Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn read_u64(file: &mut File) -> Result<u64> {
    let bytes = read_bytes(file, 8)?;
    Ok(u64::from_le_bytes([
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
    ]))
}

fn read_string(file: &mut File) -> Result<String> {
    let len = read_u64(file)?;
    if len > MAX_STRING_BYTES {
        return Err(AppError::ModelCorrupt {
            detail: "GGUF string is too large".into(),
        });
    }
    let bytes = read_bytes(file, len as usize)?;
    String::from_utf8(bytes).map_err(|_| AppError::ModelCorrupt {
        detail: "GGUF string is not UTF-8".into(),
    })
}

fn read_unsigned(file: &mut File, value_type: u32) -> Result<u64> {
    Ok(match value_type {
        0 => read_bytes(file, 1)?[0] as u64,
        1 => read_bytes(file, 1)?[0] as i8 as i64 as u64,
        2 => {
            let b = read_bytes(file, 2)?;
            u16::from_le_bytes([b[0], b[1]]) as u64
        }
        3 => {
            let b = read_bytes(file, 2)?;
            i16::from_le_bytes([b[0], b[1]]) as i64 as u64
        }
        4 => read_u32(file)? as u64,
        5 => read_u32(file)? as i32 as i64 as u64,
        6 => {
            let b = read_bytes(file, 4)?;
            f32::from_le_bytes([b[0], b[1], b[2], b[3]]) as u64
        }
        7 => read_bytes(file, 1)?[0] as u64,
        10 => read_u64(file)?,
        11 => read_u64(file)? as i64 as u64,
        12 => read_u64(file)?,
        other => {
            return Err(AppError::ModelCorrupt {
                detail: format!("unknown GGUF value type {other}"),
            });
        }
    })
}

fn skip_value(file: &mut File, value_type: u32, depth: u8) -> Result<()> {
    if depth > 4 {
        return Err(AppError::ModelCorrupt {
            detail: "GGUF metadata nesting is too deep".into(),
        });
    }
    let skip = |file: &mut File, n: u64| -> Result<()> {
        file.seek(SeekFrom::Current(n as i64)).map_err(corrupt)?;
        Ok(())
    };
    match value_type {
        0 | 1 | 7 => skip(file, 1),
        2 | 3 => skip(file, 2),
        4..=6 => skip(file, 4),
        8 => {
            let len = read_u64(file)?;
            if len > MAX_STRING_BYTES {
                return Err(AppError::ModelCorrupt {
                    detail: "GGUF string is too large".into(),
                });
            }
            skip(file, len)
        }
        9 => {
            let element_type = read_u32(file)?;
            let count = read_u64(file)?;
            if count > MAX_ARRAY_ELEMENTS {
                return Err(AppError::ModelCorrupt {
                    detail: "GGUF array is implausibly large".into(),
                });
            }
            // Fixed-width elements can be skipped in one seek.
            let fixed = match element_type {
                0 | 1 | 7 => Some(1_u64),
                2 | 3 => Some(2),
                4..=6 => Some(4),
                10..=12 => Some(8),
                _ => None,
            };
            if let Some(width) = fixed {
                skip(file, width.saturating_mul(count))
            } else {
                for _ in 0..count {
                    skip_value(file, element_type, depth + 1)?;
                }
                Ok(())
            }
        }
        10..=12 => skip(file, 8),
        other => Err(AppError::ModelCorrupt {
            detail: format!("unknown GGUF value type {other}"),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn gguf_bytes(version: u32, tensors: u64, kvs: &[(&str, u32, Vec<u8>)]) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(MAGIC);
        data.extend_from_slice(&version.to_le_bytes());
        data.extend_from_slice(&tensors.to_le_bytes());
        data.extend_from_slice(&(kvs.len() as u64).to_le_bytes());
        for (key, value_type, value) in kvs {
            data.extend_from_slice(&(key.len() as u64).to_le_bytes());
            data.extend_from_slice(key.as_bytes());
            data.extend_from_slice(&value_type.to_le_bytes());
            data.extend_from_slice(value);
        }
        data
    }

    fn string_value(value: &str) -> Vec<u8> {
        let mut bytes = (value.len() as u64).to_le_bytes().to_vec();
        bytes.extend_from_slice(value.as_bytes());
        bytes
    }

    fn write_temp(data: &[u8]) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("model.gguf");
        let mut file = File::create(&path).unwrap();
        file.write_all(data).unwrap();
        (dir, path)
    }

    #[test]
    fn parses_architecture_and_file_type() {
        let data = gguf_bytes(
            3,
            291,
            &[
                ("general.architecture", 8, string_value("qwen35")),
                ("general.file_type", 4, 38_u32.to_le_bytes().to_vec()),
            ],
        );
        let (_dir, path) = write_temp(&data);
        let info = read_gguf_info(&path).unwrap();
        assert_eq!(info.version, 3);
        assert_eq!(info.tensor_count, 291);
        assert_eq!(info.architecture.as_deref(), Some("qwen35"));
        assert_eq!(info.file_type, Some(38));
        validate_gguf(&path, Some("qwen35")).unwrap();
        assert!(validate_gguf(&path, Some("llama")).is_err());
    }

    #[test]
    fn rejects_wrong_magic() {
        let (_dir, path) = write_temp(b"NOPE00000000");
        assert!(read_gguf_info(&path).is_err());
    }

    #[test]
    fn rejects_truncated_header() {
        let mut data = gguf_bytes(3, 1, &[("general.architecture", 8, string_value("qwen35"))]);
        data.truncate(data.len() - 2);
        let (_dir, path) = write_temp(&data);
        assert!(read_gguf_info(&path).is_err());
    }

    #[test]
    fn skips_arrays_and_strings() {
        let mut array = Vec::new();
        array.extend_from_slice(&8_u32.to_le_bytes()); // string elements
        array.extend_from_slice(&2_u64.to_le_bytes());
        array.extend_from_slice(&string_value("hello"));
        array.extend_from_slice(&string_value("world"));
        let data = gguf_bytes(
            3,
            2,
            &[
                ("tokenizer.ggml.tokens", 9, array),
                ("general.architecture", 8, string_value("llama")),
                ("general.file_type", 4, 2_u32.to_le_bytes().to_vec()),
            ],
        );
        let (_dir, path) = write_temp(&data);
        let info = read_gguf_info(&path).unwrap();
        assert_eq!(info.architecture.as_deref(), Some("llama"));
    }
}
