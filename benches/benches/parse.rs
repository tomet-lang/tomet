//! Parser throughput: `parse_document` (AST) and `parse_cst` (lossless tree)
//! over the fixtures, synthetic documents of growing size, and -- when
//! `TOMET_CORPUS_DIR` is set -- real data.
//!
//! The synthetic sizes grow tenfold per step on purpose: the point is the shape
//! of the curve, not one number. `TOMET_BENCH_LARGE=1` adds 1 MB documents.

use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use tomet_benches::{
    Flavor, Input, assert_parses, external_sets, fixtures, synth_doc, total_bytes,
};

const SIZES: &[usize] = &[1_000, 10_000, 100_000];
const LARGE: usize = 1_000_000;

/// Environment variable that adds the 1 MB documents. Off by default: while
/// parsing is super-linear in document size one 1 MB iteration takes tens of
/// seconds, which makes the whole suite take far too long to run casually.
const LARGE_ENV: &str = "TOMET_BENCH_LARGE";

fn sizes() -> Vec<usize> {
    let mut sizes = SIZES.to_vec();
    if std::env::var_os(LARGE_ENV).is_some_and(|v| !v.is_empty() && v != "0") {
        sizes.push(LARGE);
    }
    sizes
}

type ParseFn = fn(&str);

fn document(src: &str) {
    let _ = black_box(tomet_parser::parse_document(black_box(src)));
}

fn cst(src: &str) {
    black_box(tomet_parser::parse_cst(black_box(src)));
}

fn bench_api(c: &mut Criterion, api: &str, parse: ParseFn) {
    let mut group = c.benchmark_group(format!("parse/{api}"));

    // The whole fixture corpus per iteration.
    let fx = fixtures();
    group.throughput(Throughput::Bytes(total_bytes(&fx)));
    group.bench_function("fixtures", |b| {
        b.iter(|| fx.iter().for_each(|i| parse(&i.text)))
    });

    // One document per size.
    for flavor in [Flavor::Wiki, Flavor::Prose] {
        for bytes in sizes() {
            let input = synth_doc(flavor, bytes);
            assert_parses(&input);
            group.throughput(Throughput::Bytes(input.text.len() as u64));
            group.sample_size(if bytes >= LARGE { 10 } else { 50 });
            group.bench_with_input(
                BenchmarkId::new(flavor.label(), bytes),
                &input,
                |b, input| b.iter(|| parse(&input.text)),
            );
        }
    }
    // Real data, if available: a whole set per iteration.
    group.sample_size(10);
    for (name, files) in external_sets() {
        group.throughput(Throughput::Bytes(total_bytes(&files)));
        group.bench_function(BenchmarkId::new("corpus", &name), |b| {
            b.iter(|| files.iter().for_each(|i: &Input| parse(&i.text)))
        });
    }

    group.finish();
}

fn parse(c: &mut Criterion) {
    bench_api(c, "document", document);
    bench_api(c, "cst", cst);
}

criterion_group!(benches, parse);
criterion_main!(benches);
