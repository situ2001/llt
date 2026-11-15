use llt::Timetable;
use std::sync::OnceLock;

static TT: OnceLock<Timetable> = OnceLock::new();

/// The real an.db package (ver 20260904), parsed once and shared by all tests.
pub fn timetable() -> &'static Timetable {
    TT.get_or_init(|| {
        Timetable::parse(&std::fs::read(std::env::var("LLT_REFERENCE_PACKAGE").unwrap()).unwrap())
            .expect("parse an.db")
    })
}
