#![cfg(feature = "real-data-tests")]

use llt::Timetable;
use std::io::{Cursor, Read, Write};

fn with_member(name: &str, mutate: impl FnOnce(&mut Vec<u8>)) -> Vec<u8> {
    let mut archive = zip::ZipArchive::new(Cursor::new(
        std::fs::read(std::env::var("LLT_CURRENT_PACKAGE").unwrap()).unwrap(),
    ))
    .unwrap();
    let mut bytes = Vec::new();
    archive
        .by_name(name)
        .unwrap()
        .read_to_end(&mut bytes)
        .unwrap();
    mutate(&mut bytes);
    let mut output = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for index in 0..archive.len() {
        let member = archive.by_index(index).unwrap();
        if member.name() == name {
            output
                .start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            output.write_all(&bytes).unwrap();
        } else {
            output.raw_copy_file(member).unwrap();
        }
    }
    output.finish().unwrap().into_inner()
}

fn rejected(package: &[u8], expected: &str) {
    let error = Timetable::parse(package)
        .err()
        .expect("damaged package must be rejected");
    assert!(error.to_string().contains(expected), "{error}");
}

#[test]
fn rejects_empty_timetable_record() {
    rejected(
        &with_member("T1.dat", |bytes| {
            let count = bytes[15] as usize * 255 + bytes[16] as usize;
            bytes.drain(17..17 + 7 * count);
            bytes[15] = 0;
            bytes[16] = 0;
        }),
        "no stops",
    );
}

#[test]
fn rejects_invalid_clocks_before_deriving_days() {
    for (offset, value) in [(19, 24), (20, 60)] {
        rejected(
            &with_member("T1.dat", |bytes| bytes[offset] = value),
            "invalid clock",
        );
    }
}

#[test]
fn rejects_truncated_train_and_station_shards() {
    rejected(&with_member("T1.dat", |bytes| bytes.truncate(17)), "T1.dat");
    rejected(&with_member("S1.dat", |bytes| bytes.truncate(3)), "S1.dat");
}
