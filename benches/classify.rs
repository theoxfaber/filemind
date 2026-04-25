use criterion::{criterion_group, criterion_main, Criterion};
use std::path::PathBuf;

use filemind::classifier;
use filemind::config::Config;
use filemind::extractor::Extracted;

fn make_extracted(text: &str, magic: &[u8], has_text: bool) -> Extracted {
    Extracted {
        text: text.to_string(),
        magic: magic.to_vec(),
        has_text,
    }
}

fn bench_classify_invoice_pdf(c: &mut Criterion) {
    let path = PathBuf::from("receipt_invoice.pdf");
    let text = "Invoice #1234\nBill to: John Doe\nTotal due: $500\nPayment: card\nSubtotal: $480";
    let ext = make_extracted(text, b"%PDF-1.4", true);
    let config = Config::default();

    c.bench_function("classify_invoice_pdf", |b| {
        b.iter(|| classifier::classify(&path, &ext, &config))
    });
}

fn bench_classify_code_file(c: &mut Criterion) {
    let path = PathBuf::from("main.rs");
    let text = "fn main() {\n    struct Foo;\n    impl Foo { fn run(&self) {} }\n}";
    let ext = make_extracted(text, b"", true);
    let config = Config::default();

    c.bench_function("classify_code_file", |b| {
        b.iter(|| classifier::classify(&path, &ext, &config))
    });
}

fn bench_classify_no_signals(c: &mut Criterion) {
    let path = PathBuf::from("random.xyz");
    let ext = make_extracted("", b"", false);
    let config = Config::default();

    c.bench_function("classify_no_signals", |b| {
        b.iter(|| classifier::classify(&path, &ext, &config))
    });
}

criterion_group!(
    benches,
    bench_classify_invoice_pdf,
    bench_classify_code_file,
    bench_classify_no_signals,
);
criterion_main!(benches);
