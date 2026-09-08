//! Wall-clock throughput for the longest imported ride in the fixture database.

use std::hint::black_box;

use divan::Bencher;

#[path = "support/load_ride.rs"]
mod load_ride_support;
use load_ride_support::Fixture;

#[divan::bench]
fn load_largest_ride(bencher: Bencher<'_, '_>) {
    let fixture = Fixture::setup_largest();
    bencher.bench(|| black_box(fixture.load()));
    fixture.assert_matches(&fixture.load());
    fixture.shutdown();
}

fn main() {
    divan::main();
}
