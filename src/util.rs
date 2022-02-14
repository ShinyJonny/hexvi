use std::io::{Read, Write, Seek, SeekFrom};
use std::fs::File;
use std::process::{Command, Output, Stdio};

/// Reads a file into a Vec of bytes.
pub fn freadn_to_vec(file: &mut File, size: usize) -> Result<Vec<u8>, std::io::Error>
{
    let orig_position = file.seek(SeekFrom::Current(0))?;

    let mut all_read = 0;
    let mut vector: Vec<u8> = Vec::new();
    let mut buf: [u8; 512]  = [0; 512];

    loop {
        let read = file.read(&mut buf)?;
        // EOF
        if read == 0 {
            break
        } else {
            // Still more data to read.
            if all_read + read <= size {
                all_read += read;
                vector.extend_from_slice(&buf[..read]);
            // If we already read more than requested.
            } else {
                vector.extend(&buf[..(size - all_read)]);
                break;
            }
        }
    }

    // Reset the seek back to its position.
    file.seek(SeekFrom::Start(orig_position))?;

    Ok(vector)
}

/// Converts a byte to its canonical representation.
pub fn check_printable(byte: u8) -> bool
{
    if byte >= 0x20 && byte < 0x7f {
        true
    } else {
        false
    }
}

/// Starts a process, writes data to its stdin, and returns its output.
pub fn popen(process: &str, args: &[&str], data: Vec<u8>) -> Result<Output, i32>
{
    let mut process = match Command::new(process)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn() {
            Err(_) => return Err(1),
            Ok(v) => v
        };

    let mut stdin = match process.stdin.take() {
        None => return Err(2),
        Some(v) => v
    };

    std::thread::spawn(move || {
        stdin.write_all(data.as_slice())
            .expect("failed to write to the parser's stdin");
        stdin.flush()
            .expect("failed to flush the parser's stdin");
    });

    let output = match process.wait_with_output() {
        Err(_) => return  Err(4),
        Ok(o) => o,
    };

    Ok(output)
}
