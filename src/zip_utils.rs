// It is a lot less code to use the Zip crate but it increases the executable
// size significantly.

use flate2::read::DeflateDecoder;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::Path;

pub struct ZipEntry {
    pub file_name: String,
    pub compressed_size: u32,
    pub compression_method: u16,
    pub local_header_offset: u32,
}

pub fn parse_central_directory(buffer: &[u8]) -> io::Result<Vec<ZipEntry>> {
    let mut entries = Vec::new();
    let mut i = 0;
    const DEFLATE_SIGNATURE: &[u8] = b"\x50\x4b\x01\x02";

    while i + 4 <= buffer.len() {
        if &buffer[i..i + 4] == DEFLATE_SIGNATURE {
            if i + 46 > buffer.len() {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "Incomplete central directory header",
                ));
            }

            let compression_method = u16::from_le_bytes(buffer[i + 10..i + 12
                    ].try_into().unwrap());
            let compressed_size = u32::from_le_bytes(buffer[i + 20..i + 24
                    ].try_into().unwrap());

            let file_name_length =
                u16::from_le_bytes(buffer[i + 28..i + 30].try_into().unwrap()) 
                        as usize;
            let extra_field_length =
                u16::from_le_bytes(buffer[i + 30..i + 32].try_into().unwrap()) 
                        as usize;
            let file_comment_length =
                u16::from_le_bytes(buffer[i + 32..i + 34].try_into().unwrap()) 
                        as usize;
            let local_header_offset =
                u32::from_le_bytes(buffer[i + 42..i + 46].try_into().unwrap());

            let header_size = 46;
            let total_len = file_name_length + extra_field_length + 
                    file_comment_length;
            let start = i + header_size;
            let end = start + total_len;

            if end > buffer.len() {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "Incomplete file name or extra fields",
                ));
            }

            let file_name =
                String::from_utf8_lossy(&buffer[start..start + 
                        file_name_length]).to_string();

            entries.push(ZipEntry {
                file_name,
                compressed_size,
                compression_method,
                local_header_offset,
            });

            i = end;
        } else {
            i += 1;
        }
    }

    Ok(entries)
}

pub fn extract_file(entry: &ZipEntry, buffer: &[u8], extract_to_dir: &Path) -> 
        io::Result<()> {
    let offset = entry.local_header_offset as usize;

    if offset + 30 > buffer.len() {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "Incomplete local header",
        ));
    }

    if &buffer[offset..offset + 4] != b"\x50\x4b\x03\x04" {
        eprintln!(
            "Invalid local header signature at offset {}: {:?}",
            offset,
            &buffer[offset..offset + 4]
        );
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Invalid local file header signature",
        ));
    }

    let file_name_length =
        u16::from_le_bytes(buffer[offset + 26..offset + 28].try_into().unwrap(
                )) as usize;
    let extra_field_length =
        u16::from_le_bytes(buffer[offset + 28..offset + 30].try_into().unwrap(
                )) as usize;

    let data_start = offset + 30 + file_name_length + extra_field_length;
    let data_end = data_start + entry.compressed_size as usize;

    if data_end > buffer.len() {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "File data exceeds buffer",
        ));
    }

    let file_data = &buffer[data_start..data_end];
    let path = extract_to_dir.join(&entry.file_name);

    // Handle directories
    if entry.file_name.ends_with('/') {
        fs::create_dir_all(&path)?;
        return Ok(());
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut output = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&path)?;

    match entry.compression_method {
        0 => {
            // Stored (no compression)
            output.write_all(file_data)?;
        }
        8 => {
            // Deflate compression
            let mut decoder = DeflateDecoder::new(file_data);
            io::copy(&mut decoder, &mut output)?;
        }
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                format!(
                    "Unsupported compression method: {}",
                    entry.compression_method
                ),
            ));
        }
    }
    Ok(())
}
