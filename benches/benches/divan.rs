//! Divan benchmarks for tomet: timing, throughput, and memory allocations.
//!
//! Measures `parse_document` (AST), `parse_cst` (CST), and `format_source_with_config`
//! over synthetic documents of growing size and fixtures.
//!
//! Memory allocations (count and bytes) are automatically tracked via `divan::AllocProfiler`.
//!
//! Run:
//! ```text
//! cargo bench -p tomet-benches --bench divan
//! cargo bench -p tomet-benches --bench divan -- --test   # dry-run verification
//! ```

use divan::{AllocProfiler, Bencher};
use tomet_benches::{Flavor, assert_parses, fixtures, synth_doc};
use tomet_config::{FormatConfig, GroupOrder, PrinterConfig};

#[global_allocator]
static ALLOC: AllocProfiler = AllocProfiler::system();

fn main() {
    divan::main();
}

const SIZES: &[usize] = &[1_000, 10_000, 100_000];

fn default_printer_config() -> PrinterConfig {
    PrinterConfig {
        format: FormatConfig {
            table_adjust_width: Some("auto".to_string()),
            table_max_col_width: Some(50),
            link_group_order: Some(GroupOrder::ContentFirst),
            ..Default::default()
        },
        ..Default::default()
    }
}

#[divan::bench_group]
mod parse {
    use super::*;

    #[divan::bench_group]
    mod document {
        use super::*;

        #[divan::bench(args = SIZES)]
        fn wiki(bencher: Bencher, bytes: usize) {
            let input = synth_doc(Flavor::Wiki, bytes);
            assert_parses(&input);
            bencher
                .counter(divan::counter::BytesCount::new(input.text.len()))
                .bench(|| {
                    let _ = divan::black_box(tomet_parser::parse_document(divan::black_box(
                        &input.text,
                    )));
                });
        }

        #[divan::bench(args = SIZES)]
        fn prose(bencher: Bencher, bytes: usize) {
            let input = synth_doc(Flavor::Prose, bytes);
            assert_parses(&input);
            bencher
                .counter(divan::counter::BytesCount::new(input.text.len()))
                .bench(|| {
                    let _ = divan::black_box(tomet_parser::parse_document(divan::black_box(
                        &input.text,
                    )));
                });
        }

        #[divan::bench]
        fn fixtures_all(bencher: Bencher) {
            let fx = fixtures();
            let total_bytes: usize = fx.iter().map(|i| i.text.len()).sum();
            bencher
                .counter(divan::counter::BytesCount::new(total_bytes))
                .bench(|| {
                    for i in &fx {
                        let _ = divan::black_box(tomet_parser::parse_document(divan::black_box(
                            &i.text,
                        )));
                    }
                });
        }
    }

    #[divan::bench_group]
    mod cst {
        use super::*;

        #[divan::bench(args = SIZES)]
        fn wiki(bencher: Bencher, bytes: usize) {
            let input = synth_doc(Flavor::Wiki, bytes);
            assert_parses(&input);
            bencher
                .counter(divan::counter::BytesCount::new(input.text.len()))
                .bench(|| {
                    divan::black_box(tomet_parser::parse_cst(divan::black_box(&input.text)));
                });
        }

        #[divan::bench(args = SIZES)]
        fn prose(bencher: Bencher, bytes: usize) {
            let input = synth_doc(Flavor::Prose, bytes);
            assert_parses(&input);
            bencher
                .counter(divan::counter::BytesCount::new(input.text.len()))
                .bench(|| {
                    divan::black_box(tomet_parser::parse_cst(divan::black_box(&input.text)));
                });
        }

        #[divan::bench]
        fn fixtures_all(bencher: Bencher) {
            let fx = fixtures();
            let total_bytes: usize = fx.iter().map(|i| i.text.len()).sum();
            bencher
                .counter(divan::counter::BytesCount::new(total_bytes))
                .bench(|| {
                    for i in &fx {
                        divan::black_box(tomet_parser::parse_cst(divan::black_box(&i.text)));
                    }
                });
        }
    }
}

#[divan::bench_group]
mod format {
    use super::*;

    #[divan::bench(args = SIZES)]
    fn wiki(bencher: Bencher, bytes: usize) {
        let input = synth_doc(Flavor::Wiki, bytes);
        assert_parses(&input);
        let config = default_printer_config();
        bencher
            .counter(divan::counter::BytesCount::new(input.text.len()))
            .bench(|| {
                divan::black_box(tomet_formatter::format_source_with_config(
                    divan::black_box(&input.text),
                    &config,
                ));
            });
    }

    #[divan::bench(args = SIZES)]
    fn prose(bencher: Bencher, bytes: usize) {
        let input = synth_doc(Flavor::Prose, bytes);
        assert_parses(&input);
        let config = default_printer_config();
        bencher
            .counter(divan::counter::BytesCount::new(input.text.len()))
            .bench(|| {
                divan::black_box(tomet_formatter::format_source_with_config(
                    divan::black_box(&input.text),
                    &config,
                ));
            });
    }

    #[divan::bench]
    fn fixtures_all(bencher: Bencher) {
        let fx = fixtures();
        let total_bytes: usize = fx.iter().map(|i| i.text.len()).sum();
        let config = default_printer_config();
        bencher
            .counter(divan::counter::BytesCount::new(total_bytes))
            .bench(|| {
                for i in &fx {
                    divan::black_box(tomet_formatter::format_source_with_config(
                        divan::black_box(&i.text),
                        &config,
                    ));
                }
            });
    }
}
