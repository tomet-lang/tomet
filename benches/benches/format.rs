//! Formatter throughput: `format_source_with_config` over the fixtures,
//! synthetic documents of growing size, and -- when `TOMET_CORPUS_DIR` is set --
//! real data.
//!
//! The synthetic sizes grow tenfold per step on purpose: the point is the shape
//! of the curve, not one number. `TOMET_BENCH_LARGE=1` adds 1 MB documents.

use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use tomet_benches::{
    Flavor, Input, assert_parses, external_sets, fixtures, synth_doc, total_bytes,
};
use tomet_config::{FormatConfig, GroupOrder, PrinterConfig};

const SIZES: &[usize] = &[1_000, 10_000, 100_000];
const LARGE: usize = 1_000_000;

const LARGE_ENV: &str = "TOMET_BENCH_LARGE";

fn sizes() -> Vec<usize> {
    let mut sizes = SIZES.to_vec();
    if std::env::var_os(LARGE_ENV).is_some_and(|v| !v.is_empty() && v != "0") {
        sizes.push(LARGE);
    }
    sizes
}

fn bench_format(c: &mut Criterion) {
    let mut group = c.benchmark_group("format/source_with_config");

    let config = PrinterConfig {
        format: FormatConfig {
            table_adjust_width: Some("auto".to_string()),
            table_max_col_width: Some(50),
            link_group_order: Some(GroupOrder::ContentFirst),
            ..Default::default()
        },
        ..Default::default()
    };

    // The whole fixture corpus per iteration.
    let fx = fixtures();
    group.throughput(Throughput::Bytes(total_bytes(&fx)));
    group.bench_function("fixtures", |b| {
        b.iter(|| {
            fx.iter().for_each(|i| {
                let _ = black_box(tomet_formatter::format_source_with_config(
                    black_box(&i.text),
                    &config,
                ));
            })
        })
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
                |b, input| {
                    b.iter(|| {
                        let _ = black_box(tomet_formatter::format_source_with_config(
                            black_box(&input.text),
                            &config,
                        ));
                    })
                },
            );
        }
    }

    // Real data, if available: a whole set per iteration.
    group.sample_size(10);
    for (name, files) in external_sets() {
        group.throughput(Throughput::Bytes(total_bytes(&files)));
        group.bench_function(BenchmarkId::new("corpus", &name), |b| {
            b.iter(|| {
                files.iter().for_each(|i: &Input| {
                    let _ = black_box(tomet_formatter::format_source_with_config(
                        black_box(&i.text),
                        &config,
                    ));
                })
            })
        });
    }

    group.finish();
}

criterion_group!(benches, bench_format);
criterion_main!(benches);
